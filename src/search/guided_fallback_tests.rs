//! Exhaustive unlimited fallback proofs, physical reopening and unusable stops.

use super::*;


/// Explicit source, destination and nonnegative declared cost.
type Edge = (u8, u8, u32);


/// Small independently specified graph with visible expansion counts.
struct Graph {
    /// Every legal directed edge; no solver-generated reference answer.
    edges: &'static [Edge],
    /// Optional complete endpoint; None makes exhaustion necessary.
    goal: Option<u8>,
    /// Explicit state-local fallback benefits.
    scores: &'static [(u8, u32)],
    /// Actual traversal work, used to reject an accidental second pass.
    expanded: Cell<usize>,
}

impl SearchProblem for Graph {
    type State = u8;
    type Action = Edge;
    type Successors = std::vec::IntoIter<(Edge, u8)>;


    /// Start at the fixed graph origin.
    fn initial(&self) -> u8 {
        0
    }


    /// Test only the explicitly supplied complete endpoint.
    fn is_goal(&self, state: u8) -> bool {
        self.goal == Some(state)
    }


    /// Count one real expansion and expose every specified outgoing edge.
    fn successors(&self, state: u8) -> Self::Successors {
        self.expanded.set(self.expanded.get() + 1);
        self.edges
            .iter()
            .copied()
            .filter(|edge| edge.0 == state)
            .map(|edge| (edge, edge.1))
            .collect::<Vec<_>>()
            .into_iter()
    }


    /// Read the independently declared cost from the physical action.
    fn action_cost(&self, _: u8, edge: Edge) -> u32 {
        edge.2
    }


    /// Guided search must never consult an admissibility-only hook.
    fn heuristic(&self, _: u8) -> u32 {
        panic!("unexpected exact heuristic")
    }


    /// Guided search must never invoke goal-pruning proofs.
    fn impossible(&self, _: u8) -> bool {
        panic!("unexpected impossibility proof")
    }
}

impl BestEffortProblem for Graph {
    /// Read the fixed independent benefit, with zero for unlisted states.
    fn score(&self, state: u8) -> u32 {
        self.scores
            .iter()
            .find(|entry| entry.0 == state)
            .map_or(0, |entry| entry.1)
    }
}


/// Build an immutable finite problem with fresh visible work counters.
fn graph(edges: &'static [Edge], scores: &'static [(u8, u32)], goal: Option<u8>) -> Graph {
    Graph {
        edges,
        goal,
        scores,
        expanded: Cell::new(0),
    }
}


/// Solve one complete graph with a caller-declared ordering only.
fn solve_graph(problem: &Graph, guide: &dyn Fn(u8) -> u32) -> BestEffortOutcome<Edge> {
    solve_guided_unlimited_best_effort_with_control(
        problem,
        &AtomicBool::new(false),
        guide,
        None,
        None,
    )
}


/// Complete reachability wins even after a higher-scoring non-goal is visited.
#[test]
fn freecell_315_guided_fallback_prefers_complete_and_exhausts_only_once() {
    let complete = graph(&[(0, 1, 1), (0, 9, 2)], &[(1, 99), (9, 52)], Some(9));
    let BestEffortOutcome::Solved {
        actions,
        minimum_proven,
        ..
    } = solve_graph(&complete, &|state| if state == 9 { 100 } else { 0 })
    else {
        panic!("reachable full goal must win")
    };
    assert_eq!(actions, [(0, 9, 2)]);
    assert!(!minimum_proven);

    let exhausted = graph(
        &[(0, 1, 1), (1, 2, 1), (0, 3, 1), (3, 4, 5)],
        &[(1, 1), (2, 10), (3, 9), (4, 20)],
        None,
    );
    let BestEffortOutcome::Partial {
        actions,
        score,
        proof,
        stats,
    } = solve_graph(&exhausted, &|state| if state == 3 { 100 } else { 0 })
    else {
        panic!("exhaustion must release a scored prefix")
    };
    assert_eq!(actions, [(0, 3, 1), (3, 4, 5)]);
    assert_eq!((score, proof), (20, PartialProof::Exhaustive));
    assert_eq!((stats.expanded, exhausted.expanded.get()), (5, 5));
}


/// Equal maxima prefer lower deliberate cost, then fewer total transitions.
#[test]
fn freecell_315_guided_fallback_proves_cost_and_depth_ties() {
    for (problem, expected) in [
        (
            graph(
                &[(0, 1, 2), (1, 9, 2), (0, 2, 0), (2, 3, 1), (3, 8, 0)],
                &[(9, 10), (8, 10)],
                None,
            ),
            vec![(0, 2, 0), (2, 3, 1), (3, 8, 0)],
        ),
        (
            graph(
                &[(0, 1, 1), (1, 8, 0), (0, 9, 1)],
                &[(8, 10), (9, 10)],
                None,
            ),
            vec![(0, 9, 1)],
        ),
    ] {
        let BestEffortOutcome::Partial {
            actions,
            score,
            proof,
            ..
        } = solve_graph(&problem, &|state| if state == 9 { 100 } else { 0 })
        else {
            panic!("exhaustive maximum")
        };
        assert_eq!(actions, expected);
        assert_eq!((score, proof), (10, PartialProof::Exhaustive));
    }
    let empty = graph(&[], &[], None);
    assert!(
        matches!(solve_graph(&empty, &|_| 0), BestEffortOutcome::Partial {
        actions, score: 0, proof: PartialProof::Exhaustive, ..
    } if actions.is_empty())
    );
}


/// Canonical equality deliberately ignores a physical representative's location.
#[derive(Clone, Copy, Debug)]
struct Physical {
    key: u8,
    place: u8,
}

impl PartialEq for Physical {
    /// Equal canonical keys have equivalent futures but retain physical history.
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Physical {}

impl Hash for Physical {
    /// Hash the same identity used by exact equality.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}


/// One canonical key is reopened at lower cost through a different physical path.
struct Reopening;

impl SearchProblem for Reopening {
    type State = Physical;
    type Action = Edge;
    type Successors = std::vec::IntoIter<(Edge, Physical)>;


    /// Preserve the original physical origin.
    fn initial(&self) -> Physical {
        Physical { key: 0, place: 0 }
    }


    /// This graph has no full goal.
    fn is_goal(&self, _: Physical) -> bool {
        false
    }


    /// Canonical futures remain equal while physical actions use their own parent.
    fn successors(&self, state: Physical) -> Self::Successors {
        let next = match (state.key, state.place) {
            (0, _) => vec![(1, 10, 4), (2, 20, 1)],
            (1, 10) => vec![(3, 30, 1)],
            (1, _) => vec![(3, 31, 1)],
            (2, _) => vec![(1, 11, 1)],
            _ => Vec::new(),
        };
        next.into_iter()
            .map(|(key, place, cost)| ((state.place, place, cost), Physical { key, place }))
            .collect::<Vec<_>>()
            .into_iter()
    }


    /// Physical transfer costs remain independent of canonical representation.
    fn action_cost(&self, _: Physical, edge: Edge) -> u32 {
        edge.2
    }
}

impl BestEffortProblem for Reopening {
    /// Benefit is canonical-state-local, as required by the public contract.
    fn score(&self, state: Physical) -> u32 {
        if state.key == 3 { 10 } else { 0 }
    }
}


/// The maximum prefix's parent chain must come from its reopened representative.
#[test]
fn freecell_315_guided_fallback_reopen_keeps_physical_parents() {
    let BestEffortOutcome::Partial { actions, score, .. } =
        solve_guided_unlimited_best_effort_with_control(
            &Reopening,
            &AtomicBool::new(false),
            &|state| if state.key == 2 { 100 } else { 0 },
            None,
            None,
        )
    else {
        panic!("exhaustive physical maximum")
    };
    assert_eq!(score, 10);
    assert_eq!(actions, [(0, 20, 1), (20, 11, 1), (11, 31, 1)]);
}


/// A retained useful candidate never turns cancellation or invariant failure into success.
#[test]
fn freecell_315_guided_fallback_stops_with_no_prefix() {
    let problem = graph(&[(0, 1, 1), (1, 2, 1)], &[(1, 10), (2, 20)], None);
    let cancel = AtomicBool::new(false);
    let outcome = solve_guided_unlimited_best_effort_with_control(
        &problem,
        &cancel,
        &|state| {
            if state == 2 {
                cancel.store(true, AtomicOrdering::Release);
            }
            0
        },
        None,
        None,
    );
    assert!(matches!(
        outcome,
        BestEffortOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));

    cancel.store(false, AtomicOrdering::Release);
    let mut engine = Engine::new(SearchLimits::default(), &cancel);
    engine.limits = None;
    engine.stats.expanded = usize::MAX;
    let score = |state| problem.score(state);
    let mut tracker = ScoreTracker {
        score: &score,
        best: None,
    };
    assert!(matches!(
        run_tracked(
            &problem,
            &mut engine,
            SearchOptions::default(),
            &mut ProgressReporter::new(None),
            Some(&|_| 0),
            Some(&mut tracker)
        ),
        SearchOutcome::Stopped {
            reason: SearchStop::InternalInvariant,
            ..
        }
    ));
    assert!(tracker.best.is_some());
}


/// Expired bounded defaults cannot silently terminate the new unlimited traversal.
#[test]
fn freecell_315_guided_fallback_has_no_default_caps_and_checks_reconstruction() {
    let problem = graph(&[(0, 1, 1)], &[(1, 1)], None);
    let cancel = AtomicBool::new(false);
    let limits = SearchLimits::default();
    let mut engine = Engine::new(limits, &cancel);
    engine.limits = None;
    engine.started = Instant::now()
        .checked_sub(limits.max_duration + Duration::from_secs(1))
        .unwrap();
    engine.stats.expanded = limits.max_expansions + 1;
    let score = |state| problem.score(state);
    let mut tracker = ScoreTracker {
        score: &score,
        best: None,
    };
    assert!(matches!(
        run_tracked(
            &problem,
            &mut engine,
            SearchOptions::default(),
            &mut ProgressReporter::new(None),
            Some(&|_| 0),
            Some(&mut tracker)
        ),
        SearchOutcome::Unsolvable { .. }
    ));
    let best = tracker.best.unwrap();
    assert_eq!(best.score, 1);
    engine.nodes[best.index].parent = Some(usize::MAX);
    assert_eq!(
        engine.route(best.index, &mut ProgressReporter::new(None)),
        Err(SearchStop::InternalInvariant)
    );
    cancel.store(true, AtomicOrdering::Release);
    assert_eq!(
        engine.route(best.index, &mut ProgressReporter::new(None)),
        Err(SearchStop::Cancelled)
    );
}


/// A final observer revocation discards both a complete route and an exhaustive prefix.
#[test]
fn freecell_315_final_observer_cancellation_withholds_solved_and_partial() {
    for goal in [Some(2), None] {
        let problem = graph(&[(0, 1, 1), (1, 2, 1)], &[(1, 1), (2, 2)], goal);
        let reference = solve_graph(&problem, &|_| 0);
        assert!(matches!(
            (goal, reference),
            (Some(_), BestEffortOutcome::Solved { .. })
                | (
                    None,
                    BestEffortOutcome::Partial {
                        proof: PartialProof::Exhaustive,
                        ..
                    }
                )
        ));
        let cancel = AtomicBool::new(false);
        let mut final_notifications = 0;
        let mut progress = |snapshot: SearchProgress| {
            if snapshot.finished {
                final_notifications += 1;
                cancel.store(true, AtomicOrdering::Release);
            }
        };
        let outcome = solve_guided_unlimited_best_effort_with_control(
            &problem,
            &cancel,
            &|_| 0,
            Some(&mut progress),
            None,
        );
        assert!(matches!(
            outcome,
            BestEffortOutcome::Stopped {
                reason: SearchStop::Cancelled,
                ..
            }
        ));
        assert_eq!(final_notifications, 1);
    }
}


/// A final observer preserves an already-cancelled stop and its original reported statistics.
#[test]
fn freecell_315_final_observer_preserves_already_stopped_reason_and_stats() {
    let problem = graph(&[(0, 1, 1)], &[(1, 1)], None);
    let cancel = AtomicBool::new(true);
    let mut final_notifications = 0;
    let mut reported_stats = None;
    let mut progress = |snapshot: SearchProgress| {
        if snapshot.finished {
            final_notifications += 1;
            reported_stats = Some(snapshot.stats);
            // Make any replacement statistics observably later than the original failure.
            let completed = Instant::now();
            while completed.elapsed() < Duration::from_millis(1) {
                std::hint::spin_loop();
            }
            cancel.store(true, AtomicOrdering::Release);
        }
    };
    let outcome = solve_guided_unlimited_best_effort_with_control(
        &problem,
        &cancel,
        &|_| 0,
        Some(&mut progress),
        None,
    );
    let BestEffortOutcome::Stopped { reason, stats } = outcome else {
        panic!("failed search must not release actions")
    };
    assert_eq!(reason, SearchStop::Cancelled);
    assert_eq!(Some(stats), reported_stats);
    assert_eq!(stats.expanded, 0);
    assert_eq!(final_notifications, 1);
}
