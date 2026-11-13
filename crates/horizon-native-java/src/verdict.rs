//! The label one in-game run gets, from its entrypoint stages and host calls.
//!
//! A loader runs more than one entrypoint stage (`main` then `server` on a
//! server). The game's report keeps the jar running as long as one stage ran,
//! because the listeners that stage registered stay live, but a jar with a
//! failed stage must never read as clean: a real loader would have thrown, so
//! the failure is this interpreter's, not evidence about the jar. `ran-partial`
//! is that state and outranks the host-call distinction.

/// The verdict for one jar.
///
/// * no stage ran: `refused`;
/// * at least one stage failed and at least one ran: `ran-partial`, whatever
///   its host-call count;
/// * every stage ran: `ran-hollow` when a host call was answered neutrally,
///   else `ran-clean`.
pub fn verdict(stages_ran: usize, failed_stages: usize, unknown_host_calls: usize) -> &'static str {
    if stages_ran == 0 {
        "refused"
    } else if failed_stages > 0 {
        "ran-partial"
    } else if unknown_host_calls > 0 {
        "ran-hollow"
    } else {
        "ran-clean"
    }
}

#[cfg(test)]
mod tests {
    use super::verdict;

    #[test]
    fn one_failed_stage_among_running_ones_is_partial() {
        assert_eq!(verdict(1, 1, 0), "ran-partial");
        assert_eq!(verdict(2, 1, 0), "ran-partial");
    }

    #[test]
    fn every_stage_failing_is_refused() {
        assert_eq!(verdict(0, 0, 0), "refused");
        assert_eq!(verdict(0, 2, 7), "refused");
    }

    #[test]
    fn all_stages_running_cleanly_is_ran_clean() {
        assert_eq!(verdict(1, 0, 0), "ran-clean");
        assert_eq!(verdict(2, 0, 0), "ran-clean");
    }

    #[test]
    fn unmodelled_calls_make_a_full_run_hollow() {
        assert_eq!(verdict(1, 0, 3), "ran-hollow");
    }

    #[test]
    fn a_failed_stage_outranks_unknown_host_calls() {
        assert_eq!(verdict(1, 1, 9), "ran-partial");
    }
}
