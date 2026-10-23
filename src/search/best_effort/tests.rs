//! Small exhaustive graphs check scored fallback semantics independently of Pyramid rules.

use super::*;


/// Finite deterministic graph with an optional goal and a sound root dead-end proof.
struct Graph {
    /// Complete ordered edge list; action names are destination states.
    edges: &'static [(u8, u8)],
    /// Optional reachable goal; absent means no full goal exists.
    goal: Option<u8>,
    /// Stable state-local fallback scores.
    scores: &'static [(u8, u32)],
    /// A full-goal impossibility proof, valid only when no goal exists.
    impossible_root: bool,
}


impl SearchProblem for Graph {
    type State = u8;
    type Action = u8;
    type Successors = std::vec::IntoIter<(u8, u8)>;


    /// Use node zero as the unchanged graph origin.
    fn initial(&self) -> u8 {
        0
    }


    /// Only the explicit goal node is a full goal.
    fn is_goal(&self, state: u8) -> bool {
        self.goal == Some(state)
    }


    /// Generate each legal outgoing edge in its declared order.
    fn successors(&self, state: u8) -> Self::Successors {
        self.edges
            .iter()
            .filter(|(from, _)| *from == state)
            .map(|(_, to)| (*to, *to))
            .collect::<Vec<_>>()
            .into_iter()
    }


    /// Prove the whole no-goal graph impossible without ruling out useful scores.
    fn impossible(&self, state: u8) -> bool {
        self.impossible_root && self.goal.is_none() && state == 0
    }
}


impl BestEffortProblem for Graph {
    /// Missing score labels carry zero benefit.
    fn score(&self, state: u8) -> u32 {
        self.scores
            .iter()
            .find(|(node, _)| *node == state)
            .map_or(0, |(_, score)| *score)
    }
}


/// Two routes to the best score, including cycles and a longer equal-score prefix.
fn no_goal(impossible_root: bool) -> Graph {
    Graph {
        edges: &[(0, 1), (1, 2), (0, 3), (3, 4), (4, 5), (5, 2), (2, 0)],
        goal: None,
        scores: &[(1, 1), (2, 6), (3, 2), (4, 3), (5, 6)],
        impossible_root,
    }
}


/// A sound full-clear pruning proof must not discard the maximum-removal prefix.
#[test]
fn exhaustive_fallback_ignores_goal_pruning_and_minimises_equal_score_cost() {
    let problem = no_goal(true);
    let cancel = AtomicBool::new(false);
    assert!(matches!(
        solve_with_options(
            &problem,
            SearchLimits::default(),
            &cancel,
            SearchOptions::default(),
            None
        ),
        SearchOutcome::Unsolvable { .. }
    ));
    match solve_best_effort(
        &problem,
        SearchLimits::default(),
        &cancel,
        SearchOptions::default(),
        None,
    ) {
        BestEffortOutcome::Partial {
            actions,
            score,
            proof,
            ..
        } => {
            assert_eq!(actions, [1, 2]);
            assert_eq!(score, 6);
            assert_eq!(proof, PartialProof::Exhaustive);
        }
        other => panic!("expected exhaustive partial route: {other:?}"),
    }
}


/// Full routes always outrank a higher-scoring non-goal, preserving exact proof.
#[test]
fn full_goal_precedes_fallback_score_and_keeps_exact_proof() {
    let problem = Graph {
        edges: &[(0, 1), (1, 2), (0, 3)],
        goal: Some(2),
        scores: &[(3, 100)],
        impossible_root: false,
    };
    let cancel = AtomicBool::new(false);
    match solve_best_effort(
        &problem,
        SearchLimits::default(),
        &cancel,
        SearchOptions::default(),
        None,
    ) {
        BestEffortOutcome::Solved {
            actions,
            minimum_proven,
            ..
        } => {
            assert_eq!(actions, [1, 2]);
            assert!(minimum_proven);
        }
        other => panic!("expected full goal: {other:?}"),
    }
}


/// Phase changes cannot reset state/expansion budgets or emit multiple final snapshots.
#[test]
fn shared_cutoff_returns_best_found_and_monotonic_cumulative_statistics() {
    let cancel = AtomicBool::new(false);
    let limits = SearchLimits {
        max_states: 8,
        max_expansions: 8,
        ..SearchLimits::default()
    };
    let mut snapshots = Vec::new();
    let mut observer = |snapshot: SearchProgress| snapshots.push(snapshot);
    let result = solve_best_effort(
        &no_goal(false),
        limits,
        &cancel,
        SearchOptions::default(),
        Some(&mut observer),
    );
    let BestEffortOutcome::Partial {
        actions,
        score,
        proof,
        stats,
    } = result
    else {
        panic!("expected bounded partial");
    };
    assert_eq!(actions, [1, 2]);
    assert_eq!(score, 6);
    assert_eq!(proof, PartialProof::Bounded(SearchStop::StateLimit));
    assert_eq!(stats.states, 8);
    assert!(stats.expanded <= 8);
    assert!(!snapshots[0].finished);
    assert_eq!(snapshots.iter().filter(|item| item.finished).count(), 1);
    assert_eq!(snapshots.last().unwrap().stats, stats);
    assert!(
        snapshots
            .windows(2)
            .all(|pair| pair[0].stats.states <= pair[1].stats.states
                && pair[0].stats.expanded <= pair[1].stats.expanded)
    );
}


/// Exhausted unpruned primary search already proves the fallback without spending a second budget.
#[test]
fn exhausted_primary_dijkstra_proves_maximum_without_repeating_the_graph() {
    let graph = Graph {
        edges: &[(0, 1), (1, 2)],
        goal: None,
        scores: &[(2, 8)],
        impossible_root: false,
    };
    let limits = SearchLimits {
        max_states: 3,
        max_expansions: 3,
        ..SearchLimits::default()
    };
    let options = SearchOptions {
        method: SearchMethod::Dijkstra,
        ..SearchOptions::default()
    };
    let result = solve_best_effort(&graph, limits, &AtomicBool::new(false), options, None);
    assert!(
        matches!(result, BestEffortOutcome::Partial { actions, score: 8, proof: PartialProof::Exhaustive, stats } if actions == [1, 2] && stats.states == 3 && stats.expanded == 3)
    );
}


/// A goal retained before a state cutoff is useful but has no popped-goal minimum proof.
#[test]
fn discovered_goal_at_resource_cutoff_never_claims_an_exact_minimum() {
    let graph = Graph {
        edges: &[(0, 1)],
        goal: Some(1),
        scores: &[],
        impossible_root: false,
    };
    let limits = SearchLimits {
        max_states: 1,
        ..SearchLimits::default()
    };
    assert!(
        matches!(solve_best_effort(&graph, limits, &AtomicBool::new(false), SearchOptions::default(), None), BestEffortOutcome::Solved { actions, minimum_proven: false, stats } if actions == [1] && stats.states == 1)
    );
}


/// Final observer cancellation withholds both solved and useful partial routes.
#[test]
fn cancellation_from_final_progress_withholds_every_executable_outcome() {
    for goal in [None, Some(2)] {
        let graph = Graph {
            edges: &[(0, 1), (1, 2)],
            goal,
            scores: &[(2, 8)],
            impossible_root: false,
        };
        let cancel = AtomicBool::new(false);
        let mut finished = 0usize;
        let mut observer = |snapshot: SearchProgress| {
            if snapshot.finished {
                finished += 1;
                cancel.store(true, AtomicOrdering::Relaxed);
            }
        };
        let result = solve_best_effort(
            &graph,
            SearchLimits::default(),
            &cancel,
            SearchOptions::default(),
            Some(&mut observer),
        );
        assert!(
            matches!(result, BestEffortOutcome::Stopped { reason: SearchStop::Cancelled, stats } if stats.expanded > 0)
        );
        assert_eq!(finished, 1);
    }
}


/// Initial-goal evaluation can revoke STOP before any executable result is exposed.
struct RevokedInitialGoal<'a> {
    /// External cancellation flag, set from the caller-owned goal predicate.
    cancel: &'a AtomicBool,
}


impl SearchProblem for RevokedInitialGoal<'_> {
    type State = u8;
    type Action = u8;
    type Successors = std::iter::Empty<(u8, u8)>;


    /// Supply one complete initial state without allocation.
    fn initial(&self) -> u8 {
        0
    }


    /// Revoke at the initial-goal callback boundary before claiming success.
    fn is_goal(&self, _: u8) -> bool {
        self.cancel.store(true, AtomicOrdering::Relaxed);
        true
    }


    /// No expansion is required for this initially solved graph.
    fn successors(&self, _: u8) -> Self::Successors {
        std::iter::empty()
    }
}


impl BestEffortProblem for RevokedInitialGoal<'_> {
    /// No fallback benefit is relevant to the initially solved graph.
    fn score(&self, _: u8) -> u32 {
        0
    }
}


/// Even an empty initial winning route is withheld after observed cancellation.
#[test]
fn cancellation_during_initial_goal_evaluation_withholds_success() {
    let cancel = AtomicBool::new(false);
    let result = solve_best_effort(
        &RevokedInitialGoal { cancel: &cancel },
        SearchLimits::default(),
        &cancel,
        SearchOptions::default(),
        None,
    );
    assert!(matches!(
        result,
        BestEffortOutcome::Stopped {
            reason: SearchStop::Cancelled,
            ..
        }
    ));
}


/// Fixture wrapper revokes STOP only after a scored legal prefix was discovered.
struct Cancelling<'a> {
    /// Unchanged no-goal graph.
    graph: Graph,
    /// External cooperative cancellation flag.
    cancel: &'a AtomicBool,
}


impl SearchProblem for Cancelling<'_> {
    type State = u8;
    type Action = u8;
    type Successors = std::vec::IntoIter<(u8, u8)>;


    /// Retain node zero as origin.
    fn initial(&self) -> u8 {
        0
    }


    /// This fixture has no full goal.
    fn is_goal(&self, _: u8) -> bool {
        false
    }


    /// Revocation at node one occurs after its improving prefix was retained.
    fn successors(&self, state: u8) -> Self::Successors {
        if state == 1 {
            self.cancel.store(true, AtomicOrdering::Relaxed);
        }
        self.graph.successors(state)
    }
}


impl BestEffortProblem for Cancelling<'_> {
    /// Retain the wrapped scores without cancellation side effects.
    fn score(&self, state: u8) -> u32 {
        self.graph.score(state)
    }
}


/// STOP never converts an already found prefix into an executable result.
#[test]
fn cancellation_after_a_candidate_exposes_no_actions() {
    let cancel = AtomicBool::new(false);
    let problem = Cancelling {
        graph: no_goal(false),
        cancel: &cancel,
    };
    assert!(matches!(
        solve_best_effort(
            &problem,
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


/// A pre-reconstructed prefix survives elapsed-budget exhaustion without late allocation.
#[test]
fn deadline_retains_checked_prefix_but_fatal_errors_do_not() {
    let cancel = AtomicBool::new(false);
    let problem = no_goal(false);
    let mut engine = Engine::new(SearchLimits::default(), &cancel);
    engine
        .insert(
            Node {
                state: 0,
                parent: None,
                action: None,
                depth: 0,
                cost: 0,
            },
            0,
            NonZeroU32::MIN,
        )
        .unwrap();
    let mut candidate = Candidate {
        actions: Vec::new(),
        score: 0,
        goal: false,
        label: PathLabel::default(),
    };
    consider(
        &problem,
        &mut engine,
        0,
        (1, 1),
        PathLabel { cost: 1, depth: 1 },
        &mut candidate,
        usize::MAX,
    )
    .unwrap();
    engine.started = Instant::now()
        .checked_sub(engine.limits.max_duration)
        .unwrap();
    assert_eq!(engine.poll(), Err(SearchStop::TimeLimit));
    assert!(
        matches!(finish(candidate, engine.stats(), Some(SearchStop::TimeLimit), &cancel), BestEffortOutcome::Partial { actions, proof: PartialProof::Bounded(SearchStop::TimeLimit), .. } if actions == [1])
    );
    for reason in [
        SearchStop::Cancelled,
        SearchStop::AllocationFailure,
        SearchStop::InternalInvariant,
    ] {
        assert!(
            matches!(finish(Candidate { actions: vec![1], score: 1, goal: false, label: PathLabel { cost: 1, depth: 1 } }, SearchStats::default(), Some(reason), &cancel), BestEffortOutcome::Stopped { reason: actual, .. } if reason == actual)
        );
    }
}
