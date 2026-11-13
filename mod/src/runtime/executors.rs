//! `java.util.concurrent` executors, run on the runtime's own thread.
//!
//! A mod that does its init, update check or metric work on a pool builds it
//! with an `Executors` factory and hands the work to `execute`/`submit`. The
//! bounded runtime has one thread, so the task runs inline, before the call
//! returns; the `Future` a `submit` answers is already complete, and
//! `shutdown`/`isShutdown`/`awaitTermination` report the state the shutdown
//! calls set.
//!
//! Named departures, named in the PR that added this file:
//! * `execute`/`submit`/`schedule` run the task **immediately, on the calling
//!   thread**, not concurrently on a pool thread. A jar that reads a flag its
//!   task sets *after* `execute` returns sees it already set (in Java the task
//!   may not have started); the answers to `get`, `isDone` and `shutdown` are
//!   the completed ones.
//! * a scheduled task runs at once instead of after its delay.
//! * `awaitTermination` answers the current shutdown state instead of waiting
//!   (the runtime never blocks); `scheduleAtFixedRate`/`scheduleWithFixedDelay`
//!   are not modelled (a periodic task would run to the budget), so they stay
//!   counted unknowns.
//! * a `ThreadFactory` is modelled only as far as making an unstarted
//!   `Thread`; the pool never creates one, and `Thread.start` stays counted.

use super::*;

/// The synthetic field an executor marker keeps its shutdown flag under.
const SHUTDOWN_FIELD: &str = "#executorShutdown";
/// The synthetic field a future marker keeps its completed value under.
const FUTURE_VALUE: &str = "#executorValue";
/// `Executors.callable(runnable)` and `callable(runnable, result)` markers.
const CALLABLE_PREFIX: &str = "executor-callable/";
const CALLABLE_RESULT_PREFIX: &str = "executor-callable-result/";
const THREAD_FACTORY: &str = "executor-thread-factory";

const EXECUTOR: &str = "java/util/concurrent/Executor";
const EXECUTOR_SERVICE: &str = "java/util/concurrent/ExecutorService";
const SCHEDULED_EXECUTOR_SERVICE: &str = "java/util/concurrent/ScheduledExecutorService";
const SCHEDULED_FUTURE: &str = "java/util/concurrent/ScheduledFuture";
const FUTURE: &str = "java/util/concurrent/Future";
const RUNNABLE_RUN: &str = "()V";
const CALLABLE_CALL: &str = "()Ljava/lang/Object;";

fn run_ref() -> MethodRef {
    MethodRef {
        owner: "java/lang/Runnable".into(),
        name: "run".into(),
        descriptor: RUNNABLE_RUN.into(),
    }
}

/// Answer one executor, factory or future call, or `None`.
pub(super) fn invoke<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    match r.owner.as_str() {
        "java/util/concurrent/Executors" => invoke_executors(rt, r, args),
        "java/util/concurrent/Callable" => invoke_callable(rt, r, receiver),
        EXECUTOR | EXECUTOR_SERVICE | SCHEDULED_EXECUTOR_SERVICE => {
            invoke_executor(rt, r, receiver, args)
        }
        FUTURE | SCHEDULED_FUTURE | "java/util/concurrent/RunnableFuture" => {
            invoke_future(rt, r, receiver)
        }
        _ => Ok(None),
    }
}

fn invoke_executors<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let mut pool = || rt.new_object("executor");
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "newSingleThreadExecutor"
            | "newCachedThreadPool"
            | "newSingleThreadScheduledExecutor"
            | "newWorkStealingPool"
            // A virtual thread per task is, for this runtime, the same
            // single-threaded service; the departure is the one above.
            | "newVirtualThreadPerTaskExecutor",
            "()Ljava/util/concurrent/ExecutorService;"
            | "()Ljava/util/concurrent/ScheduledExecutorService;",
        ) => pool(),
        (
            "newSingleThreadExecutor"
            | "newCachedThreadPool"
            | "newSingleThreadScheduledExecutor"
            | "newFixedThreadPool"
            | "newScheduledThreadPool",
            _,
        ) if r
            .descriptor
            .starts_with("(Ljava/util/concurrent/ThreadFactory;)")
            || r.descriptor.contains("ThreadFactory") =>
        {
            pool()
        }
        ("newFixedThreadPool", _)
        | ("newWorkStealingPool", "(I)Ljava/util/concurrent/ExecutorService;") => {
            // `newFixedThreadPool`'s `ThreadPoolExecutor` refuses a
            // non-positive maximum; the JDK 27 message is the real one.
            let Some(Value::Int(size)) = args.first() else {
                return Err(NativeError::Metadata(
                    "Executors.newFixedThreadPool needs an int".into(),
                ));
            };
            if *size <= 0 {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/IllegalArgumentException".into(),
                    message: Some("maximumPoolSize must be positive".into()),
                    frame: None,
                });
            }
            pool()
        }
        ("newScheduledThreadPool", _) => {
            let Some(Value::Int(size)) = args.first() else {
                return Err(NativeError::Metadata(
                    "Executors.newScheduledThreadPool needs an int".into(),
                ));
            };
            if *size < 0 {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/IllegalArgumentException".into(),
                    message: None,
                    frame: None,
                });
            }
            pool()
        }
        // `Executors.callable(runnable)` and `callable(runnable, result)`.
        (
            "callable",
            "(Ljava/lang/Runnable;)Ljava/util/concurrent/Callable;"
            | "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Callable;",
        ) => {
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            let task = value_marker(task);
            let result = args.get(1).map(value_marker).unwrap_or_default();
            Value::Ref(Some(if r.descriptor.contains("Ljava/lang/Object;)") {
                format!("{CALLABLE_RESULT_PREFIX}{task}|{result}")
            } else {
                format!("{CALLABLE_PREFIX}{task}")
            }))
        }
        ("defaultThreadFactory" | "privilegedThreadFactory", _) => {
            Value::Ref(Some(THREAD_FACTORY.into()))
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn invoke_callable<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
) -> Result<Option<Value>, NativeError> {
    if (r.name.as_str(), r.descriptor.as_str()) != ("call", CALLABLE_CALL) {
        return Ok(None);
    }
    let Some(marker) = receiver.and_then(expect_ref) else {
        return Err(native_npe());
    };
    if let Some(rest) = marker.strip_prefix(CALLABLE_PREFIX) {
        let task = parse_value_marker(rest)?;
        run_runnable(rt, &task)?;
        return Ok(Some(Value::Ref(None)));
    }
    if let Some(rest) = marker.strip_prefix(CALLABLE_RESULT_PREFIX) {
        let (task, result) = rest.split_once('|').unwrap_or((rest, ""));
        run_runnable(rt, &parse_value_marker(task)?)?;
        return Ok(Some(parse_value_marker(result)?));
    }
    Ok(None)
}

fn invoke_executor<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(marker) = receiver.and_then(expect_ref) else {
        return Err(native_npe());
    };
    let marker = marker.to_owned();
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        ("execute", "(Ljava/lang/Runnable;)V") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            run_runnable(rt, task)?;
            Value::Void
        }
        ("submit", "(Ljava/lang/Runnable;)Ljava/util/concurrent/Future;") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            run_runnable(rt, task)?;
            complete_future(rt, Value::Ref(None))
        }
        ("submit", "(Ljava/lang/Runnable;Ljava/lang/Object;)Ljava/util/concurrent/Future;") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            run_runnable(rt, task)?;
            complete_future(rt, args.get(1).cloned().unwrap_or(Value::Ref(None)))
        }
        ("submit", "(Ljava/util/concurrent/Callable;)Ljava/util/concurrent/Future;") => {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            let result = run_callable(rt, task)?;
            complete_future(rt, result)
        }
        ("schedule", descriptor)
            if descriptor.starts_with("(Ljava/lang/Runnable;J")
                || descriptor.starts_with("(Ljava/util/concurrent/Callable;J") =>
        {
            reject_after_shutdown(rt, &marker)?;
            let Some(task) = args.first() else {
                return Err(native_npe());
            };
            let result = if r.descriptor.starts_with("(Ljava/util/concurrent/Callable;") {
                run_callable(rt, task)?
            } else {
                run_runnable(rt, task)?;
                Value::Ref(None)
            };
            complete_future(rt, result)
        }
        ("shutdown" | "close", "()V") => {
            rt.set_executor_shutdown(&marker);
            Value::Void
        }
        ("shutdownNow", _) => {
            rt.set_executor_shutdown(&marker);
            collection_value("list", &[])
        }
        ("isShutdown" | "isTerminated", "()Z") => {
            Value::Int(i32::from(rt.executor_shutdown(&marker)))
        }
        // The runtime never blocks: a shutdown pool is already terminated.
        ("awaitTermination", "(JLjava/util/concurrent/TimeUnit;)Z") => {
            Value::Int(i32::from(rt.executor_shutdown(&marker)))
        }
        ("toString", "()Ljava/lang/String;") => Value::Ref(Some("executor".into())),
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// `ThreadFactory.newThread(runnable)` hands out an unstarted `Thread` marker.
pub(super) fn invoke_thread_factory<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    _args: &[Value],
) -> Option<Value> {
    if !receiver
        .and_then(expect_ref)
        .is_some_and(|marker| marker == THREAD_FACTORY)
        || (r.name.as_str(), r.descriptor.as_str())
            != ("newThread", "(Ljava/lang/Runnable;)Ljava/lang/Thread;")
    {
        return None;
    }
    Some(rt.new_object("java/lang/Thread"))
}

fn invoke_future<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
) -> Result<Option<Value>, NativeError> {
    let Some(marker) = receiver.and_then(expect_ref) else {
        return Err(native_npe());
    };
    Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("get", "()Ljava/lang/Object;")
        | ("get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;") => rt
            .fields
            .get(&format!("{marker}|{FUTURE_VALUE}"))
            .cloned()
            .unwrap_or(Value::Ref(None)),
        // The task ran before the future was answered, so it is always done
        // and cannot be cancelled any more.
        ("isDone", "()Z") => Value::Int(1),
        ("isCancelled", "()Z") | ("cancel", "(Z)Z") => Value::Int(0),
        ("toString", "()Ljava/lang/String;") => Value::Ref(Some("future".into())),
        _ => return Ok(None),
    }))
}

fn reject_after_shutdown<H: Host>(rt: &Runtime<'_, H>, marker: &str) -> Result<(), NativeError> {
    if rt.executor_shutdown(marker) {
        return Err(NativeError::UncaughtException {
            class: "java/util/concurrent/RejectedExecutionException".into(),
            message: None,
            frame: None,
        });
    }
    Ok(())
}

/// A completed future holding `value`.
fn complete_future<H: Host>(rt: &mut Runtime<'_, H>, value: Value) -> Value {
    let future = rt.new_object("future");
    if let Some(marker) = expect_ref(&future) {
        rt.fields.insert(format!("{marker}|{FUTURE_VALUE}"), value);
    }
    future
}

/// Run one `Runnable` value: a lambda marker runs its body, a jar object
/// dispatches its own `run`, and anything else is the host's call (counted).
fn run_runnable<H: Host>(rt: &mut Runtime<'_, H>, task: &Value) -> Result<(), NativeError> {
    if rt.invoke_callback(task, &[])?.is_some() {
        return Ok(());
    }
    let Some(marker) = expect_ref(task) else {
        return Err(native_npe());
    };
    if let Some((class, _)) = marker.split_once('#')
        && rt
            .class(class)
            .is_some_and(|file| file.method("run", RUNNABLE_RUN).is_some())
    {
        let class = class.to_owned();
        rt.invoke(&class, "run", RUNNABLE_RUN, Some(task.clone()), &[])?;
        return Ok(());
    }
    rt.host.invoke_instance(&run_ref(), task, &[])?;
    Ok(())
}

/// Run one `Callable` value and answer what it returned.
fn run_callable<H: Host>(rt: &mut Runtime<'_, H>, task: &Value) -> Result<Value, NativeError> {
    if let Some(value) = rt.invoke_callback(task, &[])? {
        return Ok(value);
    }
    let Some(marker) = expect_ref(task) else {
        return Err(native_npe());
    };
    if let Some((class, _)) = marker.split_once('#')
        && rt
            .class(class)
            .is_some_and(|file| file.method("call", CALLABLE_CALL).is_some())
    {
        let class = class.to_owned();
        return rt.invoke(&class, "call", CALLABLE_CALL, Some(task.clone()), &[]);
    }
    rt.host
        .invoke_instance(
            &MethodRef {
                owner: "java/util/concurrent/Callable".into(),
                name: "call".into(),
                descriptor: CALLABLE_CALL.into(),
            },
            task,
            &[],
        )
        .map(|value| {
            if value == Value::Void {
                Value::Ref(None)
            } else {
                value
            }
        })
}

impl<H: Host> Runtime<'_, H> {
    fn executor_shutdown(&self, marker: &str) -> bool {
        matches!(
            self.fields.get(&format!("{marker}|{SHUTDOWN_FIELD}")),
            Some(Value::Int(flag)) if *flag != 0
        )
    }

    fn set_executor_shutdown(&mut self, marker: &str) {
        self.fields
            .insert(format!("{marker}|{SHUTDOWN_FIELD}"), Value::Int(1));
    }
}

fn native_npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}
