//! Public Pyramid fallback checks, independently replayed without input or capture.
//!
//! The original licensed fifteen-action fixture remains unchanged. A separately
//! constructed unique deck traps an apex Ace above all four complementary Queens,
//! demonstrating that impossible-clear pruning must not discard useful removals.

#![cfg(all(feature = "pyramid_search", feature = "pyramid_json"))]

use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

use xsar::{
    games::pyramid::{CompleteDeal, replay, search},
    search::{
        BestEffortOutcome, PartialProof, SearchLimits, SearchMethod, SearchOptions, SearchOutcome,
    },
};


/// Decode the unchanged licensed complete fixture through the public bounded codec.
fn deal() -> CompleteDeal {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pyramid-planning/upstream-15.json");
    xsar::pyramid_json::read_from(File::open(path).expect("licensed fixture"))
        .expect("complete original deck")
}


/// Make an impossible-clear unique deck while retaining an accessible bottom King.
///
/// Every Queen is a descendant of the apex Ace; no complement can legally remove
/// it. Other cards retain their identities, with one King at the bottom right.
fn trapped_apex_deal() -> CompleteDeal {
    let original = deal();
    let mut pool: Vec<_> = original
        .tableau()
        .iter()
        .chain(original.stock())
        .copied()
        .collect();
    let ace = pool
        .iter()
        .position(|card| card.rank().value() == 1)
        .unwrap();
    let mut ordered = vec![pool.remove(ace)];
    for _ in 0..4 {
        let queen = pool
            .iter()
            .position(|card| card.rank().value() == 12)
            .unwrap();
        ordered.push(pool.remove(queen));
    }
    ordered.extend(pool);
    let king = ordered
        .iter()
        .position(|card| card.rank().value() == 13)
        .unwrap();
    ordered.swap(27, king);
    CompleteDeal::try_new(
        ordered[..28].try_into().unwrap(),
        ordered[28..].try_into().unwrap(),
        original.metadata().clone(),
    )
    .expect("synthetic complete unique deck")
}


/// The additive API preserves a full winning route and exact minimum proof.
#[test]
fn full_goal_keeps_exact_fifteen_action_solution_and_independent_clear_replay() {
    let deal = deal();
    let result = search::solve_best_effort_with_options(
        &deal,
        SearchLimits::default(),
        &AtomicBool::new(false),
        SearchOptions::default(),
        None,
    );
    let BestEffortOutcome::Solved {
        actions,
        minimum_proven,
        ..
    } = result
    else {
        panic!("expected independently known full solution: {result:?}");
    };
    assert!(minimum_proven);
    assert_eq!(actions.len(), 15);
    let trace = replay::replay(&deal, &actions, true).expect("independent clear replay");
    assert_eq!(trace.last().unwrap().remaining_tableau, 0);
}


/// Two necessary tableau Kings plus thirteen pairs independently bound this fixture at fifteen.
#[test]
fn explicit_match_cost_solves_the_original_fixture_with_independent_replay() {
    use xsar::games::pyramid::planning::RecordedAction;

    let deal = deal();
    let result = search::solve_best_effort_with_cost(
        &deal,
        SearchLimits::default(),
        &AtomicBool::new(false),
        SearchOptions::default(),
        search::PyramidCost::CardMatches,
        None,
    );
    let BestEffortOutcome::Solved {
        actions,
        minimum_proven,
        ..
    } = result
    else {
        panic!("expected match-cost solution: {result:?}")
    };
    assert!(minimum_proven);
    assert_eq!(
        deal.tableau()
            .iter()
            .filter(|card| card.rank().value() == 13)
            .count(),
        2
    );
    let matches = actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                RecordedAction::Match(_, _) | RecordedAction::King(_)
            )
        })
        .count();
    assert_eq!(matches, 15);
    let trace = replay::replay(&deal, &actions, true).expect("independent match-cost replay");
    assert_eq!(trace.last().unwrap().remaining_tableau, 0);
}


/// A resource cutoff returns only a legal, explicitly unproven positive removal prefix.
#[test]
fn bounded_full_search_retains_a_replayable_removal_prefix_with_shared_counters() {
    let deal = deal();
    let limits = SearchLimits {
        max_states: 1_024,
        max_expansions: 1,
        ..SearchLimits::default()
    };
    let result = search::solve_best_effort_with_options(
        &deal,
        limits,
        &AtomicBool::new(false),
        SearchOptions::default(),
        None,
    );
    let BestEffortOutcome::Partial {
        actions,
        score,
        proof,
        stats,
    } = result
    else {
        panic!("expected bounded partial: {result:?}");
    };
    let trace = replay::replay(&deal, &actions, false).expect("independent partial replay");
    let remaining = trace.last().unwrap().remaining_tableau;
    assert!(remaining > 0 && remaining < 28);
    assert_eq!(score, u32::from(28 - remaining));
    assert!(matches!(proof, PartialProof::Bounded(_)));
    assert!(stats.states <= limits.max_states && stats.expanded <= limits.max_expansions);
}


/// Sound full-clear impossibility cannot suppress reachable tableau-removal routes.
#[test]
fn impossible_apex_still_explores_unpruned_removals_and_never_claims_a_clear() {
    let deal = trapped_apex_deal();
    let limits = SearchLimits {
        max_states: 1_024,
        max_expansions: 256,
        max_duration: Duration::from_secs(2),
        ..SearchLimits::default()
    };
    assert!(matches!(
        search::solve(&deal, limits, &AtomicBool::new(false), SearchMethod::AStar),
        SearchOutcome::Unsolvable { .. }
    ));
    let result = search::solve_best_effort_with_options(
        &deal,
        limits,
        &AtomicBool::new(false),
        SearchOptions::default(),
        None,
    );
    let BestEffortOutcome::Partial {
        actions,
        score,
        stats,
        ..
    } = result
    else {
        panic!("expected non-clear removal route: {result:?}");
    };
    assert!(!actions.is_empty() && score > 0 && score < 28);
    let trace = replay::replay(&deal, &actions, false).expect("independent trapped-apex replay");
    assert_eq!(
        score,
        u32::from(28 - trace.last().unwrap().remaining_tableau)
    );
    assert!(replay::replay(&deal, &actions, true).is_err());
    assert!(stats.states > 1 && stats.states <= limits.max_states);
    assert!(stats.expanded > 0 && stats.expanded <= limits.max_expansions);
}
