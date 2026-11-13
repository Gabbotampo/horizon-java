//! `CountDownLatch` and a constructed `ThreadPoolExecutor`, ported from JDK 25.
//!
//! Serilum's `collective` (and every mod built on it) makes its update-check
//! pool with `new ThreadPoolExecutor(...)`, calls `allowCoreThreadTimeOut(true)`
//! on it, and gates the check on a `CountDownLatch` its own init counts down.
//! Before this model, `<init>` kept no state and the latch's `getCount`/`await`
//! and the pool's `allowCoreThreadTimeOut` were counted unknowns.
//!
//! What JDK 25 does (`java.base/java/util/concurrent`, read from `src.zip` of
//! Temurin 25.0.5+8):
//!
//! * `CountDownLatch(int)`: `count < 0` throws `IllegalArgumentException("count < 0")`.
//!   `countDown` lowers a positive count by one and does nothing at zero;
//!   `getCount` answers it as a `long`.
//! * `await(long, TimeUnit)`: `unit.toNanos(timeout)` first (a `null` unit is a
//!   `NullPointerException`), then AQS `tryAcquireSharedNanos`: `true` at count
//!   zero, else `false` at once when the timeout is not positive, else wait.
//! * `await()`: returns at count zero, else waits without bound.
//! * `ThreadPoolExecutor`'s four constructors share the seven-argument one: the
//!   size and keep-alive checks in that order with their messages, then
//!   `requireNonNull` on the unit, queue, factory and handler (the message is the
//!   parameter name); it keeps `unit.toNanos(keepAliveTime)`.
//! * `allowCoreThreadTimeOut(true)` with a keep-alive of zero throws
//!   `IllegalArgumentException("Core threads must have nonzero keep alive times")`;
//!   `allowsCoreThreadTimeOut` answers the flag.
//!
//! The waits are the one place this runtime cannot follow the JDK: it has one
//! thread, so a latch above zero would be counted down, if ever, by code that has
//! not run yet. A wait that would block (`await()` above zero, or a timed
//! `await` above zero with a positive timeout) therefore stays a counted unknown
//! that answers the neutral value, never a guessed `true` or `false`.
//!
//! Work handed to a pool through an `ExecutorService`-typed call runs in the
//! executor model (`executors.rs`), with its named departure (a task runs
//! inline). The same call named on a `ThreadPoolExecutor`-typed receiver stays
//! counted, as before: a pool's own worker loop would never return. `toString`
//! (it prints the identity hash) and the pool-size setters stay counted too.

use super::*;

const LATCH: &str = "java/util/concurrent/CountDownLatch";
const POOL: &str = "java/util/concurrent/ThreadPoolExecutor";
/// The synthetic fields a latch and a pool keep their state under.
const LATCH_COUNT: &str = "#latchCount";
const POOL_KEEP_ALIVE: &str = "#poolKeepAliveNanos";
const POOL_CORE_TIMEOUT: &str = "#poolCoreTimeout";

/// Answer one `CountDownLatch` or `ThreadPoolExecutor` call, or `None`.
pub(super) fn invoke<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    match r.owner.as_str() {
        LATCH => invoke_latch(rt, r, receiver, args),
        POOL => invoke_pool(rt, r, receiver, args),
        _ => Ok(None),
    }
}

fn state_key(marker: &str, field: &str) -> String {
    format!("{marker}|{field}")
}

fn invoke_latch<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(marker) = receiver
        .and_then(expect_ref)
        .map(|marker| marker.to_string())
    else {
        return Ok(None);
    };
    let key = state_key(&marker, LATCH_COUNT);
    if (r.name.as_str(), r.descriptor.as_str()) == ("<init>", "(I)V") {
        let count = expect_int(r, args.first().unwrap_or(&Value::Int(0)))?;
        if count < 0 {
            return Err(illegal_argument("count < 0"));
        }
        rt.fields.insert(key, Value::Long(i64::from(count)));
        return Ok(Some(Value::Void));
    }
    // A latch this model did not construct (a host answer, a subclass) has no
    // count to read: leave it to the counted path.
    let Some(Value::Long(count)) = rt.fields.get(&key).cloned() else {
        return Ok(None);
    };
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        ("getCount", "()J") => Value::Long(count),
        ("countDown", "()V") => {
            if count > 0 {
                rt.fields.insert(key, Value::Long(count - 1));
            }
            Value::Void
        }
        ("await", "()V") if count == 0 => Value::Void,
        ("await", "(JLjava/util/concurrent/TimeUnit;)Z") => {
            let timeout = expect_long(r, args.first().unwrap_or(&Value::Long(0)))?;
            let Some(nanos) = to_nanos(args.get(1), timeout)? else {
                return Ok(None);
            };
            if count == 0 {
                Value::Int(1)
            } else if nanos <= 0 {
                Value::Int(0)
            } else {
                // A positive wait on a latch above zero: see the module doc.
                return Ok(None);
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn invoke_pool<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(marker) = receiver
        .and_then(expect_ref)
        .map(|marker| marker.to_string())
    else {
        return Ok(None);
    };
    let keep_alive_key = state_key(&marker, POOL_KEEP_ALIVE);
    let timeout_key = state_key(&marker, POOL_CORE_TIMEOUT);
    if r.name == "<init>" {
        let Some(keep_alive) = construct_pool(r, args)? else {
            return Ok(None);
        };
        rt.fields.insert(keep_alive_key, Value::Long(keep_alive));
        rt.fields.insert(timeout_key, Value::Int(0));
        return Ok(Some(Value::Void));
    }
    // Only a pool this model constructed has a keep-alive to check.
    let Some(Value::Long(keep_alive)) = rt.fields.get(&keep_alive_key).cloned() else {
        return Ok(None);
    };
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        ("allowCoreThreadTimeOut", "(Z)V") => {
            let allow = expect_int(r, args.first().unwrap_or(&Value::Int(0)))? != 0;
            if allow && keep_alive <= 0 {
                return Err(illegal_argument(
                    "Core threads must have nonzero keep alive times",
                ));
            }
            rt.fields.insert(timeout_key, Value::Int(i32::from(allow)));
            Value::Void
        }
        ("allowsCoreThreadTimeOut", "()Z") => rt
            .fields
            .get(&timeout_key)
            .cloned()
            .unwrap_or(Value::Int(0)),
        // `execute`/`submit` named on a `ThreadPoolExecutor`-typed receiver
        // stay counted: running them inline (the executor model's departure)
        // would run a pool's endless worker loop, such as Distant Horizons'
        // log ticker, to the budget on the one thread this runtime has.
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// Check a `ThreadPoolExecutor` constructor's arguments the way JDK 25's
/// seven-argument constructor does and answer `unit.toNanos(keepAliveTime)`,
/// or `None` for a descriptor this model does not know.
fn construct_pool(r: &MethodRef, args: &[Value]) -> Result<Option<i64>, NativeError> {
    const HEAD: &str = "(IIJLjava/util/concurrent/TimeUnit;Ljava/util/concurrent/BlockingQueue;";
    // The parameters after the queue, by name, for `requireNonNull`'s message;
    // the shorter constructors pass the default factory and handler.
    let tail: &[&str] = match r.descriptor.strip_prefix(HEAD) {
        Some(")V") => &[],
        Some("Ljava/util/concurrent/ThreadFactory;)V") => &["threadFactory"],
        Some("Ljava/util/concurrent/RejectedExecutionHandler;)V") => &["handler"],
        Some(
            "Ljava/util/concurrent/ThreadFactory;Ljava/util/concurrent/RejectedExecutionHandler;)V",
        ) => &["threadFactory", "handler"],
        _ => return Ok(None),
    };
    let core = expect_int(r, args.first().unwrap_or(&Value::Int(0)))?;
    let maximum = expect_int(r, args.get(1).unwrap_or(&Value::Int(0)))?;
    let keep_alive = expect_long(r, args.get(2).unwrap_or(&Value::Long(0)))?;
    if core < 0 {
        return Err(illegal_argument("corePoolSize must be non-negative"));
    } else if maximum <= 0 {
        return Err(illegal_argument("maximumPoolSize must be positive"));
    } else if maximum < core {
        return Err(illegal_argument(
            "maximumPoolSize must be greater than or equal to corePoolSize",
        ));
    } else if keep_alive < 0 {
        return Err(illegal_argument("keepAliveTime must be non-negative"));
    }
    let names = ["unit", "workQueue"]
        .into_iter()
        .chain(tail.iter().copied());
    for (name, value) in names.zip(args.iter().skip(3)) {
        if *value == Value::Ref(None) {
            return Err(null_pointer(name));
        }
    }
    // The unit is non-null here; a unit this runtime cannot name is not a
    // pool it can model.
    to_nanos(args.get(3), keep_alive)
}

/// `unit.toNanos(amount)` for a `TimeUnit` marker: a `null` unit is the
/// `NullPointerException` `CountDownLatch.await`'s call on it throws (the
/// constructor checks the unit for `null` first), and an unknown marker
/// answers `None`.
fn to_nanos(unit: Option<&Value>, amount: i64) -> Result<Option<i64>, NativeError> {
    let Some(unit) = unit else {
        return Ok(None);
    };
    if *unit == Value::Ref(None) {
        // JDK 25's helpful message for the call on the `unit` parameter.
        return Err(null_pointer(
            "Cannot invoke \"java.util.concurrent.TimeUnit.toNanos(long)\" because \"unit\" is null",
        ));
    }
    let Some(name) = expect_ref(unit).and_then(|marker| marker.strip_prefix("time-unit/")) else {
        return Ok(None);
    };
    let factor: i128 = match name {
        "NANOSECONDS" => 1,
        "MICROSECONDS" => 1_000,
        "MILLISECONDS" => 1_000_000,
        "SECONDS" => 1_000_000_000,
        "MINUTES" => 60_000_000_000,
        "HOURS" => 3_600_000_000_000,
        "DAYS" => 86_400_000_000_000,
        _ => return Ok(None),
    };
    // `TimeUnit.toNanos` saturates at `Long.MIN_VALUE`/`MAX_VALUE`.
    let nanos = i128::from(amount).saturating_mul(factor);
    Ok(Some(
        nanos.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
    ))
}

fn illegal_argument(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn null_pointer(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: Some(message.into()),
        frame: None,
    }
}
