//! How deep jar frames may nest: as deep as the current thread's stack holds.
//!
//! The interpreter's jar frames are Rust frames ([`Runtime::invoke`] →
//! `invoke_frame` → the instruction loop → a call instruction → `invoke`), so
//! a jar that recurses without bound is a stack overflow, and a Rust stack
//! overflow aborts the process instead of unwinding to the loader that could
//! skip just that jar. Every jar frame is admitted here first.
//!
//! The bound used to be a fixed count, 12 frames unoptimised and 128
//! optimised, sized from a frame cost of "about 100 KiB" and "about 3 KiB".
//! Measured on 2026-11-13 over the 43 jars of in-game survey #4 (the stack
//! address at every frame entry, x86-64 Linux, rustc stable), one jar frame
//! now costs 149 KiB unoptimised (median; at most 183 KiB) and 10.7 KiB
//! optimised (at most 15.2 KiB). Twelve unoptimised frames were 1.8 MiB of
//! the 2 MiB a game server thread has, and 128 optimised ones up to 1.9 MiB:
//! the fixed counts no longer kept the room they promised. They also refused
//! real chains a JVM runs: `structure_layout_optimizer`'s entrypoint nests 14
//! jar frames through ResourcefulConfig (`Main#onInitialize` …
//! `JavaConfigParser#getEntryType` ×2 → `EntryType#<clinit>` →
//! `EntryType#<init>`), as many on the JVM, which the 12-frame bound refused
//! even on the 8 MiB thread the probe runs on.
//!
//! So the bound is the stack itself: a frame is admitted while at least
//! [`FRAME_STACK_FLOOR`] bytes of the thread's stack are left below the call
//! (`java_regex::stack` reads the thread's limits from the platform). That
//! floor covers the frame about to start plus the host work that runs beneath
//! a frame without bounding its own stack use; work that does bound itself
//! (the regex matcher) adapts to whatever is left. A deep chain runs on a
//! thread with room for it and is refused, never aborted, on one without.
//!
//! Where the platform does not report the thread's stack, the bound stays a
//! count ([`UNKNOWN_STACK_DEPTH`]), sized from the measured worst frame so a
//! 2 MiB thread keeps 640 KiB free (what the regex matcher assumes there).

use crate::runtime::java_regex::stack;

/// Stack a new jar frame needs left below its call before it may start.
///
/// The survey sweep measured at most 353 KiB used below the deepest frame
/// entry of any jar unoptimised (that frame included) and 24 KiB optimised;
/// the floor keeps a margin above both. Optimised, it also leaves the regex
/// matcher room above its own 128 KiB reserve.
#[cfg(debug_assertions)]
pub(crate) const FRAME_STACK_FLOOR: usize = 512 << 10;
#[cfg(not(debug_assertions))]
pub(crate) const FRAME_STACK_FLOOR: usize = 256 << 10;

/// The most jar frames one execution nests whatever its stack: a jar whose
/// recursion never ends is refused at this depth on a very large stack
/// instead of filling it. An optimised build fits ~700 frames on an 8 MiB
/// thread, so the cap binds only on larger ones.
pub(crate) const MAX_JAR_CALL_DEPTH: usize = 1024;

/// The bound, in frames, where the platform does not report the thread's
/// stack: (2 MiB − 640 KiB) over the measured worst frame (183 KiB
/// unoptimised, 15.2 KiB optimised).
#[cfg(debug_assertions)]
pub(crate) const UNKNOWN_STACK_DEPTH: usize = 7;
#[cfg(not(debug_assertions))]
pub(crate) const UNKNOWN_STACK_DEPTH: usize = 92;

/// The address of a fresh local: where the stack is now.
#[inline(always)]
fn stack_position() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// Whether one more jar frame may start when `depth` jar frames are already
/// on this execution's chain.
pub(crate) fn admits_frame(depth: usize) -> bool {
    if depth >= MAX_JAR_CALL_DEPTH {
        return false;
    }
    match stack::remaining(stack_position()) {
        Some(left) => left >= FRAME_STACK_FLOOR,
        None => depth < UNKNOWN_STACK_DEPTH,
    }
}
