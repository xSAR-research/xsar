//! Independent physical replay, without consulting search mutation or canonical keys.
//!
//! Every original identity is conserved after each move. Only an independently
//! checked complete52-card foundation endpoint produces a successful summary.

use super::rules::{Pile, RecordedMove};
use super::{BoardState, FOUNDATION_SUITS, InitialDeal};
use crate::games::cards::{Card, Rank, Suit};
use std::sync::atomic::{AtomicBool, Ordering};


/// First independently rejected route step (route length denotes the endpoint).
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Free Cell replay step {step}: {kind}")]
pub struct ReplayError {
    /// Zero-based transfer, or final route length for endpoint errors.
    pub step: usize,
    /// Independent first-failure category.
    pub kind: ReplayFailure,
}


/// No failure supplies a verified route prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReplayFailure {
    /// Illegal exposure/destination, identity or deterministic automatic tag.
    #[error("illegal physical transfer or automatic tag")]
    Illegal,
    /// Full-deck conservation or checked storage failed.
    #[error("card inventory or storage invariant failed")]
    Inventory,
    /// Unbanked cards remain at the endpoint.
    #[error("route is incomplete")]
    Incomplete,
    /// Cooperative cancellation between bounded transfers.
    #[error("cancelled")]
    Cancelled,
}


/// Independently checked move-cost and complete endpoint counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplaySummary {
    /// Deliberate cost-one transfers.
    pub deliberate: usize,
    /// Separately recorded cost-zero automatic transfers.
    pub automatic: usize,
    /// Canonical C,D,H,S foundation progress, all thirteen on success.
    pub foundations: [u8; 4],
}


/// Verify a complete route from an immutable original physical deal.
///
/// # Errors
/// Returns the first illegal, unconserved or incomplete step/endpoint.
pub fn verify(deal: &InitialDeal, moves: &[RecordedMove]) -> Result<ReplaySummary, ReplayError> {
    verify_with_control(deal, moves, &AtomicBool::new(false))
}


/// Independently replay with cancellation between every physical transfer.
///
/// # Errors
/// Cancellation, illegal moves/tags, inventory/storage failure or incomplete goal.
pub fn verify_with_control(
    deal: &InitialDeal,
    moves: &[RecordedMove],
    cancel: &AtomicBool,
) -> Result<ReplaySummary, ReplayError> {
    verify_with_checkpoint(deal, moves, cancel, None)
}


/// Replay each bounded transfer with the planner's same cooperative pause gate.
pub(super) fn verify_with_checkpoint(
    deal: &InitialDeal,
    moves: &[RecordedMove],
    cancel: &AtomicBool,
    checkpoint: Option<&dyn Fn() -> std::time::Duration>,
) -> Result<ReplaySummary, ReplayError> {
    let mut board = deal.board().clone();
    let mut deliberate = 0_usize;
    let mut automatic = 0_usize;
    for (step, &action) in moves.iter().enumerate() {
        let error = |kind| ReplayError { step, kind };
        if cancel.load(Ordering::Acquire) {
            return Err(error(ReplayFailure::Cancelled));
        }
        if let Some(check) = checkpoint {
            let _ = check();
        }
        if cancel.load(Ordering::Acquire) {
            return Err(error(ReplayFailure::Cancelled));
        }
        let card = decode(action.card_id).ok_or_else(|| error(ReplayFailure::Illegal))?;
        if exposed(&board, action.from) != Some(card) || action.from == action.to {
            return Err(error(ReplayFailure::Illegal));
        }
        let next_auto = automatic_move(&board);
        if (action.is_auto && next_auto != Some(action)) || (!action.is_auto && next_auto.is_some())
        {
            return Err(error(ReplayFailure::Illegal));
        }
        match action.to {
            Pile::Foundation(suit) => {
                let next = board
                    .foundations
                    .get(usize::from(suit))
                    .copied()
                    .and_then(|n| n.checked_add(1));
                if FOUNDATION_SUITS.get(usize::from(suit)) != Some(&card.suit())
                    || next != Some(card.rank().value())
                {
                    return Err(error(ReplayFailure::Illegal));
                }
            }
            Pile::Cascade(column) => {
                let stack = board
                    .cascades
                    .get(usize::from(column))
                    .ok_or_else(|| error(ReplayFailure::Illegal))?;
                if stack.len() >= super::CASCADE_CAPACITY
                    || stack.iter().last().is_some_and(|top| {
                        top.rank().value() != card.rank().value() + 1
                            || is_red(top.suit()) == is_red(card.suit())
                    })
                {
                    return Err(error(ReplayFailure::Illegal));
                }
            }
            Pile::Cell(cell) => {
                if !matches!(action.from, Pile::Cascade(_))
                    || board.cells.get(usize::from(cell)) != Some(&None)
                {
                    return Err(error(ReplayFailure::Illegal));
                }
            }
        }
        match action.from {
            Pile::Cascade(column) => {
                board
                    .cascades
                    .get_mut(usize::from(column))
                    .ok_or_else(|| error(ReplayFailure::Illegal))?
                    .pop()
                    .map_err(|_| error(ReplayFailure::Inventory))?;
            }
            Pile::Cell(cell) => {
                board
                    .cells
                    .get_mut(usize::from(cell))
                    .ok_or_else(|| error(ReplayFailure::Illegal))?
                    .take();
            }
            Pile::Foundation(_) => return Err(error(ReplayFailure::Illegal)),
        }
        match action.to {
            Pile::Cascade(column) => board
                .cascades
                .get_mut(usize::from(column))
                .ok_or_else(|| error(ReplayFailure::Illegal))?
                .push(card)
                .map_err(|_| error(ReplayFailure::Inventory))?,
            Pile::Cell(cell) => {
                *board
                    .cells
                    .get_mut(usize::from(cell))
                    .ok_or_else(|| error(ReplayFailure::Illegal))? = Some(card)
            }
            Pile::Foundation(suit) => {
                *board
                    .foundations
                    .get_mut(usize::from(suit))
                    .ok_or_else(|| error(ReplayFailure::Illegal))? = card.rank().value()
            }
        }
        if !inventory(&board) {
            return Err(error(ReplayFailure::Inventory));
        }
        let count = if action.is_auto {
            &mut automatic
        } else {
            &mut deliberate
        };
        *count = count
            .checked_add(1)
            .ok_or_else(|| error(ReplayFailure::Inventory))?;
    }
    if cancel.load(Ordering::Acquire) {
        return Err(ReplayError {
            step: moves.len(),
            kind: ReplayFailure::Cancelled,
        });
    }
    if board.foundations != [13; 4]
        || board.cells != [None; 4]
        || board.cascades.iter().any(|c| !c.is_empty())
    {
        return Err(ReplayError {
            step: moves.len(),
            kind: ReplayFailure::Incomplete,
        });
    }
    Ok(ReplaySummary {
        deliberate,
        automatic,
        foundations: board.foundations,
    })
}


/// Decode the closed identity independently of the search representation.
fn decode(id: u8) -> Option<Card> {
    if id >= 52 {
        return None;
    }
    Some(Card::new(
        Rank::from_value(id % 13 + 1)?,
        *FOUNDATION_SUITS.get(usize::from(id / 13))?,
    ))
}


/// Foundations are irreversible and can never expose a source card.
fn exposed(board: &BoardState, source: Pile) -> Option<Card> {
    match source {
        Pile::Cascade(c) => board.cascades.get(usize::from(c))?.iter().last(),
        Pile::Cell(c) => board.cells.get(usize::from(c)).copied().flatten(),
        Pile::Foundation(_) => None,
    }
}


/// Colour independently derived from the shared closed suit domain.
fn is_red(suit: Suit) -> bool {
    matches!(suit, Suit::Diamonds | Suit::Hearts)
}


/// Independently compute the approved first safe auto, in physical scan order.
fn automatic_move(board: &BoardState) -> Option<RecordedMove> {
    for source in (0..8).map(Pile::Cascade).chain((0..4).map(Pile::Cell)) {
        let Some(card) = exposed(board, source) else {
            continue;
        };
        let suit = FOUNDATION_SUITS.iter().position(|s| *s == card.suit())?;
        if board.foundations.get(suit).copied()?.checked_add(1) != Some(card.rank().value()) {
            continue;
        }
        let safe = card.rank().value() <= 2
            || FOUNDATION_SUITS
                .iter()
                .zip(board.foundations)
                .filter(|(other, _)| is_red(**other) != is_red(card.suit()))
                .all(|(_, count)| count >= card.rank().value() - 1);
        if safe {
            return Some(RecordedMove {
                card_id: u8::try_from(suit * 13).ok()? + card.rank().value() - 1,
                from: source,
                to: Pile::Foundation(u8::try_from(suit).ok()?),
                is_auto: true,
            });
        }
    }
    None
}


/// Recount every live and already-banked identity after each independent move.
fn inventory(board: &BoardState) -> bool {
    let mut mask = 0_u64;
    let mut add = |card: Card| {
        let Some(suit) = FOUNDATION_SUITS.iter().position(|s| *s == card.suit()) else {
            return false;
        };
        let id = suit * 13 + usize::from(card.rank().value()) - 1;
        let Some(bit) = 1_u64.checked_shl(u32::try_from(id).unwrap_or(u32::MAX)) else {
            return false;
        };
        if mask & bit != 0 {
            return false;
        }
        mask |= bit;
        true
    };
    for stack in &board.cascades {
        for card in stack.iter() {
            if !add(card) {
                return false;
            }
        }
    }
    for card in board.cells.iter().flatten() {
        if !add(*card) {
            return false;
        }
    }
    for (suit, count) in FOUNDATION_SUITS.into_iter().zip(board.foundations) {
        if count > 13 {
            return false;
        }
        for value in 1..=count {
            let Some(rank) = Rank::from_value(value) else {
                return false;
            };
            if !add(Card::new(rank, suit)) {
                return false;
            }
        }
    }
    mask == (1_u64 << 52) - 1
}
