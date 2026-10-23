//! Additive weighted-search and observer contracts through dependency-free APIs.
//!
//! Small graphs expose longer weighted routes, immutable reopened histories and
//! cancellation. Optional Pyramid checks independently replay retained fixtures;
//! no test sends input or treats an interrupted search as an impossibility proof.

#![cfg(feature = "search")]

use std::{
    cell::RefCell,
    num::NonZeroU32,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use xsar::search::{
    SearchLimits, SearchMethod, SearchOptions, SearchOutcome, SearchProblem, SearchProgress,
    SearchStats, SearchStop, solve, solve_with_options,
};


/// Small directed graph with stable successors and observable callback visits.
struct Graph {
    /// Ordered directed edges; edge endpoints also form the typed action.
    edges: &'static [(u8, u8)],
    /// Admissible lower bounds for selected vertices; all others use zero.
    estimates: &'static [(u8, u32)],
    /// Single goal vertex, with the initial vertex fixed at zero.
    goal: u8,
    /// Vertices for which the fixture supplies an impossibility proof.
    impossible: &'static [u8],
    /// Panic on guidance calls to prove Dijkstra ignores both optional methods.
    forbid_guidance: bool,
    /// Domain goal checks, including the separate initial-state check.
    goal_checks: RefCell<Vec<u8>>,
    /// Vertices whose outgoing edges were actually expanded.
    expansions: RefCell<Vec<u8>>,
}


impl Graph {
    /// Construct a finite graph without pruning or forbidden guidance callbacks.
    fn new(edges: &'static [(u8, u8)], estimates: &'static [(u8, u32)], goal: u8) -> Self {
        Self {
            edges,
            estimates,
            goal,
            impossible: &[],
            forbid_guidance: false,
            goal_checks: RefCell::new(Vec::new()),
            expansions: RefCell::new(Vec::new()),
        }
    }
}


impl SearchProblem for Graph {
    /// A complete neutral graph vertex without card-specific representation.
    type State = u8;
    /// Directed edge preserving both endpoints for independent path validation.
    type Action = (u8, u8);
    /// Finite iterator retaining the graph's declared outgoing-edge order.
    type Successors = std::vec::IntoIter<(Self::Action, Self::State)>;


    /// Begin at vertex zero for every independent invocation.
    fn initial(&self) -> Self::State {
        0
    }


    /// Record a domain callback and recognise only the supplied goal vertex.
    fn is_goal(&self, state: Self::State) -> bool {
        self.goal_checks.borrow_mut().push(state);
        state == self.goal
    }


    /// Record an expansion and yield legal edges in their explicit stable order.
    fn successors(&self, state: Self::State) -> Self::Successors {
        self.expansions.borrow_mut().push(state);
        self.edges
            .iter()
            .copied()
            .filter_map(|edge| (edge.0 == state).then_some((edge, edge.1)))
            .collect::<Vec<_>>()
            .into_iter()
    }


    /// Supply a conservative estimate and detect forbidden Dijkstra guidance.
    fn heuristic(&self, state: Self::State) -> u32 {
        assert!(!self.forbid_guidance, "Dijkstra called heuristic");
        self.estimates
            .iter()
            .find_map(|(vertex, estimate)| (*vertex == state).then_some(*estimate))
            .unwrap_or(0)
    }


    /// Apply explicit proof labels and detect forbidden Dijkstra pruning.
    fn impossible(&self, state: Self::State) -> bool {
        assert!(!self.forbid_guidance, "Dijkstra called impossible");
        self.impossible.contains(&state)
    }
}


/// Keep graph fixtures small, with independent state/expansion/memory/time bounds.
fn limits() -> SearchLimits {
    SearchLimits {
        max_states: 100,
        max_expansions: 100,
        max_memory_bytes: 1024 * 1024,
        max_duration: Duration::from_secs(10),
    }
}


/// Require a positive test weight while preserving the caller's method choice.
fn options(method: SearchMethod, weight: u32) -> SearchOptions {
    SearchOptions {
        method,
        heuristic_weight: NonZeroU32::new(weight).expect("positive test weight"),
    }
}


/// Extract solved actions and counters with useful diagnostics for other outcomes.
fn solved(outcome: SearchOutcome<(u8, u8)>) -> (Box<[(u8, u8)]>, SearchStats) {
    match outcome {
        SearchOutcome::Solved { actions, stats } => (actions, stats),
        other => panic!("expected a solved route, received {other:?}"),
    }
}


/// Borrow any outcome's counters without interpreting stopped searches as proofs.
fn stats<A>(outcome: &SearchOutcome<A>) -> SearchStats {
    match outcome {
        SearchOutcome::Solved { stats, .. }
        | SearchOutcome::Unsolvable { stats }
        | SearchOutcome::Stopped { stats, .. } => *stats,
    }
}


/// Compare deterministic work while excluding naturally varying wall-clock time.
fn work(mut stats: SearchStats) -> SearchStats {
    stats.elapsed = Duration::ZERO;
    stats
}


/// Require one terminal report matching the outcome and monotonic cumulative work.
fn assert_reports<A>(outcome: &SearchOutcome<A>, reports: &[SearchProgress]) {
    assert!(!reports.first().expect("initial report").finished);
    assert_eq!(reports.iter().filter(|report| report.finished).count(), 1);
    let final_report = reports.last().expect("final report");
    assert!(final_report.finished);
    assert_eq!(final_report.stats, stats(outcome));
    for pair in reports.windows(2) {
        assert!(pair[0].stats.expanded <= pair[1].stats.expanded);
        assert!(pair[0].stats.generated <= pair[1].stats.generated);
        assert!(pair[0].stats.states <= pair[1].stats.states);
        assert!(pair[0].stats.peak_frontier <= pair[1].stats.peak_frontier);
        assert!(pair[0].stats.elapsed <= pair[1].stats.elapsed);
    }
}


/// Weight two changes ordering and can stop at a longer first popped goal.
#[test]
fn weighted_search_can_return_a_longer_legal_route_before_exact_search() {
    let graph = Graph::new(
        &[(0, 1), (1, 2), (2, 3), (0, 4), (4, 5), (5, 6), (6, 3)],
        &[(1, 2), (2, 1)],
        3,
    );
    let cancel = AtomicBool::new(false);
    let (exact, exact_stats) = solved(solve_with_options(
        &graph,
        limits(),
        &cancel,
        options(SearchMethod::AStar, 1),
        None,
    ));
    let (weighted, weighted_stats) = solved(solve_with_options(
        &graph,
        limits(),
        &cancel,
        options(SearchMethod::AStar, 2),
        None,
    ));
    assert_eq!(&*exact, &[(0, 1), (1, 2), (2, 3)]);
    assert_eq!(&*weighted, &[(0, 4), (4, 5), (5, 6), (6, 3)]);
    assert!(weighted_stats.expanded < exact_stats.expanded);
    for route in [&*exact, &*weighted] {
        assert_eq!(route.first().unwrap().0, 0);
        assert_eq!(route.last().unwrap().1, graph.goal);
        assert!(route.iter().all(|edge| graph.edges.contains(edge)));
        assert!(route.windows(2).all(|pair| pair[0].1 == pair[1].0));
    }
}


/// The additive exact interface preserves routes, work counters and stable ties.
#[test]
fn weight_one_matches_existing_exact_and_reference_interfaces() {
    let graph = Graph::new(&[(0, 1), (0, 2), (1, 3), (2, 3)], &[], 3);
    let cancel = AtomicBool::new(false);
    assert_eq!(SearchOptions::default(), options(SearchMethod::AStar, 1));
    for method in [SearchMethod::AStar, SearchMethod::Dijkstra] {
        let (old_actions, old_stats) = solved(solve(&graph, limits(), &cancel, method));
        let mut reports = Vec::new();
        let mut observer = |report| reports.push(report);
        let new = solve_with_options(
            &graph,
            limits(),
            &cancel,
            options(method, 1),
            Some(&mut observer),
        );
        assert_reports(&new, &reports);
        let (new_actions, new_stats) = solved(new);
        assert_eq!(new_actions, old_actions);
        assert_eq!(work(new_stats), work(old_stats));
        assert_eq!(&*new_actions, &[(0, 1), (1, 3)]);
    }
}


/// Reference search ignores even a maximal weight and both forbidden callbacks.
#[test]
fn dijkstra_ignores_weight_and_optional_guidance_with_an_observer() {
    let mut graph = Graph::new(&[(0, 1)], &[(0, u32::MAX)], 1);
    graph.forbid_guidance = true;
    graph.impossible = &[0, 1];
    let mut reports = Vec::new();
    let mut observer = |report| reports.push(report);
    let outcome = solve_with_options(
        &graph,
        limits(),
        &AtomicBool::new(false),
        options(SearchMethod::Dijkstra, u32::MAX),
        Some(&mut observer),
    );
    assert_reports(&outcome, &reports);
    assert_eq!(&*solved(outcome).0, &[(0, 1)]);
}


/// Weighted reopening preserves parents and skips an older record before its goal.
#[test]
fn weighted_reopening_retains_immutable_history_and_skips_stale_entries() {
    let graph = Graph::new(
        &[
            (0, 1),
            (0, 7),
            (1, 2),
            (7, 8),
            (8, 2),
            (2, 3),
            (3, 4),
            (4, 5),
            (5, 6),
        ],
        &[(1, 2)],
        6,
    );
    let (actions, stats) = solved(solve_with_options(
        &graph,
        limits(),
        &AtomicBool::new(false),
        options(SearchMethod::AStar, 2),
        None,
    ));
    assert_eq!(&*actions, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)]);
    assert_eq!(stats.expanded, 11);
    assert_eq!(stats.generated, 12);
    assert_eq!(stats.states, 13);
    assert_eq!(stats.peak_frontier, 2);
    assert_eq!(
        graph
            .expansions
            .borrow()
            .iter()
            .filter(|vertex| **vertex == 2)
            .count(),
        2
    );
    assert_eq!(
        graph
            .goal_checks
            .borrow()
            .iter()
            .filter(|vertex| **vertex == 5)
            .count(),
        1
    );
}


/// All finite normal outcomes emit one final report with exactly their counters.
#[test]
fn observers_finish_for_solved_unsolvable_proofs_and_resource_stops() {
    let mut initial_proof = Graph::new(&[], &[], 9);
    initial_proof.impossible = &[0];
    let cases = [
        (
            Graph::new(&[], &[], 0),
            SearchLimits {
                max_states: 0,
                max_expansions: 0,
                max_memory_bytes: 0,
                ..limits()
            },
            false,
            None,
        ),
        (initial_proof, limits(), false, None),
        (Graph::new(&[(0, 1)], &[], 9), limits(), false, None),
        (
            Graph::new(&[(0, 1)], &[], 1),
            limits(),
            true,
            Some(SearchStop::Cancelled),
        ),
        (
            Graph::new(&[(0, 1)], &[], 1),
            SearchLimits {
                max_states: 0,
                ..limits()
            },
            false,
            Some(SearchStop::StateLimit),
        ),
        (
            Graph::new(&[(0, 1)], &[], 1),
            SearchLimits {
                max_expansions: 0,
                ..limits()
            },
            false,
            Some(SearchStop::ExpansionLimit),
        ),
        (
            Graph::new(&[(0, 1)], &[], 1),
            SearchLimits {
                max_memory_bytes: 0,
                ..limits()
            },
            false,
            Some(SearchStop::MemoryLimit),
        ),
        (
            Graph::new(&[(0, 1)], &[], 1),
            SearchLimits {
                max_duration: Duration::ZERO,
                ..limits()
            },
            false,
            Some(SearchStop::TimeLimit),
        ),
    ];
    for (graph, limits, cancelled, expected_stop) in cases {
        let mut reports = Vec::new();
        let mut observer = |report| reports.push(report);
        let outcome = solve_with_options(
            &graph,
            limits,
            &AtomicBool::new(cancelled),
            options(SearchMethod::AStar, 2),
            Some(&mut observer),
        );
        assert_reports(&outcome, &reports);
        if let Some(expected) = expected_stop {
            assert!(matches!(outcome, SearchOutcome::Stopped { reason, .. } if reason == expected));
        } else if graph.goal == 0 {
            assert!(matches!(outcome, SearchOutcome::Solved { actions, .. } if actions.is_empty()));
        } else {
            assert!(matches!(outcome, SearchOutcome::Unsolvable { .. }));
        }
    }
}


/// An initial observer can cancel before any domain callback or allocation work.
#[test]
fn observer_cancellation_precedes_initial_goal_and_zero_resource_work() {
    let graph = Graph::new(&[], &[], 0);
    let cancel = AtomicBool::new(false);
    let mut reports = Vec::new();
    let mut observer = |report| {
        reports.push(report);
        cancel.store(true, Ordering::Relaxed);
    };
    let outcome = solve_with_options(
        &graph,
        SearchLimits {
            max_states: 0,
            max_expansions: 0,
            max_memory_bytes: 0,
            max_duration: Duration::ZERO,
        },
        &cancel,
        SearchOptions::default(),
        Some(&mut observer),
    );
    assert_reports(&outcome, &reports);
    assert!(matches!(
        outcome,
        SearchOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));
    assert!(graph.goal_checks.borrow().is_empty());
    assert!(graph.expansions.borrow().is_empty());
    assert_eq!(stats(&outcome).states, 0);
}


/// The Pyramid wrapper returns independently replayable routes and shared progress.
#[cfg(all(feature = "pyramid_search", feature = "pyramid_json"))]
#[test]
fn weighted_pyramid_adapter_routes_replay_on_licensed_fixtures() {
    use xsar::games::pyramid::{planning, replay, search};

    for fixture in [15, 44] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/fixtures/pyramid-planning/upstream-{fixture}.json"
        ));
        let file = std::fs::File::open(path).expect("open retained licensed fixture");
        let deal = xsar::pyramid_json::read_from(file).expect("decode complete retained deal");
        let mut reports: Vec<planning::SearchProgress> = Vec::new();
        let mut observer = |report| reports.push(report);
        let outcome = search::solve_with_options(
            &deal,
            SearchLimits::default(),
            &AtomicBool::new(false),
            options(SearchMethod::AStar, 2),
            Some(&mut observer),
        );
        assert_reports(&outcome, &reports);
        let SearchOutcome::Solved { actions, .. } = outcome else {
            panic!("expected a weighted Pyramid route: {outcome:?}");
        };
        let trace =
            replay::replay(&deal, &actions, true).expect("independent weighted-route replay");
        assert_eq!(
            trace
                .last()
                .expect("nonempty completed route")
                .remaining_tableau,
            0
        );
    }
}
