//! The domain release boundary independently rechecks every scored engine prefix.

use super::*;
use crate::games::{
    cards::{Card, Rank},
    freecell::{BoardState, CascadeStack, FOUNDATION_SUITS, Metadata},
};


/// Complete original inventory exposing four aces before automatic progress stops.
fn prefix_input() -> (InitialDeal, Vec<RecordedMove>) {
    let mut columns: [Vec<Card>; 8] = std::array::from_fn(|_| Vec::new());
    for (index, card) in (1..=13)
        .rev()
        .flat_map(|rank| {
            FOUNDATION_SUITS.map(|suit| Card::new(Rank::from_value(rank).unwrap(), suit))
        })
        .enumerate()
    {
        columns[index % 8].push(card);
    }
    for column in &mut columns[4..] {
        column.reverse();
    }
    let deal = InitialDeal::new(
        BoardState::new(
            columns.map(|cards| CascadeStack::from_cards(cards).unwrap()),
            [None; 4],
            [0; 4],
        )
        .unwrap(),
        Metadata::new("prefix-boundary", "synthetic", "unit").unwrap(),
    )
    .unwrap();
    let mut state = Position::initial(&deal);
    let mut actions = Vec::new();
    while let Some(action) = state.automatic() {
        state = state.apply(action).unwrap();
        actions.push(action);
    }
    assert_eq!(actions.len(), 4);
    (deal, actions)
}


/// Model the generic engine's typed proof at the domain replay boundary.
fn exhausted(
    actions: Vec<RecordedMove>,
    score: u32,
) -> crate::search::BestEffortOutcome<RecordedMove> {
    crate::search::BestEffortOutcome::Partial {
        actions,
        score,
        proof: PartialProof::Exhaustive,
        stats: SearchStats::default(),
    }
}


/// Generic proof alone cannot release a wrong score, malformed move or open auto chain.
#[test]
fn freecell_315_domain_partial_release_requires_independent_endpoint_and_exact_score() {
    let (deal, moves) = prefix_input();
    let before = deal.clone();
    let cancel = AtomicBool::new(false);
    let result = replay_outcome(&deal, &cancel, None, exhausted(moves.clone(), 4));
    let BestEffortOutcome::Partial {
        moves: accepted,
        deliberate_moves,
        automatic_moves,
        foundation_cards,
        ..
    } = result
    else {
        panic!("independently verified exhaustive prefix")
    };
    assert_eq!(accepted.as_ref(), moves);
    assert_eq!(
        (deliberate_moves, automatic_moves, foundation_cards),
        (0, 4, 4)
    );
    assert_eq!(deal, before);
    assert!(matches!(
        replay_outcome(&deal, &cancel, None, exhausted(moves.clone(), 5)),
        BestEffortOutcome::Failed {
            failure: Failure::Engine(SearchStop::InternalInvariant),
            ..
        }
    ));
    let mut wrong = moves.clone();
    wrong[0].card_id = u8::MAX;
    assert!(matches!(
        replay_outcome(&deal, &cancel, None, exhausted(wrong, 4)),
        BestEffortOutcome::Failed {
            failure: Failure::Replay(replay::ReplayError {
                kind: replay::ReplayFailure::Illegal,
                ..
            }),
            ..
        }
    ));
    assert!(matches!(
        replay_outcome(&deal, &cancel, None, exhausted(moves[..3].to_vec(), 3)),
        BestEffortOutcome::Failed {
            failure: Failure::Replay(replay::ReplayError {
                kind: replay::ReplayFailure::Incomplete,
                ..
            }),
            ..
        }
    ));
    let bounded = crate::search::BestEffortOutcome::Partial {
        actions: moves.clone(),
        score: 4,
        proof: PartialProof::Bounded(SearchStop::TimeLimit),
        stats: SearchStats::default(),
    };
    assert!(matches!(
        replay_outcome(&deal, &cancel, None, bounded),
        BestEffortOutcome::Failed {
            failure: Failure::Engine(SearchStop::InternalInvariant),
            ..
        }
    ));
    cancel.store(true, std::sync::atomic::Ordering::Release);
    assert!(matches!(
        replay_outcome(&deal, &cancel, None, exhausted(moves, 4)),
        BestEffortOutcome::Cancelled(_)
    ));
}
