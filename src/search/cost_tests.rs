//! Independent finite graphs distinguish objective cost from route length.

use super::*;


/// Guided ordering may select a longer legal route without invoking exact proofs.
#[test]
fn freecell_310_guided_unlimited_preserves_exact_search_and_has_no_hidden_caps() {
    /// The guided entry must never consult these admissibility-only callbacks.
    struct Guided(Graph<'static>);
    impl SearchProblem for Guided {
        type State = u8;
        type Action = (u8, u32);
        type Successors = std::vec::IntoIter<(Self::Action, Self::State)>;
        /// Borrow the same immutable graph origin.
        fn initial(&self) -> u8 {
            self.0.initial()
        }
        /// Test the declared graph endpoint.
        fn is_goal(&self, s: u8) -> bool {
            self.0.is_goal(s)
        }
        /// Retain the full graph, without heuristic pruning.
        fn successors(&self, s: u8) -> Self::Successors {
            self.0.successors(s)
        }
        /// Use the explicit nonnegative edge cost.
        fn action_cost(&self, s: u8, a: Self::Action) -> u32 {
            self.0.action_cost(s, a)
        }
        /// Calling this exact-search hook from guided search violates the API.
        fn heuristic(&self, _: u8) -> u32 {
            panic!("guided search called admissible heuristic")
        }
        /// Calling this proof hook from guided search violates the API.
        fn impossible(&self, _: u8) -> bool {
            panic!("guided search called impossibility proof")
        }
    }
    let graph = Graph {
        edges: &[(0, 1, 1), (1, 4, 1), (0, 2, 1), (2, 3, 1), (3, 4, 1)],
        goal: Some(4),
        scores: &[],
    };
    let cancel = AtomicBool::new(false);
    let SearchOutcome::Solved { actions, .. } = solve(
        &graph,
        SearchLimits::default(),
        &cancel,
        SearchMethod::Dijkstra,
    ) else {
        panic!("exact route");
    };
    assert_eq!(&*actions, &[(1, 1), (4, 1)]);
    let guide = |s| if s == 1 { 100 } else { 0 };
    let guided = Guided(graph);
    let mut notifications = Vec::new();
    let SearchOutcome::Solved { actions, stats } = solve_guided_unlimited_with_control(
        &guided,
        &cancel,
        &guide,
        Some(&mut |p| notifications.push(p)),
        None,
    ) else {
        panic!("guided route");
    };
    assert_eq!(&*actions, &[(2, 1), (3, 1), (4, 1)]);
    assert!(!notifications.first().unwrap().finished);
    assert!(notifications.last().unwrap().finished);
    assert_eq!(notifications.iter().filter(|p| p.finished).count(), 1);
    assert_eq!(notifications.last().unwrap().stats, stats);
    let tiny = SearchLimits {
        max_states: 0,
        ..SearchLimits::default()
    };
    assert!(matches!(
        solve(&guided.0, tiny, &cancel, SearchMethod::Dijkstra),
        SearchOutcome::Stopped {
            reason: SearchStop::StateLimit,
            ..
        }
    ));
    // Advance past the old default time/expansion budgets without doing that work.
    let limits = SearchLimits::default();
    let mut engine = Engine::new(limits, &cancel);
    engine.limits = None;
    engine.started = Instant::now()
        .checked_sub(limits.max_duration + Duration::from_secs(1))
        .unwrap();
    engine.stats.expanded = limits.max_expansions + 1;
    assert!(matches!(
        run(
            &guided,
            &mut engine,
            SearchOptions::default(),
            &mut ProgressReporter::new(None),
            Some(&guide)
        ),
        SearchOutcome::Solved { .. }
    ));
    let cancel = AtomicBool::new(false);
    let stopped = solve_guided_unlimited_with_control(
        &guided,
        &cancel,
        &|_| {
            cancel.store(true, AtomicOrdering::Release);
            0
        },
        None,
        None,
    );
    assert!(matches!(
        stopped,
        SearchOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));
}


/// Reopening an equal canonical key must preserve actual physical parent actions.
#[test]
fn freecell_310_guided_reopen_keeps_physical_arena_history() {
    /// Equality deliberately ignores the physical representative, as Free Cell does.
    #[derive(Clone, Copy, Debug)]
    struct Physical {
        key: u8,
        place: u8,
    }
    impl PartialEq for Physical {
        /// Only the canonical identity defines equivalent futures.
        fn eq(&self, b: &Self) -> bool {
            self.key == b.key
        }
    }
    impl Eq for Physical {}
    impl std::hash::Hash for Physical {
        /// Match canonical equality even when physical representatives differ.
        fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
            self.key.hash(h);
        }
    }
    /// Finite graph whose canonical key is reached through two physical histories.
    struct PhysicalGraph;
    impl SearchProblem for PhysicalGraph {
        type State = Physical;
        type Action = (u8, u8, u32);
        type Successors = std::vec::IntoIter<(Self::Action, Self::State)>;
        /// Start at the original physical origin.
        fn initial(&self) -> Physical {
            Physical { key: 0, place: 0 }
        }
        /// The final key is independent of physical placement.
        fn is_goal(&self, s: Physical) -> bool {
            s.key == 3
        }
        /// Record each transition from the actual parent representative.
        fn successors(&self, s: Physical) -> Self::Successors {
            let edges: &[(u8, u8, u32)] = match s.key {
                0 => &[(1, 7, 4), (2, 9, 1)],
                2 => &[(1, 3, 1)],
                1 => &[(3, 4, 20)],
                _ => &[],
            };
            edges
                .iter()
                .map(|&(key, place, cost)| ((s.place, place, cost), Physical { key, place }))
                .collect::<Vec<_>>()
                .into_iter()
        }
        /// Preserve independently declared route costs.
        fn action_cost(&self, _: Physical, a: Self::Action) -> u32 {
            a.2
        }
    }
    let outcome = solve_guided_unlimited_with_control(
        &PhysicalGraph,
        &AtomicBool::new(false),
        &|s| if s.key == 2 { 10 } else { 0 },
        None,
        None,
    );
    let SearchOutcome::Solved { actions, .. } = outcome else {
        panic!("reopened route");
    };
    assert_eq!(&*actions, &[(0, 9, 1), (9, 3, 1), (3, 4, 20)]);
    let mut physical = 0;
    for &(from, to, _) in &actions {
        assert_eq!(from, physical);
        physical = to;
    }
    assert_eq!(physical, 4);
}


/// Complete directed graph with explicit costs and independently labelled scores.
struct Graph<'a> {
    /// Stable edges, each declaring origin, destination and nonnegative cost.
    edges: &'a [(u8, u8, u32)],
    /// Optional goal; absent means an exhaustive scored fallback is required.
    goal: Option<u8>,
    /// Benefit at named endpoints, independent of paid action cost.
    scores: &'static [(u8, u32)],
}


impl SearchProblem for Graph<'_> {
    type State = u8;
    type Action = (u8, u32);
    type Successors = std::vec::IntoIter<(Self::Action, Self::State)>;


    /// Every fixture begins at the same independent origin.
    fn initial(&self) -> u8 {
        0
    }


    /// Only the declared endpoint is a full goal.
    fn is_goal(&self, state: u8) -> bool {
        self.goal == Some(state)
    }


    /// Enumerate the complete finite outgoing edge list in its declared order.
    fn successors(&self, state: u8) -> Self::Successors {
        self.edges
            .iter()
            .filter(|(from, _, _)| *from == state)
            .map(|(_, to, cost)| ((*to, *cost), *to))
            .collect::<Vec<_>>()
            .into_iter()
    }


    /// Action identity includes the independently declared edge cost.
    fn action_cost(&self, _state: u8, action: Self::Action) -> u32 {
        action.1
    }
}


impl BestEffortProblem for Graph<'_> {
    /// Read benefit directly from the fixture, never from action count or cost.
    fn score(&self, state: u8) -> u32 {
        self.scores
            .iter()
            .find(|(node, _)| *node == state)
            .map_or(0, |(_, score)| *score)
    }
}


/// Both exact methods prefer a cheaper route even when it has more operations.
#[test]
fn minimum_cost_route_retains_every_zero_cost_action() {
    let graph = Graph {
        edges: &[(0, 9, 4), (0, 1, 0), (1, 2, 1), (2, 9, 0)],
        goal: Some(9),
        scores: &[],
    };
    let cancel = AtomicBool::new(false);
    for method in [SearchMethod::AStar, SearchMethod::Dijkstra] {
        let SearchOutcome::Solved { actions, .. } =
            solve(&graph, SearchLimits::default(), &cancel, method)
        else {
            panic!("expected cheap goal")
        };
        assert_eq!(&*actions, &[(1, 0), (2, 1), (9, 0)]);
        assert_eq!(actions.iter().map(|action| action.1).sum::<u32>(), 1);
        let BestEffortOutcome::Solved {
            actions: fallback,
            minimum_proven,
            ..
        } = solve_best_effort(
            &graph,
            SearchLimits::default(),
            &cancel,
            SearchOptions {
                method,
                ..SearchOptions::default()
            },
            None,
        )
        else {
            panic!("expected cheap scored goal")
        };
        assert_eq!(fallback.as_slice(), &*actions);
        assert!(minimum_proven);
    }
}


/// Equal-cost cycles cannot improve the root or any already shorter route.
#[test]
fn zero_cost_self_and_return_cycles_terminate_without_extra_histories() {
    let graph = Graph {
        edges: &[
            (0, 0, 0),
            (0, 1, 0),
            (1, 0, 0),
            (1, 1, 0),
            (1, 2, 1),
            (2, 9, 0),
        ],
        goal: Some(9),
        scores: &[],
    };
    let cancel = AtomicBool::new(false);
    let SearchOutcome::Solved { actions, stats } = solve(
        &graph,
        SearchLimits::default(),
        &cancel,
        SearchMethod::Dijkstra,
    ) else {
        panic!("cycle must terminate")
    };
    assert_eq!(&*actions, &[(1, 0), (2, 1), (9, 0)]);
    assert_eq!(stats.states, 4);
}


/// A later cheaper state preserves its immutable predecessor chain.
#[test]
fn cheaper_longer_revisit_replaces_a_shallow_expensive_record() {
    let graph = Graph {
        edges: &[(0, 2, 5), (0, 1, 1), (1, 2, 1), (2, 3, 0), (3, 9, 0)],
        goal: Some(9),
        scores: &[],
    };
    let cancel = AtomicBool::new(false);
    let SearchOutcome::Solved { actions, stats } = solve(
        &graph,
        SearchLimits::default(),
        &cancel,
        SearchMethod::Dijkstra,
    ) else {
        panic!("expected cheaper revisit")
    };
    assert_eq!(&*actions, &[(1, 1), (2, 1), (3, 0), (9, 0)]);
    assert_eq!(stats.states, 6);
}


/// Equal-cost shorter reopening dominates the old heap entry without expansion.
#[test]
fn equal_cost_shorter_revisit_skips_the_stale_history() {
    let graph = Graph {
        edges: &[
            (0, 1, 1),
            (0, 2, 0),
            (2, 3, 0),
            (3, 4, 1),
            (1, 4, 0),
            (4, 9, 1),
        ],
        goal: Some(9),
        scores: &[],
    };
    let cancel = AtomicBool::new(false);
    let SearchOutcome::Solved { actions, stats } = solve(
        &graph,
        SearchLimits::default(),
        &cancel,
        SearchMethod::Dijkstra,
    ) else {
        panic!("expected short equal-cost revisit")
    };
    assert_eq!(&*actions, &[(1, 1), (4, 0), (9, 1)]);
    assert_eq!(stats.states, 7);
    assert_eq!(stats.expanded, 5);
}


/// Fallback ties use objective cost before total operation count.
#[test]
fn maximum_score_prefers_cheaper_longer_prefix_then_shorter_equal_cost() {
    let cancel = AtomicBool::new(false);
    for (direct_cost, expected) in [(3, vec![(1, 0), (2, 0), (9, 1)]), (1, vec![(5, 1)])] {
        let edges = [(0, 5, direct_cost), (0, 1, 0), (1, 2, 0), (2, 9, 1)];
        let graph = Graph {
            edges: &edges,
            goal: None,
            scores: &[(5, 10), (9, 10)],
        };
        let BestEffortOutcome::Partial {
            actions,
            score,
            proof,
            ..
        } = solve_best_effort(
            &graph,
            SearchLimits::default(),
            &cancel,
            SearchOptions::default(),
            None,
        )
        else {
            panic!("expected exhaustive scored prefix")
        };
        assert_eq!(actions, expected);
        assert_eq!(score, 10);
        assert_eq!(proof, PartialProof::Exhaustive);
    }
}


/// A goal discovered at a record cutoff is executable but not proven optimal.
#[test]
fn zero_cost_goal_retained_at_cutoff_has_no_minimum_claim() {
    let graph = Graph {
        edges: &[(0, 9, 0)],
        goal: Some(9),
        scores: &[],
    };
    let cancel = AtomicBool::new(false);
    let limits = SearchLimits {
        max_states: 1,
        ..SearchLimits::default()
    };
    let BestEffortOutcome::Solved {
        actions,
        minimum_proven,
        stats,
    } = solve_best_effort(&graph, limits, &cancel, SearchOptions::default(), None)
    else {
        panic!("expected retained goal")
    };
    assert_eq!(actions, [(9, 0)]);
    assert!(!minimum_proven);
    assert_eq!(stats.states, 1);
}


/// Objective arithmetic and reconstruction depth fail independently on overflow.
#[test]
fn objective_cost_depth_and_exact_priority_never_wrap_or_saturate() {
    assert_eq!(
        PathLabel {
            cost: usize::MAX,
            depth: 0
        }
        .successor(1),
        Err(SearchStop::InternalInvariant)
    );
    assert_eq!(
        PathLabel {
            cost: 0,
            depth: usize::MAX
        }
        .successor(0),
        Err(SearchStop::InternalInvariant)
    );
    assert_eq!(
        search_priority(usize::MAX, 1, NonZeroU32::MIN),
        Err(SearchStop::InternalInvariant)
    );
}


/// Cancellation inside the new cost callback precedes any downstream heuristic.
#[test]
fn cancellation_from_cost_callback_withholds_routes_before_next_callback() {
    /// A deterministic edge cost can synchronise an external STOP in this regression.
    struct RevokingCost<'a> {
        cancel: &'a AtomicBool,
    }
    impl SearchProblem for RevokingCost<'_> {
        type State = u8;
        type Action = u8;
        type Successors = std::option::IntoIter<(u8, u8)>;
        fn initial(&self) -> u8 {
            0
        }
        fn is_goal(&self, state: u8) -> bool {
            state == 2
        }
        fn successors(&self, state: u8) -> Self::Successors {
            (state == 0).then_some((1, 1)).into_iter()
        }
        fn action_cost(&self, _: u8, _: u8) -> u32 {
            self.cancel.store(true, AtomicOrdering::Relaxed);
            0
        }
        fn heuristic(&self, state: u8) -> u32 {
            assert_eq!(state, 0, "STOP must precede successor heuristic");
            0
        }
    }
    impl BestEffortProblem for RevokingCost<'_> {
        fn score(&self, _: u8) -> u32 {
            0
        }
    }

    let cancel = AtomicBool::new(false);
    let graph = RevokingCost { cancel: &cancel };
    assert!(matches!(
        solve(
            &graph,
            SearchLimits::default(),
            &cancel,
            SearchMethod::AStar
        ),
        SearchOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));
    cancel.store(false, AtomicOrdering::Relaxed);
    assert!(matches!(
        solve_best_effort(
            &graph,
            SearchLimits::default(),
            &cancel,
            SearchOptions::default(),
            None
        ),
        BestEffortOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));
}


/// Nonzero admissible but inconsistent guidance must reopen a lower-cost zero-edge route.
#[test]
fn inconsistent_admissible_heuristic_reopens_the_cheaper_path() {
    /// A small independently specified inconsistent-heuristic problem.
    struct Guided;
    impl SearchProblem for Guided {
        type State = u8;
        type Action = (u8, u32);
        type Successors = std::vec::IntoIter<(Self::Action, Self::State)>;
        fn initial(&self) -> u8 {
            0
        }
        fn is_goal(&self, state: u8) -> bool {
            state == 9
        }
        fn successors(&self, state: u8) -> Self::Successors {
            [(0, 1, 2), (0, 2, 0), (2, 1, 1), (1, 9, 1)]
                .into_iter()
                .filter(|(from, _, _)| *from == state)
                .map(|(_, to, cost)| ((to, cost), to))
                .collect::<Vec<_>>()
                .into_iter()
        }
        fn action_cost(&self, _: u8, action: Self::Action) -> u32 {
            action.1
        }
        fn heuristic(&self, state: u8) -> u32 {
            match state {
                0 | 2 => 2,
                _ => 0,
            }
        }
    }
    let cancel = AtomicBool::new(false);
    for method in [SearchMethod::AStar, SearchMethod::Dijkstra] {
        let SearchOutcome::Solved { actions, .. } =
            solve(&Guided, SearchLimits::default(), &cancel, method)
        else {
            panic!("expected reopened minimum-cost goal")
        };
        assert_eq!(&*actions, &[(2, 0), (1, 1), (9, 1)]);
    }
}
