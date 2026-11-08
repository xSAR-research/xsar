//! Public Free Cell solver/replay checks; acceptance input is supplied explicitly.
#![cfg(feature = "freecell_search")]

use std::sync::atomic::AtomicBool;
use xsar::games::{
    cards::{Card, Rank},
    freecell::{self, BoardState, CascadeStack, FOUNDATION_SUITS, InitialDeal, Metadata},
};


/// Synthetic descending interleaved suits expose only safe automatic transfers.
fn automatic_deal() -> InitialDeal {
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
    InitialDeal::new(
        BoardState::new(
            columns.map(|c| CascadeStack::from_cards(c).unwrap()),
            [None; 4],
            [0; 4],
        )
        .unwrap(),
        Metadata::new("automatic", "synthetic", "unit").unwrap(),
    )
    .unwrap()
}


/// Public solve produces only a separately replayed full route and preserves input.
#[test]
fn complete_automatic_route_and_tampering() {
    let deal = automatic_deal();
    let before = deal.board().clone();
    let freecell::search::Outcome::Solved {
        mut moves, replay, ..
    } = freecell::search::solve_with_control(&deal, &AtomicBool::new(false), None, None)
    else {
        panic!("complete synthetic route");
    };
    assert_eq!(
        (
            moves.len(),
            replay.deliberate,
            replay.automatic,
            replay.foundations
        ),
        (52, 0, 52, [13; 4])
    );
    assert_eq!(deal.board(), &before);
    assert_eq!(freecell::replay::verify(&deal, &moves).unwrap(), replay);
    let original_route = moves.clone();
    moves[0].is_auto = false;
    assert_eq!(freecell::replay::verify(&deal, &moves).unwrap_err().step, 0);
    for mutation in 0..3 {
        let mut changed = original_route.clone();
        match mutation {
            0 => changed[0].card_id = 51,
            1 => changed[0].from = freecell::rules::Pile::Foundation(0),
            _ => changed[0].to = freecell::rules::Pile::Cell(0),
        }
        let error = freecell::replay::verify(&deal, &changed).unwrap_err();
        assert_eq!(
            (error.step, error.kind),
            (0, freecell::replay::ReplayFailure::Illegal)
        );
    }
    let incomplete = &original_route[..original_route.len() - 1];
    assert_eq!(
        freecell::replay::verify(&deal, incomplete)
            .unwrap_err()
            .kind,
        freecell::replay::ReplayFailure::Incomplete
    );
    assert_eq!(
        freecell::replay::verify_with_control(&deal, &original_route, &AtomicBool::new(true))
            .unwrap_err()
            .kind,
        freecell::replay::ReplayFailure::Cancelled
    );
    let again = freecell::search::solve_with_control(&deal, &AtomicBool::new(false), None, None);
    let freecell::search::Outcome::Solved { moves: again, .. } = again else {
        panic!("deterministic complete route")
    };
    assert_eq!(again, original_route);
}


/// Explicit acceptance run uses the actual exported deal, never a synthetic substitute.
#[cfg(feature = "freecell_json")]
#[test]
#[ignore = "requires FREECELL_ACCEPTANCE_DEAL naming the operator export"]
fn exported_deal_complete_route() {
    let path = std::env::var_os("FREECELL_ACCEPTANCE_DEAL").expect("explicit exported deal path");
    let deal = xsar::freecell_json::read_from(std::fs::File::open(path).unwrap()).unwrap();
    let original = deal.board().clone();
    let mut last = std::time::Instant::now();
    let mut progress = |p: freecell::search::Progress| {
        if last.elapsed().as_secs() >= 2 || p.search.finished {
            eprintln!(
                "expanded={} states={} frontier={} banked={} elapsed={:?}",
                p.search.stats.expanded,
                p.search.stats.states,
                p.search.stats.peak_frontier,
                p.best_foundation_cards,
                p.search.stats.elapsed
            );
            last = std::time::Instant::now();
        }
    };
    let outcome = freecell::search::solve_with_control(
        &deal,
        &AtomicBool::new(false),
        Some(&mut progress),
        None,
    );
    let freecell::search::Outcome::Solved {
        moves,
        replay,
        stats,
    } = outcome
    else {
        panic!("acceptance requires complete route: {outcome:?}");
    };
    assert_eq!(replay.foundations, [13; 4]);
    assert_eq!(deal.board(), &original);
    assert_eq!(freecell::replay::verify(&deal, &moves).unwrap(), replay);
    eprintln!(
        "COMPLETE deliberate={} automatic={} transfers={} expanded={} states={} elapsed={:?}",
        replay.deliberate,
        replay.automatic,
        moves.len(),
        stats.expanded,
        stats.states,
        stats.elapsed
    );
}


/// Pause checkpoints remain active after search completion, throughout independent replay.
#[test]
fn replay_uses_the_same_checkpoint_and_can_cancel_before_route_publication() {
    use std::{cell::Cell, sync::atomic::Ordering, time::Duration};
    for cancel_replay in [false, true] {
        let search_finished = Cell::new(false);
        let replay_checks = Cell::new(0);
        let cancel = AtomicBool::new(false);
        let mut progress = |p: freecell::search::Progress| {
            if p.search.finished {
                search_finished.set(true);
            }
        };
        let checkpoint = || {
            if search_finished.get() {
                replay_checks.set(replay_checks.get() + 1);
                if cancel_replay {
                    cancel.store(true, Ordering::Release);
                }
            }
            Duration::ZERO
        };
        let outcome = freecell::search::solve_with_control(
            &automatic_deal(),
            &cancel,
            Some(&mut progress),
            Some(&checkpoint),
        );
        if cancel_replay {
            assert!(matches!(outcome, freecell::search::Outcome::Cancelled(_)));
            assert_eq!(replay_checks.get(), 1);
        } else {
            assert!(matches!(outcome, freecell::search::Outcome::Solved { .. }));
            assert_eq!(replay_checks.get(), 52);
        }
    }
}
