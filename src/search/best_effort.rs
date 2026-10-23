//! Optional scored fallback over the same bounded, checked nonnegative-cost engine.
//!
//! Full-goal A* retains sound pruning. If it cannot return a goal, an unpruned
//! phase uses only the remaining aggregate record, expansion and time budgets.
//! A retained legal prefix survives ordinary resource cutoffs, never cancellation,
//! allocation failure or a broken invariant. Exhaustion proves maximum score and
//! shortest cost among equal-score prefixes; a cutoff proves neither maximum.

use super::*;


#[cfg(test)]
mod tests;


/// Pure problem with an additional deterministic, state-local fallback score.
///
/// Larger scores are preferred; equal scores prefer lower declared cost, then depth.
/// The score must remain stable for equal states. It grants no goal authority.
pub trait BestEffortProblem: SearchProblem {
    /// Measure the fallback benefit of a state without changing the problem.
    fn score(&self, state: Self::State) -> u32;
}


/// Evidence supporting a scored prefix when a full-goal route was not returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartialProof {
    /// Unpruned search exhausted: maximum score and minimum cost at that score.
    Exhaustive,
    /// Best discovered prefix only; the named aggregate resource bound intervened.
    Bounded(SearchStop),
}


/// Full-goal, explicitly partial, or unusable outcome from one combined budget.
#[derive(Debug, Eq, PartialEq)]
pub enum BestEffortOutcome<A> {
    /// A goal-reaching route, independently replayable by the caller.
    Solved {
        /// Ordered logical actions, including an empty route for an initial goal.
        actions: Vec<A>,
        /// True only for minimum declared cost at a popped exact A* or Dijkstra goal.
        minimum_proven: bool,
        /// Aggregate work and elapsed time across both phases.
        stats: SearchStats,
    },
    /// A legal scored prefix; an empty prefix can mean no beneficial move exists.
    Partial {
        /// Ordered actions ending at the scored state; no full-goal claim follows.
        actions: Vec<A>,
        /// Actual caller-defined score at the endpoint.
        score: u32,
        /// Exhaustive maximum proof or explicitly bounded best-found status.
        proof: PartialProof,
        /// Aggregate work and elapsed time across both phases.
        stats: SearchStats,
    },
    /// Cancellation or a failure for which no executable prefix is exposed.
    Stopped {
        /// Cancellation, allocation, invariant failure, or a bound before progress.
        reason: SearchStop,
        /// Aggregate actual work and elapsed time.
        stats: SearchStats,
    },
}


/// Owned best prefix, reconstructed before a possible deadline rather than afterwards.
struct Candidate<A> {
    /// Prefix actions; retained until a replacement has been completely checked.
    actions: Vec<A>,
    /// Stable caller score of the endpoint.
    score: u32,
    /// Whether the endpoint itself satisfies the primary goal.
    goal: bool,
    /// Independent declared cost and route depth of the retained prefix.
    label: PathLabel,
}


/// Add counters while retaining a single invocation clock and peak frontier.
fn cumulative(previous: SearchStats, current: SearchStats) -> SearchStats {
    SearchStats {
        expanded: previous.expanded.saturating_add(current.expanded),
        generated: previous.generated.saturating_add(current.generated),
        states: previous.states.saturating_add(current.states),
        peak_frontier: previous.peak_frontier.max(current.peak_frontier),
        elapsed: current.elapsed,
    }
}


/// Check STOP and deadline around a due cumulative observer notification.
fn poll<S: Copy + Eq + Hash, A: Copy>(
    engine: &Engine<'_, S, A>,
    previous: SearchStats,
    progress: &mut ProgressReporter<'_>,
) -> Result<(), SearchStop> {
    engine.poll()?;
    if progress.report(cumulative(previous, engine.stats()), false) {
        engine.poll()?;
    }
    Ok(())
}


/// Ordinary resource cutoffs may return an already checked prefix.
fn permits_prefix(reason: SearchStop) -> bool {
    matches!(
        reason,
        SearchStop::StateLimit
            | SearchStop::ExpansionLimit
            | SearchStop::MemoryLimit
            | SearchStop::TimeLimit
    )
}


/// Reconstruct an improving successor before pruning or later resource exhaustion.
///
/// Both old and replacement output capacities are accounted for. Cancellation,
/// allocation and invariant errors preserve the old value but remain fatal.
fn consider<P: BestEffortProblem>(
    problem: &P,
    engine: &mut Engine<'_, P::State, P::Action>,
    parent: usize,
    successor: (P::Action, P::State),
    label: PathLabel,
    candidate: &mut Candidate<P::Action>,
    memory_allowance: usize,
) -> Result<(), SearchStop> {
    let (action, next) = successor;
    engine.poll()?;
    let score = problem.score(next);
    let goal = problem.is_goal(next);
    let improves = if goal {
        !candidate.goal || label < candidate.label
    } else {
        !candidate.goal
            && (score > candidate.score || (score == candidate.score && label < candidate.label))
    };
    if !improves {
        return engine.poll();
    }
    let mut quiet = ProgressReporter::new(None);
    let mut actions = engine.route(parent, &mut quiet)?.into_vec();
    actions
        .try_reserve_exact(1)
        .map_err(|_| SearchStop::AllocationFailure)?;
    actions.push(action);
    let bytes = actions.capacity().saturating_mul(size_of::<P::Action>());
    let old_bytes = candidate
        .actions
        .capacity()
        .saturating_mul(size_of::<P::Action>());
    let allocated = estimated_bytes::<P::State, P::Action>(
        engine.nodes.capacity(),
        engine.frontier.capacity(),
        engine.best.capacity(),
    )
    .saturating_add(old_bytes)
    .saturating_add(bytes);
    if allocated > memory_allowance {
        return Err(SearchStop::MemoryLimit);
    }
    engine.poll()?;
    *candidate = Candidate {
        actions,
        score,
        goal,
        label,
    };
    engine.limits.max_memory_bytes = memory_allowance.saturating_sub(bytes);
    Ok(())
}


/// Run one phase; absent options disable full-goal pruning and heuristic guidance.
///
/// Returning `None` means exhaustive frontier depletion, not a resource cutoff.
fn phase<P: BestEffortProblem>(
    problem: &P,
    engine: &mut Engine<'_, P::State, P::Action>,
    guidance: Option<SearchOptions>,
    previous: SearchStats,
    progress: &mut ProgressReporter<'_>,
    candidate: &mut Candidate<P::Action>,
    memory_allowance: usize,
) -> Result<Option<Vec<P::Action>>, SearchStop> {
    poll(engine, previous, progress)?;
    let initial = problem.initial();
    if problem.is_goal(initial) {
        engine.poll()?;
        return Ok(Some(Vec::new()));
    }
    let guidance = guidance.filter(|options| options.method == SearchMethod::AStar);
    let guided = guidance.is_some();
    if guided && problem.impossible(initial) {
        return Ok(None);
    }
    let weight = guidance.map_or(NonZeroU32::MIN, |options| options.heuristic_weight);
    let heuristic = if guided {
        problem.heuristic(initial)
    } else {
        0
    };
    engine.insert(
        Node {
            state: initial,
            parent: None,
            action: None,
            depth: 0,
            cost: 0,
        },
        heuristic,
        weight,
    )?;
    loop {
        poll(engine, previous, progress)?;
        let Some(entry) = engine.frontier.pop() else {
            return Ok(None);
        };
        let node = engine
            .nodes
            .get(entry.node)
            .copied()
            .ok_or(SearchStop::InternalInvariant)?;
        if engine.best.get(&node.state) != Some(&node.label()) {
            continue;
        }
        if problem.is_goal(node.state) {
            let mut quiet = ProgressReporter::new(None);
            let actions = engine.route(entry.node, &mut quiet)?.into_vec();
            poll(engine, previous, progress)?;
            return Ok(Some(actions));
        }
        if engine.stats.expanded >= engine.limits.max_expansions {
            return Err(SearchStop::ExpansionLimit);
        }
        engine.stats.expanded += 1;
        let mut successors = problem.successors(node.state);
        loop {
            poll(engine, previous, progress)?;
            let Some((action, next)) = successors.next() else {
                break;
            };
            poll(engine, previous, progress)?;
            engine.stats.generated = engine.stats.generated.saturating_add(1);
            let label = node
                .label()
                .successor(problem.action_cost(node.state, action))?;
            consider(
                problem,
                engine,
                entry.node,
                (action, next),
                label,
                candidate,
                memory_allowance,
            )?;
            if guided && problem.impossible(next) {
                continue;
            }
            if engine.best.get(&next).is_some_and(|known| *known <= label) {
                continue;
            }
            let heuristic = if guided { problem.heuristic(next) } else { 0 };
            engine.insert(
                Node {
                    state: next,
                    parent: Some(entry.node),
                    action: Some(action),
                    depth: label.depth,
                    cost: label.cost,
                },
                heuristic,
                weight,
            )?;
        }
    }
}


/// Convert only safe cutoffs into a prefix, checking STOP again at the boundary.
fn finish<A>(
    candidate: Candidate<A>,
    stats: SearchStats,
    reason: Option<SearchStop>,
    cancel: &AtomicBool,
) -> BestEffortOutcome<A> {
    let reason = if cancel.load(AtomicOrdering::Relaxed) {
        Some(SearchStop::Cancelled)
    } else {
        reason
    };
    if let Some(reason) = reason.filter(|reason| !permits_prefix(*reason)) {
        return BestEffortOutcome::Stopped { reason, stats };
    }
    if candidate.goal {
        return BestEffortOutcome::Solved {
            actions: candidate.actions,
            minimum_proven: false,
            stats,
        };
    }
    BestEffortOutcome::Partial {
        actions: candidate.actions,
        score: candidate.score,
        proof: reason.map_or(PartialProof::Exhaustive, PartialProof::Bounded),
        stats,
    }
}


/// Seek a full goal first, then maximise a state score under one combined budget.
///
/// Existing [`solve_with_options`] remains unchanged and never returns partial
/// routes. This additive API first uses its requested goal ordering and pruning.
/// If no goal is returned, remaining aggregate records, expansions and time are
/// available to an unpruned Dijkstra phase. Phase-one collections are released
/// before phase two. A legal best prefix is retained before the deadline; output
/// buffers count against the conservative memory allowance. A bounded prefix is
/// best-found only. Exhaustive unpruned completion proves the maximum score and
/// shortest prefix at that score. Neither form grants a primary-goal claim.
///
/// Cancellation, allocation failure and invariant failure return `Stopped`
/// without actions. The observer gets one initial and one final cumulative
/// snapshot, plus throttled periodic snapshots. Callback and problem contracts,
/// cooperative timing and memory-estimate limitations match [`solve_with_options`].
///
/// # Panics
/// Caller problem/observer panics propagate; normal-return reporting is not promised on unwind.
pub fn solve_best_effort<P: BestEffortProblem>(
    problem: &P,
    limits: SearchLimits,
    cancel: &AtomicBool,
    options: SearchOptions,
    observer: Option<&mut dyn FnMut(SearchProgress)>,
) -> BestEffortOutcome<P::Action> {
    let mut engine = Engine::new(limits, cancel);
    let mut progress = ProgressReporter::new(observer);
    progress.report(engine.stats(), false);
    let mut candidate = Candidate {
        actions: Vec::new(),
        score: 0,
        goal: false,
        label: PathLabel::default(),
    };
    let outcome = if let Err(reason) = engine.poll() {
        BestEffortOutcome::Stopped {
            reason,
            stats: engine.stats(),
        }
    } else {
        candidate.score = problem.score(problem.initial());
        let first = phase(
            problem,
            &mut engine,
            Some(options),
            SearchStats::default(),
            &mut progress,
            &mut candidate,
            limits.max_memory_bytes,
        );
        match first {
            Ok(Some(actions)) => BestEffortOutcome::Solved {
                actions,
                minimum_proven: options.method == SearchMethod::Dijkstra
                    || options.heuristic_weight == NonZeroU32::MIN,
                stats: engine.stats(),
            },
            Ok(None) if options.method == SearchMethod::Dijkstra => {
                finish(candidate, engine.stats(), None, cancel)
            }
            Err(reason) if !permits_prefix(reason) => {
                finish(candidate, engine.stats(), Some(reason), cancel)
            }
            first => {
                let previous = engine.stats();
                let mut remaining = limits;
                remaining.max_states = limits.max_states.saturating_sub(previous.states);
                remaining.max_expansions = limits.max_expansions.saturating_sub(previous.expanded);
                remaining.max_memory_bytes = limits.max_memory_bytes.saturating_sub(
                    candidate
                        .actions
                        .capacity()
                        .saturating_mul(size_of::<P::Action>()),
                );
                let started = engine.started;
                drop(engine);
                let mut fallback = Engine::new(remaining, cancel);
                fallback.started = started;
                let first_reason = first.err();
                if let Err(reason) = fallback.poll() {
                    finish(
                        candidate,
                        cumulative(previous, fallback.stats()),
                        Some(reason),
                        cancel,
                    )
                } else if remaining.max_states == 0 || remaining.max_expansions == 0 {
                    let reason = first_reason.unwrap_or(if remaining.max_states == 0 {
                        SearchStop::StateLimit
                    } else {
                        SearchStop::ExpansionLimit
                    });
                    finish(
                        candidate,
                        cumulative(previous, fallback.stats()),
                        Some(reason),
                        cancel,
                    )
                } else {
                    match phase(
                        problem,
                        &mut fallback,
                        None,
                        previous,
                        &mut progress,
                        &mut candidate,
                        limits.max_memory_bytes,
                    ) {
                        Ok(Some(actions)) => BestEffortOutcome::Solved {
                            actions,
                            minimum_proven: true,
                            stats: cumulative(previous, fallback.stats()),
                        },
                        Ok(None) => finish(
                            candidate,
                            cumulative(previous, fallback.stats()),
                            None,
                            cancel,
                        ),
                        Err(reason) => finish(
                            candidate,
                            cumulative(previous, fallback.stats()),
                            Some(reason),
                            cancel,
                        ),
                    }
                }
            }
        }
    };
    let stats = match &outcome {
        BestEffortOutcome::Solved { stats, .. }
        | BestEffortOutcome::Partial { stats, .. }
        | BestEffortOutcome::Stopped { stats, .. } => *stats,
    };
    progress.report(stats, true);
    if cancel.load(AtomicOrdering::Relaxed) {
        BestEffortOutcome::Stopped {
            reason: SearchStop::Cancelled,
            stats,
        }
    } else {
        outcome
    }
}
