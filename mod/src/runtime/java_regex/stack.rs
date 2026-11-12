//! How much of the current thread's stack is still free.
//!
//! The matcher runs inside the interpreter, which may already have a dozen
//! large frames on the stack (in an unoptimized build an interpreter frame
//! takes ~100 KB). A fixed allowance measured from the matcher's own entry
//! ignores those and could overflow the thread; the allowance is the stack
//! actually left below the matcher's entry instead.
//!
//! The thread's lowest usable address comes from the platform once per
//! thread: `pthread_getattr_np` (Linux, Android), `pthread_get_stackaddr_np`
//! and `pthread_get_stacksize_np` (macOS), `GetCurrentThreadStackLimits`
//! (Windows). Elsewhere it is unknown and the caller falls back to a fixed
//! allowance.

use std::cell::Cell;

thread_local! {
    /// This thread's lowest usable stack address, once looked up (`Some(None)`
    /// when the platform does not say).
    static LOW: Cell<Option<Option<usize>>> = const { Cell::new(None) };
}

/// Bytes between `here` (an address on the current stack) and the end of
/// the current thread's stack, or `None` where the platform does not say.
pub(crate) fn remaining(here: usize) -> Option<usize> {
    let low = LOW.with(|cached| {
        cached.get().unwrap_or_else(|| {
            let low = low_bound();
            cached.set(Some(low));
            low
        })
    })?;
    here.checked_sub(low)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn low_bound() -> Option<usize> {
    // SAFETY: `attr` is initialised by `pthread_getattr_np` before it is
    // read, and destroyed exactly once; the out-pointers are valid locals.
    unsafe {
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
            return None;
        }
        let mut address: *mut libc::c_void = std::ptr::null_mut();
        let mut size: libc::size_t = 0;
        let mut guard: libc::size_t = 0;
        let stack = libc::pthread_attr_getstack(&attr, &mut address, &mut size);
        let guarded = libc::pthread_attr_getguardsize(&attr, &mut guard);
        libc::pthread_attr_destroy(&mut attr);
        // The guard page(s) may lie inside the reported range; never count
        // them as usable.
        (stack == 0 && guarded == 0 && !address.is_null()).then(|| address as usize + guard)
    }
}

#[cfg(target_os = "macos")]
fn low_bound() -> Option<usize> {
    // SAFETY: both calls only read the current thread's attributes.
    unsafe {
        let thread = libc::pthread_self();
        let top = libc::pthread_get_stackaddr_np(thread) as usize;
        let size = libc::pthread_get_stacksize_np(thread);
        // One page below the reported size is the guard.
        top.checked_sub(size)?.checked_add(16 << 10)
    }
}

#[cfg(windows)]
fn low_bound() -> Option<usize> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let (mut low, mut high) = (0usize, 0usize);
    // SAFETY: the function writes the two limits of the current thread to
    // the given valid pointers.
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    // The lowest pages are the guard and the overflow handler's reserve.
    (low != 0).then(|| low + (64 << 10))
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
)))]
fn low_bound() -> Option<usize> {
    None
}
