//! `java.util.Locale` construction and `java.lang.Thread` identity.
//!
//! Both are JDK surfaces real jars reach during bootstrap, and both were
//! counted as gaps by the in-game survey (`docs/in-game-survey.md`, "Top
//! causes" §5):
//!
//! * `new Locale("en", "US")` — calcmod's `CalcCommand.<clinit>` builds one to
//!   ask `NumberFormat.getInstance(Locale)`, and the host had no constructor
//!   for it, so the class failed to initialise and its command never
//!   registered. A `Locale` is an exact (language, country) pair here, with
//!   the JDK's normalisation (language lower-case, country upper-case) and its
//!   `toString`/`toLanguageTag` spellings.
//! * `Thread#getId()` — spark's `ThreadDumper$Specific(Thread)` records the
//!   game thread's id. The runtime is single-threaded, so the one thread a
//!   callback runs on is the JVM's main thread: id 1. A `Thread` object a jar
//!   constructs keeps a stable id of its own, because it never runs (nothing
//!   here starts a thread) and two distinct objects must not compare equal.
//!
//! `Thread.currentThread()` already answers that main marker through the host;
//! this module only gives it an id.

use crate::{MethodRef, NativeError, StdHost, Value, expect_ref};

/// The marker `Thread.currentThread()` answers.
const MAIN_THREAD: &str = "thread/main";

/// Whether `marker` is a thread this runtime knows: the main thread, or a
/// `Thread` object a jar constructed (`java/lang/Thread#<n>`).
fn thread_id(marker: &str) -> Option<i64> {
    if marker == MAIN_THREAD {
        return Some(1);
    }
    let object = marker.strip_prefix("java/lang/Thread#")?;
    // `new_object` numbers every object of the invocation, so the id is
    // distinct from the main thread's 1 and stable for the object's lifetime.
    object.parse::<i64>().ok().map(|id| id.saturating_add(2))
}

/// The (language, country) a locale marker names.
///
/// `locale/US`, `locale/ROOT` and `locale/ENGLISH` are the static constants;
/// a constructed locale is `locale/<language>_<country>` (either half may be
/// empty, as `new Locale("en")` is).
fn locale_parts(marker: &str) -> Option<(&str, &str)> {
    match marker.strip_prefix("locale/")? {
        "US" => Some(("en", "US")),
        "ROOT" => Some(("", "")),
        "ENGLISH" => Some(("en", "")),
        rest => rest.split_once('_'),
    }
}

/// The language tag the JDK's `Locale.toLanguageTag` builds for a
/// language/country pair: the language, `und` when it is empty, then the
/// country after a hyphen when that is not empty.
fn language_tag(language: &str, country: &str) -> String {
    let language = if language.is_empty() { "und" } else { language };
    if country.is_empty() {
        language.to_owned()
    } else {
        format!("{language}-{country}")
    }
}

/// `sun.util.locale.BaseLocale.hashCode`: characters folded with 31 from
/// zero, in language→script→region→variant order, each lower-cased. Script and
/// variant are always empty for a locale this model builds.
fn locale_hash(language: &str, country: &str) -> i32 {
    fn codes(text: &str) -> impl Iterator<Item = i32> + '_ {
        text.chars().map(|character| {
            let mut lowered = character.to_lowercase();
            let first = lowered.next().unwrap_or(character);
            i32::from(first as u32 as u16)
        })
    }
    let mut hash = 0i32;
    for code in codes(language).chain(codes(country)) {
        hash = 31i32.wrapping_mul(hash).wrapping_add(code);
    }
    hash
}

/// The `java.util.Locale` spelling: `language`, then `_country` when set.
fn locale_text(language: &str, country: &str) -> String {
    if country.is_empty() {
        language.to_owned()
    } else {
        format!("{language}_{country}")
    }
}

impl StdHost {
    /// `java.util.Locale` construction and its identity/format queries.
    pub(crate) fn locale_member(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if r.owner != "java/util/Locale" {
            return None;
        }
        let marker = receiver.and_then(expect_ref)?;
        // `new Locale(language, country)`: the JDK lower-cases the language
        // and upper-cases the country before the object exists.
        if r.name == "<init>" {
            return match r.descriptor.as_str() {
                "(Ljava/lang/String;)V" => {
                    let language = args.first().and_then(expect_ref)?.to_lowercase();
                    self.locales
                        .insert(marker.clone(), (language, String::new()));
                    Some(Ok(Value::Void))
                }
                "(Ljava/lang/String;Ljava/lang/String;)V" => {
                    let language = args.first().and_then(expect_ref)?;
                    let country = args.get(1).and_then(expect_ref)?;
                    let language = language.to_lowercase();
                    let country = country.to_uppercase();
                    self.locales.insert(marker.clone(), (language, country));
                    Some(Ok(Value::Void))
                }
                // `new Locale(String, String, String)` carries a variant this
                // model does not keep; refusing it is better than silently
                // dropping the part a caller may compare.
                _ => None,
            };
        }
        let (language, country) = if let Some(parts) = self.locales.get(marker).cloned() {
            parts
        } else {
            let (language, country) = locale_parts(marker)?;
            (language.to_owned(), country.to_owned())
        };
        Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("getLanguage", "()Ljava/lang/String;") => Ok(string(&language)),
            ("getCountry", "()Ljava/lang/String;") => Ok(string(&country)),
            ("toLanguageTag", "()Ljava/lang/String;") => {
                Ok(string(&language_tag(&language, &country)))
            }
            ("toString", "()Ljava/lang/String;") => Ok(string(&locale_text(&language, &country))),
            // `Locale.equals` compares the language and country here; the
            // script, variant and extensions a tag could carry are not part of
            // this model, and no constructor that takes them is modelled.
            ("equals", "(Ljava/lang/Object;)Z") => {
                let other = args
                    .first()
                    .and_then(expect_ref)
                    .and_then(|other| locale_parts(other))
                    .map(|(l, c)| (l.to_owned(), c.to_owned()));
                Ok(Value::Int(i32::from(
                    other == Some((language.clone(), country.clone())),
                )))
            }
            // `Locale.hashCode` is `sun.util.locale.BaseLocale.hashCode`:
            // every character of language, script, region and variant folded
            // with 31 from zero, language/script/region lower-cased (JDK 27's
            // bytecode, `BaseLocale.hashCode`; there are no extensions here).
            ("hashCode", "()I") => Ok(Value::Int(locale_hash(&language, &country))),
            _ => Err(NativeError::Metadata(format!(
                "unsupported Locale query {}#{}{}",
                r.owner, r.name, r.descriptor
            ))),
        })
    }

    /// `Thread#getId`/`Thread#threadId` on the main thread or a constructed
    /// `Thread` object.
    pub(crate) fn thread_identity(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
    ) -> Option<Result<Value, NativeError>> {
        if r.owner != "java/lang/Thread" {
            return None;
        }
        if !matches!(
            (r.name.as_str(), r.descriptor.as_str()),
            ("getId" | "threadId", "()J")
        ) {
            return None;
        }
        let marker = receiver.and_then(expect_ref)?;
        let id = thread_id(marker)?;
        Some(Ok(Value::Long(id)))
    }
}

fn string(text: &str) -> Value {
    Value::Ref(Some(text.to_owned()))
}
