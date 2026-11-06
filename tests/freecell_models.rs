//! Public model invariants also run with only the optional model feature.
#![cfg(feature = "freecell")]
use xsar::games::{
    cards::{Card, Rank, Suit},
    freecell::*,
};
fn deal() -> InitialDeal {
    let cards: Vec<_> = Suit::ALL
        .into_iter()
        .flat_map(|s| Rank::ALL.into_iter().map(move |r| Card::new(r, s)))
        .collect();
    let mut offset = 0;
    let stacks = INITIAL_LENGTHS.map(|n| {
        let stack = CascadeStack::from_cards(cards[offset..offset + n].iter().copied()).unwrap();
        offset += n;
        stack
    });
    InitialDeal::new(
        BoardState::new(stacks, [None; 4], [0; 4]).unwrap(),
        Metadata::new("test", "native", "fixture").unwrap(),
    )
    .unwrap()
}
#[test]
fn stack_bounds_preserve_failed_mutation() {
    let mut stack = CascadeStack::default();
    let card = Card::new(Rank::Ace, Suit::Clubs);
    assert!(stack.pop().is_err());
    for _ in 0..20 {
        stack.push(card).unwrap();
    }
    let before = stack.clone();
    assert!(stack.push(card).is_err());
    assert_eq!(stack, before);
    assert!(stack.get(20).is_err());
    for _ in 0..20 {
        assert_eq!(stack.pop().unwrap(), card);
    }
    assert!(stack.is_empty());
    assert!(CascadeLocation::new(8, 0).is_err());
    assert!(CascadeLocation::new(0, 20).is_err());
}

#[test]
fn initial_deal_preserves_physical_order_and_rejects_duplicates() {
    let original = deal();
    let mut stacks = original.board().cascades().clone();
    let card = stacks[0].get(0).unwrap();
    stacks[1].pop().unwrap();
    stacks[1].push(card).unwrap();
    let result = InitialDeal::new(
        BoardState::new(stacks, [None; 4], [0; 4]).unwrap(),
        original.metadata().clone(),
    );
    assert!(matches!(result, Err(ModelError::Duplicate { .. })));
    let mut stacks = original.board().cascades().clone();
    stacks.swap(0, 4);
    assert!(
        InitialDeal::new(
            BoardState::new(stacks, [None; 4], [0; 4]).unwrap(),
            original.metadata().clone()
        )
        .is_err()
    );
    assert!(
        BoardState::new(
            original.board().cascades().clone(),
            [None; 4],
            [14, 0, 0, 0]
        )
        .is_err()
    );
    assert!(
        InitialDeal::new(
            BoardState::new(
                original.board().cascades().clone(),
                [Some(card), None, None, None],
                [0; 4]
            )
            .unwrap(),
            original.metadata().clone()
        )
        .is_err()
    );
}

#[test]
fn metadata_bounds_and_duplicate_locations_are_exact() {
    assert!(Metadata::new("x".repeat(128), "p", "s").is_ok());
    assert!(Metadata::new("é".repeat(64), "p", "s").is_ok());
    for id in [
        String::new(),
        "é".repeat(65),
        "a\\b".into(),
        "a\nb".into(),
        "\0".into(),
    ] {
        assert!(Metadata::new(id, "p", "s").is_err());
    }
    let original = deal();
    let mut stacks = original.board().cascades().clone();
    let card = stacks[0].get(0).unwrap();
    stacks[7].pop().unwrap();
    stacks[7].push(card).unwrap();
    match InitialDeal::new(
        BoardState::new(stacks, [None; 4], [0; 4]).unwrap(),
        original.metadata().clone(),
    )
    .unwrap_err()
    {
        ModelError::Duplicate {
            card: actual,
            first,
            second,
        } => {
            assert_eq!(actual, card);
            assert_eq!(
                first,
                Location::Cascade(CascadeLocation::new(0, 0).unwrap())
            );
            assert_eq!(
                second,
                Location::Cascade(CascadeLocation::new(7, 5).unwrap())
            );
        }
        other => panic!("unexpected model failure: {other}"),
    }
    assert!(
        InitialDeal::new(
            BoardState::new(original.board().cascades().clone(), [None; 4], [0, 1, 0, 0]).unwrap(),
            original.metadata().clone()
        )
        .is_err()
    );
}
