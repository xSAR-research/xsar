//! Non-game consumers exercise sequencing, finite bounds and terminal failures.
//!
//! These tests provide application verdicts and receipts directly. They establish
//! no transport delivery, image recognition, calibration or live acceptance.

#![cfg(feature = "execution")]

use std::time::Duration;

use xsar::execution::{
    ActionPlan, Budget, DeliveryReceipt, EffectVerdict, ExecutionError, Limits, ObservationPhase,
    PreconditionVerdict, Request, Sequencer, Ticket,
};


/// Non-card operations deliberately do not require `Clone` or `Copy`.
#[derive(Debug, PartialEq, Eq)]
enum DeviceOperation {
    /// Set one arbitrary instrument level.
    Level(u8),
    /// Emit one application-specific pulse.
    Pulse(char),
}


/// Independent generic consumer with a simple opaque application authority epoch.
type DeviceSequence = Sequencer<DeviceOperation, u64>;


/// Small finite configuration leaves room for targeted reductions in each test.
fn limits() -> Limits {
    Limits {
        max_actions: 4,
        max_gestures: 8,
        max_captures: 16,
        max_precondition_observations: 3,
        max_effect_observations: 3,
        max_duration: Duration::from_secs(60),
    }
}


/// Freeze independently supplied delays with one or more ordered operations.
fn plan(operations: Vec<DeviceOperation>) -> ActionPlan<DeviceOperation> {
    ActionPlan {
        operations,
        settle_delay: Duration::from_millis(10),
        reobserve_delay: Duration::from_millis(5),
    }
}


/// Construct a new run using the same epoch without sharing ticket ownership.
fn sequence(plans: Vec<ActionPlan<DeviceOperation>>, bounds: Limits) -> DeviceSequence {
    Sequencer::new(7, plans, bounds, Duration::ZERO).expect("valid finite sequence")
}


/// Return the next actual observation ticket and verify its declared purpose.
fn observation(sequence: &mut DeviceSequence, now: Duration, expected: ObservationPhase) -> Ticket {
    let Request::Observe { ticket, phase, .. } =
        sequence.next(&7, now).expect("observation request")
    else {
        panic!("expected fresh observation");
    };
    assert_eq!(phase, expected);
    ticket
}


/// Approve one action's preconditions; this cannot verify its later effect.
fn ready(sequence: &mut DeviceSequence, now: Duration) {
    let ticket = observation(sequence, now, ObservationPhase::Precondition);
    sequence
        .precondition(ticket, &7, now, PreconditionVerdict::Ready)
        .expect("ready action");
}


/// Obtain the next one-shot gesture request without sending any real input.
fn dispatch(sequence: &mut DeviceSequence, now: Duration) -> (Ticket, DeviceOperation) {
    let Request::Dispatch {
        ticket, operation, ..
    } = sequence.next(&7, now).expect("dispatch request")
    else {
        panic!("expected one dispatch");
    };
    (ticket, operation)
}


/// Report one acknowledged operation, explicitly distinct from effect proof.
fn acknowledged(sequence: &mut DeviceSequence, ticket: Ticket, now: Duration) {
    sequence
        .delivered(
            ticket,
            &7,
            now,
            DeliveryReceipt {
                attempted: true,
                acknowledged: true,
                stop_requested: false,
            },
        )
        .expect("acknowledged delivery");
}


/// Complete one requested wait by advancing the fake monotonic elapsed time.
fn settle(sequence: &mut DeviceSequence, now: &mut Duration) {
    let Request::Wait { ticket, until } = sequence.next(&7, *now).expect("wait request") else {
        panic!("expected cooperative wait");
    };
    assert!(until >= *now);
    *now = until;
    sequence.waited(ticket, &7, *now).expect("wait completed");
}


/// Verify the complete effect and consume its one model-commit notification.
fn verify(sequence: &mut DeviceSequence, now: Duration, expected_index: usize) {
    let ticket = observation(sequence, now, ObservationPhase::Effect);
    sequence
        .effect(ticket, &7, now, EffectVerdict::Verified)
        .expect("verified complete effect");
    let Request::ActionVerified { action_index } = sequence.next(&7, now).expect("verified notice")
    else {
        panic!("expected action notice");
    };
    assert_eq!(action_index, expected_index);
}


/// Two non-card actions use three gestures and commit only complete effects.
#[test]
fn non_card_consumer_runs_two_actions_and_three_ordered_gestures() {
    let mut sequence = sequence(
        vec![
            plan(vec![DeviceOperation::Level(3)]),
            plan(vec![
                DeviceOperation::Pulse('a'),
                DeviceOperation::Pulse('b'),
            ]),
        ],
        limits(),
    );
    let mut now = Duration::ZERO;
    let mut observed_operations = Vec::new();
    let mut verified_actions = Vec::new();
    loop {
        match sequence.next(&7, now).expect("legal application loop") {
            Request::Observe {
                ticket,
                phase: ObservationPhase::Precondition,
                ..
            } => {
                sequence
                    .precondition(ticket, &7, now, PreconditionVerdict::Ready)
                    .unwrap();
            }
            Request::Observe {
                ticket,
                phase: ObservationPhase::Effect,
                ..
            } => {
                assert_eq!(
                    sequence.statistics().actions_verified,
                    verified_actions.len()
                );
                sequence
                    .effect(ticket, &7, now, EffectVerdict::Verified)
                    .unwrap();
            }
            Request::Dispatch {
                ticket,
                operation,
                action_index,
                gesture_index,
            } => {
                assert_eq!(
                    sequence.statistics().actions_verified,
                    verified_actions.len()
                );
                assert!(matches!(sequence.next(&7, now).unwrap(), Request::Pending));
                observed_operations.push((action_index, gesture_index, operation));
                acknowledged(&mut sequence, ticket, now);
            }
            Request::Wait { ticket, until } => {
                now = until;
                sequence.waited(ticket, &7, now).unwrap();
            }
            Request::ActionVerified { action_index } => verified_actions.push(action_index),
            Request::Finished => break,
            Request::Pending => panic!("application must answer each outstanding request"),
        }
    }
    assert_eq!(
        observed_operations,
        vec![
            (0, 0, DeviceOperation::Level(3)),
            (1, 0, DeviceOperation::Pulse('a')),
            (1, 1, DeviceOperation::Pulse('b')),
        ]
    );
    assert_eq!(verified_actions, vec![0, 1]);
    assert_eq!(sequence.statistics().gestures_reserved, 3);
    assert_eq!(sequence.statistics().gestures_dispatched, 3);
    assert_eq!(sequence.statistics().gestures_attempted, 3);
    assert_eq!(sequence.statistics().gestures_acknowledged, 3);
    assert_eq!(sequence.statistics().captures_requested, 4);
    assert!(matches!(sequence.next(&7, now).unwrap(), Request::Finished));
}


/// Polling or losing a dispatch token cannot produce the same operation again.
#[test]
fn outstanding_dispatch_is_never_reissued() {
    let mut sequence = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    ready(&mut sequence, Duration::ZERO);
    let (ticket, operation) = dispatch(&mut sequence, Duration::ZERO);
    assert_eq!(operation, DeviceOperation::Level(1));
    for _ in 0..4 {
        assert!(matches!(
            sequence.next(&7, Duration::ZERO).unwrap(),
            Request::Pending
        ));
    }
    drop(ticket);
    assert!(matches!(
        sequence.next(&7, Duration::ZERO).unwrap(),
        Request::Pending
    ));
    assert_eq!(sequence.statistics().gestures_dispatched, 1);
    assert_eq!(sequence.statistics().gestures_attempted, 0);
    sequence.abort();
    assert_eq!(
        sequence.next(&7, Duration::ZERO).unwrap_err(),
        ExecutionError::Aborted
    );
}


/// STOP between gestures retains the first ACK and suppresses the second gesture.
#[test]
fn stop_after_first_gesture_never_resumes_the_partial_action() {
    for abort_before_receipt in [false, true] {
        let mut sequence = sequence(
            vec![plan(vec![
                DeviceOperation::Pulse('a'),
                DeviceOperation::Pulse('b'),
            ])],
            limits(),
        );
        ready(&mut sequence, Duration::ZERO);
        let (ticket, _) = dispatch(&mut sequence, Duration::ZERO);
        if abort_before_receipt {
            sequence.abort();
        }
        assert_eq!(
            sequence.delivered(
                ticket,
                &7,
                Duration::ZERO,
                DeliveryReceipt {
                    attempted: true,
                    acknowledged: true,
                    stop_requested: !abort_before_receipt,
                }
            ),
            Err(ExecutionError::Aborted)
        );
        assert_eq!(sequence.statistics().gestures_reserved, 2);
        assert_eq!(sequence.statistics().gestures_dispatched, 1);
        assert_eq!(sequence.statistics().gestures_attempted, 1);
        assert_eq!(sequence.statistics().gestures_acknowledged, 1);
        assert_eq!(sequence.statistics().actions_verified, 0);
        assert_eq!(
            sequence.next(&7, Duration::ZERO).unwrap_err(),
            ExecutionError::Aborted
        );
    }
}


/// Even same-epoch, same-ordinal requests from different runs are not interchangeable.
#[test]
fn cross_run_ticket_and_wrong_response_phase_terminate_execution() {
    let mut first = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    let mut second = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    let first_ticket = observation(&mut first, Duration::ZERO, ObservationPhase::Precondition);
    let second_ticket = observation(&mut second, Duration::ZERO, ObservationPhase::Precondition);
    assert_eq!(
        first.precondition(
            second_ticket,
            &7,
            Duration::ZERO,
            PreconditionVerdict::Ready
        ),
        Err(ExecutionError::WrongTicket)
    );
    assert_eq!(
        first.next(&7, Duration::ZERO).unwrap_err(),
        ExecutionError::WrongTicket
    );
    drop(first_ticket);
    let mut third = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    let ticket = observation(&mut third, Duration::ZERO, ObservationPhase::Precondition);
    assert_eq!(
        third.effect(ticket, &7, Duration::ZERO, EffectVerdict::Verified),
        Err(ExecutionError::WrongPhase)
    );
    assert_eq!(third.statistics().actions_verified, 0);
}


/// Refused, uncertain and impossible delivery receipts never authorise another input.
#[test]
fn every_failed_delivery_is_terminal_without_replay() {
    for (attempted, acknowledged, expected) in [
        (false, false, ExecutionError::DeliveryRefused),
        (true, false, ExecutionError::DeliveryUncertain),
        (false, true, ExecutionError::InvalidReceipt),
    ] {
        let mut sequence = sequence(
            vec![plan(vec![
                DeviceOperation::Pulse('a'),
                DeviceOperation::Pulse('b'),
            ])],
            limits(),
        );
        ready(&mut sequence, Duration::ZERO);
        let (ticket, _) = dispatch(&mut sequence, Duration::ZERO);
        assert_eq!(
            sequence.delivered(
                ticket,
                &7,
                Duration::ZERO,
                DeliveryReceipt {
                    attempted,
                    acknowledged,
                    stop_requested: false,
                }
            ),
            Err(expected)
        );
        assert_eq!(sequence.next(&7, Duration::ZERO).unwrap_err(), expected);
        assert_eq!(sequence.statistics().gestures_dispatched, 1);
        assert_eq!(sequence.statistics().actions_verified, 0);
    }
}


/// The complete allowance is reserved before a multi-gesture action can begin.
#[test]
fn remaining_gesture_budget_cannot_split_an_action() {
    let mut bounds = limits();
    bounds.max_gestures = 2;
    let mut sequence = sequence(
        vec![
            plan(vec![DeviceOperation::Level(1)]),
            plan(vec![
                DeviceOperation::Pulse('a'),
                DeviceOperation::Pulse('b'),
            ]),
        ],
        bounds,
    );
    let mut now = Duration::ZERO;
    ready(&mut sequence, now);
    let (ticket, _) = dispatch(&mut sequence, now);
    acknowledged(&mut sequence, ticket, now);
    settle(&mut sequence, &mut now);
    verify(&mut sequence, now, 0);
    let ticket = observation(&mut sequence, now, ObservationPhase::Precondition);
    assert_eq!(
        sequence.precondition(ticket, &7, now, PreconditionVerdict::Ready),
        Err(ExecutionError::BudgetExceeded(Budget::Gestures))
    );
    assert_eq!(sequence.statistics().gestures_reserved, 1);
    assert_eq!(sequence.statistics().gestures_dispatched, 1);
    assert_eq!(sequence.statistics().actions_started, 1);
}


/// Fresh observation budgets are finite for each phase and the complete run.
#[test]
fn precondition_effect_and_global_capture_limits_stop_without_input_replay() {
    let mut bounds = limits();
    bounds.max_precondition_observations = 1;
    let mut precondition = sequence(vec![plan(vec![DeviceOperation::Level(1)])], bounds);
    let ticket = observation(
        &mut precondition,
        Duration::ZERO,
        ObservationPhase::Precondition,
    );
    assert_eq!(
        precondition.precondition(
            ticket,
            &7,
            Duration::ZERO,
            PreconditionVerdict::ObserveAgain
        ),
        Err(ExecutionError::BudgetExceeded(
            Budget::PreconditionObservations
        ))
    );
    assert_eq!(precondition.statistics().gestures_dispatched, 0);
    for capture_limit in [false, true] {
        let mut bounds = limits();
        bounds.max_effect_observations = 1;
        if capture_limit {
            bounds.max_captures = 1;
        }
        let mut effect = sequence(vec![plan(vec![DeviceOperation::Level(1)])], bounds);
        let mut now = Duration::ZERO;
        ready(&mut effect, now);
        let (ticket, _) = dispatch(&mut effect, now);
        acknowledged(&mut effect, ticket, now);
        settle(&mut effect, &mut now);
        if capture_limit {
            assert_eq!(
                effect.next(&7, now).unwrap_err(),
                ExecutionError::BudgetExceeded(Budget::Captures)
            );
        } else {
            let ticket = observation(&mut effect, now, ObservationPhase::Effect);
            assert_eq!(
                effect.effect(ticket, &7, now, EffectVerdict::ObserveAgain),
                Err(ExecutionError::BudgetExceeded(Budget::EffectObservations))
            );
        }
        assert_eq!(effect.statistics().gestures_dispatched, 1);
        assert_eq!(effect.statistics().actions_verified, 0);
    }
}


/// Inconclusive evidence causes only bounded waits and new capture requests.
#[test]
fn reobservation_preserves_input_counts_and_uses_frozen_delays() {
    let mut sequence = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    let mut now = Duration::ZERO;
    let ticket = observation(&mut sequence, now, ObservationPhase::Precondition);
    sequence
        .precondition(ticket, &7, now, PreconditionVerdict::ObserveAgain)
        .unwrap();
    settle(&mut sequence, &mut now);
    assert_eq!(now, Duration::from_millis(5));
    ready(&mut sequence, now);
    let (ticket, _) = dispatch(&mut sequence, now);
    acknowledged(&mut sequence, ticket, now);
    settle(&mut sequence, &mut now);
    let ticket = observation(&mut sequence, now, ObservationPhase::Effect);
    sequence
        .effect(ticket, &7, now, EffectVerdict::ObserveAgain)
        .unwrap();
    settle(&mut sequence, &mut now);
    verify(&mut sequence, now, 0);
    assert_eq!(sequence.statistics().gestures_dispatched, 1);
    assert_eq!(sequence.statistics().captures_requested, 4);
}


/// Authority and clocks fail closed; a late acknowledged input is still counted.
#[test]
fn changed_epoch_regressing_clock_and_deadline_are_terminal() {
    let mut changed = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    assert_eq!(
        changed.next(&8, Duration::ZERO).unwrap_err(),
        ExecutionError::AuthorityChanged
    );
    assert_eq!(
        changed.next(&7, Duration::ZERO).unwrap_err(),
        ExecutionError::AuthorityChanged
    );
    let mut regressed = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    let ticket = observation(
        &mut regressed,
        Duration::from_secs(2),
        ObservationPhase::Precondition,
    );
    assert_eq!(
        regressed.precondition(
            ticket,
            &7,
            Duration::from_secs(1),
            PreconditionVerdict::Ready
        ),
        Err(ExecutionError::ClockRegressed)
    );
    let mut late = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    ready(&mut late, Duration::ZERO);
    let (ticket, _) = dispatch(&mut late, Duration::ZERO);
    assert_eq!(
        late.delivered(
            ticket,
            &7,
            Duration::from_secs(60),
            DeliveryReceipt {
                attempted: true,
                acknowledged: true,
                stop_requested: false,
            }
        ),
        Err(ExecutionError::DeadlineExceeded)
    );
    assert_eq!(late.statistics().gestures_acknowledged, 1);
    assert_eq!(late.statistics().actions_verified, 0);
}


/// Early wait completion and explicitly rejected evidence are terminal refusals.
#[test]
fn early_wait_and_rejected_evidence_never_commit_an_action() {
    let mut early = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    ready(&mut early, Duration::ZERO);
    let (ticket, _) = dispatch(&mut early, Duration::ZERO);
    acknowledged(&mut early, ticket, Duration::ZERO);
    let Request::Wait { ticket, .. } = early.next(&7, Duration::ZERO).unwrap() else {
        panic!("expected settle request");
    };
    assert_eq!(
        early.waited(ticket, &7, Duration::ZERO),
        Err(ExecutionError::WaitIncomplete)
    );
    let mut rejected = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    let ticket = observation(
        &mut rejected,
        Duration::ZERO,
        ObservationPhase::Precondition,
    );
    assert_eq!(
        rejected.precondition(ticket, &7, Duration::ZERO, PreconditionVerdict::Reject),
        Err(ExecutionError::PreconditionRejected)
    );
    assert_eq!(rejected.statistics().actions_verified, 0);
    let mut effect = sequence(vec![plan(vec![DeviceOperation::Level(1)])], limits());
    let mut now = Duration::ZERO;
    ready(&mut effect, now);
    let (ticket, _) = dispatch(&mut effect, now);
    acknowledged(&mut effect, ticket, now);
    settle(&mut effect, &mut now);
    let ticket = observation(&mut effect, now, ObservationPhase::Effect);
    assert_eq!(
        effect.effect(ticket, &7, now, EffectVerdict::Reject),
        Err(ExecutionError::EffectRejected)
    );
    assert_eq!(effect.statistics().actions_verified, 0);
}


/// Invalid plans, action count limits and unrepresentable deadlines fail before I/O.
#[test]
fn constructor_validates_finite_configuration_and_nonempty_actions() {
    let mut invalid = limits();
    invalid.max_gestures = 0;
    assert!(matches!(
        Sequencer::new(
            7,
            vec![plan(vec![DeviceOperation::Level(1)])],
            invalid,
            Duration::ZERO
        ),
        Err(ExecutionError::InvalidLimits)
    ));
    assert!(matches!(
        Sequencer::<DeviceOperation, _>::new(7, Vec::new(), limits(), Duration::ZERO),
        Err(ExecutionError::EmptyPlan)
    ));
    assert!(matches!(
        Sequencer::new(
            7,
            vec![plan(Vec::<DeviceOperation>::new())],
            limits(),
            Duration::ZERO
        ),
        Err(ExecutionError::EmptyPlan)
    ));
    let mut actions = limits();
    actions.max_actions = 1;
    assert!(matches!(
        Sequencer::new(
            7,
            vec![
                plan(vec![DeviceOperation::Level(1)]),
                plan(vec![DeviceOperation::Level(2)]),
            ],
            actions,
            Duration::ZERO
        ),
        Err(ExecutionError::BudgetExceeded(Budget::Actions))
    ));
    assert!(matches!(
        Sequencer::new(
            7,
            vec![plan(vec![DeviceOperation::Level(1)])],
            limits(),
            Duration::MAX
        ),
        Err(ExecutionError::TimeOverflow)
    ));
}
