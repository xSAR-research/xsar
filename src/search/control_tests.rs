//! Deterministic checkpoint cancellation, clock arithmetic and search preservation.

use super::*;


/// A counted chain exposes duplicate expansion if a suspended frontier is restarted.
struct ParkGraph {
    /// Actual successor calls, visible independently of throttled progress reports.
    expansions: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}


impl SearchProblem for ParkGraph {
    type State = u8;
    type Action = u8;
    type Successors = std::option::IntoIter<(u8, u8)>;


    /// Start at the first of three fixed nodes.
    fn initial(&self) -> u8 {
        0
    }


    /// The final chain node completes the route.
    fn is_goal(&self, state: u8) -> bool {
        state == 2
    }


    /// Count actual expansion before exposing its one legal successor.
    fn successors(&self, state: u8) -> Self::Successors {
        self.expansions.fetch_add(1, AtomicOrdering::SeqCst);
        state
            .checked_add(1)
            .filter(|next| *next <= 2)
            .map(|next| (next, next))
            .into_iter()
    }


    /// Remaining chain length is admissible under exact and weighted ordering.
    fn heuristic(&self, state: u8) -> u32 {
        u32::from(2_u8.saturating_sub(state))
    }
}


/// Real exact/weighted searches park after expansion and retain their original frontier.
/// Channel barriers prove suspension without sleeps; route and work match uninterrupted search.
#[test]
fn parked_search_retains_frontier_and_work_across_continue() {
    use std::sync::{Arc, atomic::AtomicUsize, mpsc};
    for weight in [1, 3] {
        let options = SearchOptions {
            heuristic_weight: NonZeroU32::new(weight).unwrap(),
            ..SearchOptions::default()
        };
        let reference = solve_with_options(
            &ParkGraph {
                expansions: Arc::new(AtomicUsize::new(0)),
            },
            SearchLimits::default(),
            &AtomicBool::new(false),
            options,
            None,
        );
        let expansions = Arc::new(AtomicUsize::new(0));
        let worker_expansions = Arc::clone(&expansions);
        let (park_tx, park_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let problem = ParkGraph {
                expansions: worker_expansions,
            };
            let parked = Cell::new(false);
            let checkpoint = || {
                if problem.expansions.load(AtomicOrdering::SeqCst) > 0 && !parked.replace(true) {
                    let started = Instant::now();
                    park_tx.send(()).unwrap();
                    resume_rx.recv().unwrap();
                    started.elapsed()
                } else {
                    Duration::ZERO
                }
            };
            solve_with_control(
                &problem,
                SearchLimits::default(),
                &AtomicBool::new(false),
                options,
                None,
                Some(&checkpoint),
            )
        });
        park_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(expansions.load(AtomicOrdering::SeqCst), 1);
        for _ in 0..128 {
            std::thread::yield_now();
            assert_eq!(expansions.load(AtomicOrdering::SeqCst), 1);
        }
        resume_tx.send(()).unwrap();
        let controlled = worker.join().unwrap();
        let (
            SearchOutcome::Solved {
                actions: expected,
                stats: baseline,
            },
            SearchOutcome::Solved {
                actions: actual,
                stats,
            },
        ) = (reference, controlled)
        else {
            panic!("both chain searches must solve");
        };
        assert_eq!(actual, expected);
        assert_eq!(expansions.load(AtomicOrdering::SeqCst), 2);
        assert_eq!(
            (
                stats.expanded,
                stats.generated,
                stats.states,
                stats.peak_frontier
            ),
            (
                baseline.expanded,
                baseline.generated,
                baseline.states,
                baseline.peak_frontier
            )
        );
    }
}


/// Tiny chain that can prove the full goal impossible before unpruned fallback.
struct ControlGraph {
    /// True means the primary goal is impossible but useful prefixes still exist.
    impossible_root: bool,
    /// Records the first actual expansion, which only fallback reaches when pruned.
    successors_started: Cell<bool>,
}


impl SearchProblem for ControlGraph {
    type State = u8;
    type Action = u8;
    type Successors = std::option::IntoIter<(u8, u8)>;


    /// Start at the first of three fixed nodes.
    fn initial(&self) -> u8 {
        0
    }


    /// Node two is a goal only when the graph has not declared it impossible.
    fn is_goal(&self, state: u8) -> bool {
        !self.impossible_root && state == 2
    }


    /// Yield one stable successor without extending beyond node two.
    fn successors(&self, state: u8) -> Self::Successors {
        self.successors_started.set(true);
        state
            .checked_add(1)
            .filter(|next| *next <= 2)
            .map(|next| (next, next))
            .into_iter()
    }


    /// Keep a sound root proof separate from the scored-prefix benefit.
    fn impossible(&self, state: u8) -> bool {
        self.impossible_root && state == 0
    }
}


impl BestEffortProblem for ControlGraph {
    /// Each later node has strictly greater partial benefit.
    fn score(&self, state: u8) -> u32 {
        u32::from(state)
    }
}


/// Pause accounting is exact and cannot exclude time outside a callback span.
#[test]
fn clamped_pause_accounting_preserves_active_clock() {
    let cancel = AtomicBool::new(false);
    let mut engine = Engine::<u8, u8>::new(SearchLimits::default(), &cancel);
    let origin = Instant::now();
    engine.started = origin;
    engine.account_pause(Duration::from_secs(100), Duration::from_secs(2));
    engine.account_pause(Duration::from_secs(3), Duration::from_secs(4));
    engine.account_pause(Duration::ZERO, Duration::from_secs(10));
    assert_eq!(engine.paused.get(), Duration::from_secs(5));
    let now = origin.checked_add(Duration::from_secs(12)).unwrap();
    assert_eq!(engine.active_elapsed_at(now), Duration::from_secs(7));
    let resumed = now.checked_add(Duration::from_secs(6)).unwrap();
    assert_eq!(engine.active_elapsed_at(resumed), Duration::from_secs(13));
}


/// Statistics and finite deadlines both exclude the same synthetic prior pause.
#[test]
fn finite_deadline_and_statistics_share_active_time() {
    let cancel = AtomicBool::new(false);
    let mut engine = Engine::<u8, u8>::new(
        SearchLimits {
            max_duration: Duration::from_secs(3_600),
            ..SearchLimits::default()
        },
        &cancel,
    );
    engine.started = Instant::now()
        .checked_sub(Duration::from_secs(7_200))
        .unwrap();
    engine.account_pause(Duration::from_secs(7_140), Duration::from_secs(7_140));
    assert_eq!(engine.poll(), Ok(()));
    let resumed_stats = engine.stats();
    assert!(resumed_stats.elapsed >= Duration::from_secs(60));
    assert!(resumed_stats.elapsed < engine.limits.max_duration);
    engine.paused.set(Duration::ZERO);
    assert_eq!(engine.poll(), Err(SearchStop::TimeLimit));
    assert!(engine.stats().elapsed >= Duration::from_secs(7_200));
}


/// A pre-existing STOP skips the callback; STOP inside it fences all further work.
#[test]
fn checkpoint_cancellation_is_checked_on_both_sides() {
    let cancel = AtomicBool::new(true);
    let calls = Cell::new(0);
    let checkpoint = || {
        calls.set(calls.get() + 1);
        cancel.store(true, AtomicOrdering::Relaxed);
        Duration::MAX
    };
    let mut engine = Engine::<u8, u8>::new(
        SearchLimits {
            max_duration: Duration::ZERO,
            ..SearchLimits::default()
        },
        &cancel,
    );
    engine.checkpoint = Some(&checkpoint);
    assert_eq!(engine.poll(), Err(SearchStop::Cancelled));
    assert_eq!(calls.get(), 0);
    cancel.store(false, AtomicOrdering::Relaxed);
    assert_eq!(engine.poll(), Err(SearchStop::Cancelled));
    assert_eq!(calls.get(), 1);
    assert!(engine.nodes.is_empty());
    assert!(engine.frontier.is_empty());
}


/// A non-blocking checkpoint preserves exact route, ordering and work counters.
#[test]
fn checkpoint_preserves_exact_route_and_search_work() {
    let problem = ControlGraph {
        impossible_root: false,
        successors_started: Cell::new(false),
    };
    let cancel = AtomicBool::new(false);
    let calls = Cell::new(0);
    let checkpoint = || {
        calls.set(calls.get() + 1);
        Duration::ZERO
    };
    let reference = solve_with_options(
        &problem,
        SearchLimits::default(),
        &cancel,
        SearchOptions::default(),
        None,
    );
    let controlled = solve_with_control(
        &problem,
        SearchLimits::default(),
        &cancel,
        SearchOptions::default(),
        None,
        Some(&checkpoint),
    );
    let (
        SearchOutcome::Solved {
            actions: expected,
            stats: baseline,
        },
        SearchOutcome::Solved {
            actions: actual,
            stats,
        },
    ) = (reference, controlled)
    else {
        panic!("expected both searches to solve the tiny chain");
    };
    assert_eq!(actual, expected);
    assert_eq!(actual.as_ref(), [1, 2]);
    assert_eq!(
        (
            stats.expanded,
            stats.generated,
            stats.states,
            stats.peak_frontier
        ),
        (
            baseline.expanded,
            baseline.generated,
            baseline.states,
            baseline.peak_frontier
        )
    );
    assert!(calls.get() > 0);
}


/// Fallback continues to invoke control after the guided root is pruned.
#[test]
fn fallback_keeps_checkpoint_and_partial_proof() {
    let problem = ControlGraph {
        impossible_root: true,
        successors_started: Cell::new(false),
    };
    let cancel = AtomicBool::new(false);
    let fallback_calls = Cell::new(0);
    let checkpoint = || {
        if problem.successors_started.get() {
            fallback_calls.set(fallback_calls.get() + 1);
        }
        Duration::ZERO
    };
    let result = solve_best_effort_with_control(
        &problem,
        SearchLimits::default(),
        &cancel,
        SearchOptions::default(),
        None,
        Some(&checkpoint),
    );
    let BestEffortOutcome::Partial {
        actions,
        score,
        proof,
        ..
    } = result
    else {
        panic!("expected an exhaustive prefix from unpruned fallback");
    };
    assert_eq!(actions, [1, 2]);
    assert_eq!(score, 2);
    assert_eq!(proof, PartialProof::Exhaustive);
    assert!(fallback_calls.get() > 0);
}


/// Cancellation from a checkpoint during fallback never exposes a partial route.
#[test]
fn fallback_checkpoint_cancellation_suppresses_prefix() {
    let problem = ControlGraph {
        impossible_root: true,
        successors_started: Cell::new(false),
    };
    let cancel = AtomicBool::new(false);
    let checkpoint = || {
        if problem.successors_started.get() {
            cancel.store(true, AtomicOrdering::Relaxed);
        }
        Duration::ZERO
    };
    assert!(matches!(
        solve_best_effort_with_control(
            &problem,
            SearchLimits::default(),
            &cancel,
            SearchOptions::default(),
            None,
            Some(&checkpoint),
        ),
        BestEffortOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));
}
