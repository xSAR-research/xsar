//! Finite, pull-driven sequencing of application-defined actions and gestures.
//!
//! This module never performs I/O, recognises an image, validates a game rule or
//! grants input authority. The application supplies those decisions, a stable
//! authority epoch and monotonically increasing elapsed time. Every dispatched
//! gesture is removed before it is returned. Failed or uncertain delivery,
//! cancellation, stale replies and failed evidence terminate the entire run;
//! there is no retry, reset, serialisation or resume operation.
//!
//! Deadlines are cooperative. They are checked at API boundaries and cannot
//! interrupt an application operation already in progress. The application must
//! check its own monotonic STOP/context latch and freshly validate transport
//! capabilities immediately before each gesture, including gestures belonging
//! to the same logical action. Acknowledgement proves delivery, not effect.

#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

use std::{collections::VecDeque, fmt, sync::Arc, time::Duration};


/// Application-defined gestures and timing for one indivisible logical action.
///
/// All gestures must be acknowledged before settling and final effect checking.
/// Intermediate selection evidence is not required by this sequencer. The
/// application owns the meaning, coordinates and ordering of every operation.
#[derive(Debug)]
pub struct ActionPlan<Operation> {
    /// Ordered operations, consumed individually and never returned twice.
    pub operations: Vec<Operation>,
    /// Delay after the last acknowledged gesture, before effect observation.
    pub settle_delay: Duration,
    /// Delay between inconclusive observations of either evidence phase.
    pub reobserve_delay: Duration,
}


/// Finite limits for one run; zero is never an unlimited sentinel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum logical actions accepted in the supplied route.
    pub max_actions: usize,
    /// Maximum total gesture reservations across the complete run.
    pub max_gestures: usize,
    /// Maximum observation requests, including preconditions and effects.
    pub max_captures: usize,
    /// Maximum fresh precondition observations for each action.
    pub max_precondition_observations: usize,
    /// Maximum fresh effect observations for each action.
    pub max_effect_observations: usize,
    /// Cooperative duration measured from the constructor's supplied time.
    pub max_duration: Duration,
}


/// Evidence purpose, independent of the application's observation representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationPhase {
    /// Prove that the next logical action may begin.
    Precondition,
    /// Prove the complete action's effect after all its gestures.
    Effect,
}


/// Application verdict on fresh pre-action evidence; never an effect proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreconditionVerdict {
    /// The application's rules, scene and current-position checks passed.
    Ready,
    /// Wait and acquire another fresh observation, within the phase limit.
    ObserveAgain,
    /// Evidence is unsupported, contradictory or otherwise refuses the action.
    Reject,
}


/// Application verdict on fresh post-action evidence; never input authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectVerdict {
    /// The application positively verified the entire predicted action effect.
    Verified,
    /// Wait and acquire another fresh observation without replaying input.
    ObserveAgain,
    /// The observed result cannot be accepted; terminate the entire run.
    Reject,
}


/// Actual application delivery outcome, separate from a dispatch reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryReceipt {
    /// True only when the application actually attempted transport delivery.
    pub attempted: bool,
    /// True only after the complete operation, including release, was acknowledged.
    pub acknowledged: bool,
    /// A STOP observed during delivery; retain the receipt, then terminate.
    pub stop_requested: bool,
}


/// Bounded accounting; neither reservations nor acknowledgements prove effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    /// Logical actions whose whole gesture allowance has been reserved.
    pub actions_started: usize,
    /// Logical actions positively verified by the application.
    pub actions_verified: usize,
    /// Whole-action gesture allowances consumed before first dispatch.
    pub gestures_reserved: usize,
    /// Individual gestures returned to the application for one delivery attempt.
    pub gestures_dispatched: usize,
    /// Actual transport attempts reported by valid delivery receipts.
    pub gestures_attempted: usize,
    /// Complete acknowledged operations reported by valid delivery receipts.
    pub gestures_acknowledged: usize,
    /// Fresh observation requests issued, including unanswered final requests.
    pub captures_requested: usize,
}


/// A single-use response token belonging to exactly one sequencer request.
///
/// Its private identity prevents replies from another run satisfying a request.
/// It deliberately implements neither `Clone` nor serialisation. Dropping it
/// leaves the request pending; polling never reissues its operation.
#[derive(Debug)]
pub struct Ticket {
    /// Allocation identity retained while either the run or any ticket exists.
    owner: Arc<()>,
    /// Checked increasing request ordinal within the owning run.
    serial: u64,
    /// Request purpose checked again when accepting a response.
    kind: TicketKind,
}


/// One requested application operation, a verified-action notice, or run state.
#[derive(Debug)]
pub enum Request<Operation> {
    /// Acquire and classify one new application observation.
    Observe {
        /// Consume this token with the matching evidence verdict method.
        ticket: Ticket,
        /// Zero-based logical action ordinal in the original supplied route.
        action_index: usize,
        /// Determines whether to return a precondition or effect verdict.
        phase: ObservationPhase,
    },
    /// Freshly guard and deliver this operation once, with no automatic replay.
    Dispatch {
        /// Consume this token with an actual delivery receipt.
        ticket: Ticket,
        /// Zero-based logical action ordinal in the original supplied route.
        action_index: usize,
        /// Zero-based gesture ordinal within this logical action.
        gesture_index: usize,
        /// Owned application operation, removed from the plan before return.
        operation: Operation,
    },
    /// Cooperatively wait until the supplied monotonic elapsed time.
    Wait {
        /// Consume this token after the wait, or abort if STOP interrupts it.
        ticket: Ticket,
        /// Absolute value in the caller's elapsed-time coordinate system.
        until: Duration,
    },
    /// Commit the application's predicted model for this verified action once.
    ActionVerified {
        /// Zero-based original route ordinal whose complete effect was verified.
        action_index: usize,
    },
    /// All supplied actions have been verified; no further input is requested.
    Finished,
    /// A previously issued request still requires its one matching response.
    Pending,
}


/// Resource category responsible for a finite execution refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    /// Too many supplied logical actions.
    Actions,
    /// Insufficient allowance for all gestures of the next action.
    Gestures,
    /// No fresh observation allowance remains for the run.
    Captures,
    /// The current action's precondition observation allowance is exhausted.
    PreconditionObservations,
    /// The current action's effect observation allowance is exhausted.
    EffectObservations,
}


/// Configuration, authority, ordering, evidence and delivery failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionError {
    /// Every count and the duration limit must be non-zero.
    InvalidLimits,
    /// A route or one of its logical actions contains no operations.
    EmptyPlan,
    /// A configured finite resource bound was reached.
    BudgetExceeded(Budget),
    /// The application explicitly revoked this run.
    Aborted,
    /// The supplied current authority epoch differs from the original epoch.
    AuthorityChanged,
    /// Caller-supplied elapsed time decreased.
    ClockRegressed,
    /// The cooperative deadline was reached or a required wait cannot fit.
    DeadlineExceeded,
    /// The response token belongs to another request or another run.
    WrongTicket,
    /// A response method does not match the outstanding request's purpose.
    WrongPhase,
    /// Acknowledgement was claimed without an actual delivery attempt.
    InvalidReceipt,
    /// The application's immediate guard refused delivery before transport input.
    DeliveryRefused,
    /// Transport delivery was attempted but not completely acknowledged.
    DeliveryUncertain,
    /// The application rejected fresh precondition evidence.
    PreconditionRejected,
    /// The application rejected the whole action's observed effect.
    EffectRejected,
    /// The caller reported completion before the requested wait elapsed.
    WaitIncomplete,
    /// Duration arithmetic cannot represent a configured deadline or wait.
    TimeOverflow,
    /// A finite accounting or ticket ordinal increment overflowed.
    CounterOverflow,
    /// Internal sequencing invariants cannot satisfy the current request.
    InvalidState,
}


impl fmt::Display for ExecutionError {
    /// Describe a bounded refusal without including application-controlled data.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "execution stopped: {self:?}")
    }
}


impl std::error::Error for ExecutionError {}


/// Distinct response purposes retained in private ticket identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TicketKind {
    /// Fresh action preconditions.
    Precondition,
    /// Complete action effects.
    Effect,
    /// One application transport delivery.
    Delivery,
    /// One cooperative application wait.
    Wait,
}


/// Phase reached after a cooperative wait.
#[derive(Clone, Copy, Debug)]
enum AfterWait {
    /// Acquire another precondition observation.
    Precondition,
    /// Acquire another effect observation.
    Effect,
}


/// Private legal progression; only response methods leave an outstanding phase.
#[derive(Clone, Copy, Debug)]
enum State {
    /// Ready to request fresh precondition evidence.
    Precondition,
    /// Ready to request the next gesture of an already reserved action.
    Dispatch,
    /// Ready to request fresh complete-action effect evidence.
    Effect,
    /// Ready to request a cooperative wait.
    Wait { until: Duration, after: AfterWait },
    /// Exactly one request is outstanding.
    Outstanding,
    /// Emit the verified-action notice before advancing to the next action.
    Verified,
    /// All logical actions have been positively verified.
    Finished,
}


/// Private request identity plus the continuation needed after a wait.
#[derive(Clone, Copy, Debug)]
struct Outstanding {
    /// Unique ordinal within this sequencer.
    serial: u64,
    /// Exact allowed response purpose.
    kind: TicketKind,
    /// A wait's target time and subsequent observation purpose.
    wait: Option<(Duration, AfterWait)>,
}


/// Mutable action storage, separate from the application's predicted model.
#[derive(Debug)]
struct Current<Operation> {
    /// Remaining operations; dispatch removes the next operation immediately.
    operations: VecDeque<Operation>,
    /// Original whole-action operation count, reserved before the first dispatch.
    gesture_count: usize,
    /// Zero-based original route ordinal.
    action_index: usize,
    /// Number of operations already dispatched for this action.
    dispatched: usize,
    /// Frozen delay after all operations are acknowledged.
    settle_delay: Duration,
    /// Frozen delay between inconclusive observations.
    reobserve_delay: Duration,
    /// Fresh precondition requests issued for this action.
    precondition_observations: usize,
    /// Fresh effect requests issued for this action.
    effect_observations: usize,
}


impl<Operation> Current<Operation> {
    /// Take one validated plan without copying its operations.
    fn new(plan: ActionPlan<Operation>, action_index: usize) -> Self {
        Self {
            gesture_count: plan.operations.len(),
            operations: plan.operations.into(),
            action_index,
            dispatched: 0,
            settle_delay: plan.settle_delay,
            reobserve_delay: plan.reobserve_delay,
            precondition_observations: 0,
            effect_observations: 0,
        }
    }
}


/// One finite execution session with no retry, reset or resume capability.
///
/// `Epoch` is an opaque application authority identity. Equality cannot detect a
/// context change and change-back that occurs between calls: the application
/// must latch revocation immediately and call [`Self::abort`]. No generic epoch
/// can authenticate acquisition, restored state, capture freshness or focus.
#[derive(Debug)]
pub struct Sequencer<Operation, Epoch> {
    /// Stable authority identity selected before constructing this session.
    epoch: Epoch,
    /// Allocation identity distinguishing tickets from different sequencers.
    owner: Arc<()>,
    /// Remaining validated action plans.
    plans: VecDeque<ActionPlan<Operation>>,
    /// Exactly one currently active logical action.
    current: Option<Current<Operation>>,
    /// Legal next phase or outstanding-request marker.
    state: State,
    /// At most one response may satisfy this private request identity.
    outstanding: Option<Outstanding>,
    /// Checked ticket counter, never reset in this session.
    serial: u64,
    /// Frozen finite resource limits.
    limits: Limits,
    /// Monotonic elapsed-time deadline in the caller's coordinate system.
    deadline: Duration,
    /// Most recent accepted caller time.
    last_time: Duration,
    /// First terminal failure, retained even if a late delivery receipt arrives.
    failure: Option<ExecutionError>,
    /// Distinct reservation, attempted delivery and verified-effect counts.
    statistics: Statistics,
}


impl<Operation, Epoch: Eq> Sequencer<Operation, Epoch> {
    /// Construct one finite route without input or application callbacks.
    ///
    /// Times are monotonic durations supplied by the caller, in one coordinate
    /// system for this run. The first whole action is not reserved until its
    /// preconditions pass. A later action may exceed remaining gesture allowance;
    /// that refusal occurs before any gesture of that action is dispatched.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::InvalidLimits`] for zero limits,
    /// [`ExecutionError::EmptyPlan`] for an empty route/action,
    /// [`ExecutionError::BudgetExceeded`] for too many supplied actions, or
    /// [`ExecutionError::TimeOverflow`] if its deadline cannot be represented.
    pub fn new(
        epoch: Epoch,
        plans: Vec<ActionPlan<Operation>>,
        limits: Limits,
        now: Duration,
    ) -> Result<Self, ExecutionError> {
        if limits.max_actions == 0
            || limits.max_gestures == 0
            || limits.max_captures == 0
            || limits.max_precondition_observations == 0
            || limits.max_effect_observations == 0
            || limits.max_duration.is_zero()
        {
            return Err(ExecutionError::InvalidLimits);
        }
        if plans.is_empty() || plans.iter().any(|plan| plan.operations.is_empty()) {
            return Err(ExecutionError::EmptyPlan);
        }
        if plans.len() > limits.max_actions {
            return Err(ExecutionError::BudgetExceeded(Budget::Actions));
        }
        let deadline = now
            .checked_add(limits.max_duration)
            .ok_or(ExecutionError::TimeOverflow)?;
        let mut plans: VecDeque<_> = plans.into();
        let current = plans.pop_front().map(|plan| Current::new(plan, 0));
        Ok(Self {
            epoch,
            owner: Arc::new(()),
            plans,
            current,
            state: State::Precondition,
            outstanding: None,
            serial: 0,
            limits,
            deadline,
            last_time: now,
            failure: None,
            statistics: Statistics::default(),
        })
    }


    /// Return accounting without granting authority or changing execution state.
    pub const fn statistics(&self) -> &Statistics {
        &self.statistics
    }


    /// Return the first terminal failure, if this run has been revoked or failed.
    pub const fn failure(&self) -> Option<ExecutionError> {
        self.failure
    }


    /// Permanently revoke this run without input, cleanup or automatic recovery.
    ///
    /// A valid outstanding delivery receipt may still be recorded afterwards,
    /// preserving an acknowledgement received during STOP. It cannot revive the
    /// run. The application's own STOP latch remains authoritative at all times.
    pub fn abort(&mut self) {
        self.failure.get_or_insert(ExecutionError::Aborted);
    }


    /// Issue at most one new application request, consuming dispatches first.
    ///
    /// Repeated calls while a request is outstanding return [`Request::Pending`].
    /// A verified notice is issued once; the next call begins the next action or
    /// returns [`Request::Finished`]. Finished runs never request further input.
    ///
    /// # Errors
    ///
    /// Returns and retains the first authority, clock, deadline, budget, counter
    /// or sequencing failure. Once failed, this instance cannot be restarted.
    pub fn next(
        &mut self,
        epoch: &Epoch,
        now: Duration,
    ) -> Result<Request<Operation>, ExecutionError> {
        if matches!(self.state, State::Finished) && self.failure.is_none() {
            return Ok(Request::Finished);
        }
        self.active(epoch, now)?;
        match self.state {
            State::Precondition => self.observe(ObservationPhase::Precondition),
            State::Effect => self.observe(ObservationPhase::Effect),
            State::Dispatch => self.dispatch(),
            State::Wait { until, after } => {
                let ticket = self.issue(TicketKind::Wait, Some((until, after)))?;
                Ok(Request::Wait { ticket, until })
            }
            State::Outstanding => Ok(Request::Pending),
            State::Verified => {
                let action_index = self
                    .current
                    .as_ref()
                    .map(|current| current.action_index)
                    .ok_or_else(|| self.stop(ExecutionError::InvalidState))?;
                self.current = match self.plans.pop_front() {
                    Some(plan) => {
                        let next_index = action_index
                            .checked_add(1)
                            .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
                        self.state = State::Precondition;
                        Some(Current::new(plan, next_index))
                    }
                    None => {
                        self.state = State::Finished;
                        None
                    }
                };
                Ok(Request::ActionVerified { action_index })
            }
            State::Finished => Ok(Request::Finished),
        }
    }


    /// Accept fresh preconditions, reserving the whole action before any gesture.
    ///
    /// # Errors
    ///
    /// Returns a terminal error for a stale/wrong ticket, revoked authority,
    /// invalid time, rejected evidence, exhausted allowance or arithmetic error.
    pub fn precondition(
        &mut self,
        ticket: Ticket,
        epoch: &Epoch,
        now: Duration,
        verdict: PreconditionVerdict,
    ) -> Result<(), ExecutionError> {
        self.active(epoch, now)?;
        self.accept(ticket, TicketKind::Precondition)?;
        match verdict {
            PreconditionVerdict::Reject => Err(self.stop(ExecutionError::PreconditionRejected)),
            PreconditionVerdict::ObserveAgain => self.reobserve(AfterWait::Precondition, now),
            PreconditionVerdict::Ready => {
                let count = self
                    .current
                    .as_ref()
                    .map(|current| current.gesture_count)
                    .ok_or_else(|| self.stop(ExecutionError::InvalidState))?;
                let reserved = self
                    .statistics
                    .gestures_reserved
                    .checked_add(count)
                    .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
                if reserved > self.limits.max_gestures {
                    return Err(self.stop(ExecutionError::BudgetExceeded(Budget::Gestures)));
                }
                self.statistics.gestures_reserved = reserved;
                self.statistics.actions_started = self
                    .statistics
                    .actions_started
                    .checked_add(1)
                    .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
                self.state = State::Dispatch;
                Ok(())
            }
        }
    }


    /// Record one actual delivery receipt, retaining a late ACK before revocation.
    ///
    /// A refusal or unacknowledged attempt terminates the whole run. A previously
    /// aborted run accepts its valid outstanding receipt for accounting only.
    /// The next gesture is requested only after acknowledged delivery and fresh
    /// authority/time checks; the application must guard it again before I/O.
    ///
    /// # Errors
    ///
    /// Returns the first terminal failure for stale/wrong tickets, impossible
    /// receipts, guard refusal, uncertainty, STOP, changed epoch, invalid time,
    /// insufficient waiting time or exhausted arithmetic representation.
    pub fn delivered(
        &mut self,
        ticket: Ticket,
        epoch: &Epoch,
        now: Duration,
        receipt: DeliveryReceipt,
    ) -> Result<(), ExecutionError> {
        self.accept(ticket, TicketKind::Delivery)?;
        if receipt.acknowledged && !receipt.attempted {
            return Err(self.stop(ExecutionError::InvalidReceipt));
        }
        if receipt.attempted {
            self.statistics.gestures_attempted = self
                .statistics
                .gestures_attempted
                .checked_add(1)
                .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
        }
        if receipt.acknowledged {
            self.statistics.gestures_acknowledged = self
                .statistics
                .gestures_acknowledged
                .checked_add(1)
                .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
        }
        if !receipt.attempted {
            return Err(self.stop(ExecutionError::DeliveryRefused));
        }
        if !receipt.acknowledged {
            return Err(self.stop(ExecutionError::DeliveryUncertain));
        }
        if receipt.stop_requested {
            self.abort();
        }
        self.active(epoch, now)?;
        let Some(current) = self.current.as_ref() else {
            return Err(self.stop(ExecutionError::InvalidState));
        };
        if current.operations.is_empty() {
            let delay = current.settle_delay;
            self.wait(delay, AfterWait::Effect, now)
        } else {
            self.state = State::Dispatch;
            Ok(())
        }
    }


    /// Complete the outstanding cooperative wait after its target elapsed time.
    ///
    /// # Errors
    ///
    /// Returns a terminal error for an early completion, wrong ticket/purpose,
    /// authority loss, invalid time or an already failed run.
    pub fn waited(
        &mut self,
        ticket: Ticket,
        epoch: &Epoch,
        now: Duration,
    ) -> Result<(), ExecutionError> {
        self.active(epoch, now)?;
        let outstanding = self.accept(ticket, TicketKind::Wait)?;
        let (until, after) = outstanding
            .wait
            .ok_or_else(|| self.stop(ExecutionError::InvalidState))?;
        if now < until {
            return Err(self.stop(ExecutionError::WaitIncomplete));
        }
        self.state = match after {
            AfterWait::Precondition => State::Precondition,
            AfterWait::Effect => State::Effect,
        };
        Ok(())
    }


    /// Accept whole-action effect evidence without inferring any game semantics.
    ///
    /// [`EffectVerdict::Verified`] enables one [`Request::ActionVerified`] notice;
    /// only that notice should cause the application to commit its predicted
    /// model. Inconclusive evidence requests fresh observations, never input.
    ///
    /// # Errors
    ///
    /// Returns a terminal error for rejected evidence, stale/wrong tickets,
    /// exhausted observations, revoked authority, invalid time or overflow.
    pub fn effect(
        &mut self,
        ticket: Ticket,
        epoch: &Epoch,
        now: Duration,
        verdict: EffectVerdict,
    ) -> Result<(), ExecutionError> {
        self.active(epoch, now)?;
        self.accept(ticket, TicketKind::Effect)?;
        match verdict {
            EffectVerdict::Reject => Err(self.stop(ExecutionError::EffectRejected)),
            EffectVerdict::ObserveAgain => self.reobserve(AfterWait::Effect, now),
            EffectVerdict::Verified => {
                self.statistics.actions_verified = self
                    .statistics
                    .actions_verified
                    .checked_add(1)
                    .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
                self.state = State::Verified;
                Ok(())
            }
        }
    }


    /// Preserve the first failure while permanently revoking all further input.
    fn stop(&mut self, error: ExecutionError) -> ExecutionError {
        *self.failure.get_or_insert(error)
    }


    /// Check monotonic authority/time at every ordinary public operation boundary.
    fn active(&mut self, epoch: &Epoch, now: Duration) -> Result<(), ExecutionError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if epoch != &self.epoch {
            return Err(self.stop(ExecutionError::AuthorityChanged));
        }
        if now < self.last_time {
            return Err(self.stop(ExecutionError::ClockRegressed));
        }
        self.last_time = now;
        if now >= self.deadline {
            return Err(self.stop(ExecutionError::DeadlineExceeded));
        }
        Ok(())
    }


    /// Issue a unique single-use ticket and mark its request outstanding first.
    fn issue(
        &mut self,
        kind: TicketKind,
        wait: Option<(Duration, AfterWait)>,
    ) -> Result<Ticket, ExecutionError> {
        if self.outstanding.is_some() {
            return Err(self.stop(ExecutionError::InvalidState));
        }
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
        self.outstanding = Some(Outstanding {
            serial: self.serial,
            kind,
            wait,
        });
        self.state = State::Outstanding;
        Ok(Ticket {
            owner: Arc::clone(&self.owner),
            serial: self.serial,
            kind,
        })
    }


    /// Consume only the matching response; retain outstanding data on refusal.
    fn accept(&mut self, ticket: Ticket, kind: TicketKind) -> Result<Outstanding, ExecutionError> {
        let Some(outstanding) = self.outstanding else {
            return Err(self.stop(ExecutionError::WrongTicket));
        };
        if !Arc::ptr_eq(&ticket.owner, &self.owner)
            || ticket.serial != outstanding.serial
            || ticket.kind != outstanding.kind
        {
            return Err(self.stop(ExecutionError::WrongTicket));
        }
        if kind != outstanding.kind {
            return Err(self.stop(ExecutionError::WrongPhase));
        }
        self.outstanding = None;
        Ok(outstanding)
    }


    /// Reserve one bounded fresh observation before returning its opaque ticket.
    fn observe(&mut self, phase: ObservationPhase) -> Result<Request<Operation>, ExecutionError> {
        if self.statistics.captures_requested >= self.limits.max_captures {
            return Err(self.stop(ExecutionError::BudgetExceeded(Budget::Captures)));
        }
        let Some(current) = self.current.as_mut() else {
            return Err(self.stop(ExecutionError::InvalidState));
        };
        let (count, maximum, budget, kind) = match phase {
            ObservationPhase::Precondition => (
                &mut current.precondition_observations,
                self.limits.max_precondition_observations,
                Budget::PreconditionObservations,
                TicketKind::Precondition,
            ),
            ObservationPhase::Effect => (
                &mut current.effect_observations,
                self.limits.max_effect_observations,
                Budget::EffectObservations,
                TicketKind::Effect,
            ),
        };
        if *count >= maximum {
            return Err(self.stop(ExecutionError::BudgetExceeded(budget)));
        }
        let Some(next_count) = count.checked_add(1) else {
            return Err(self.stop(ExecutionError::CounterOverflow));
        };
        *count = next_count;
        let action_index = current.action_index;
        self.statistics.captures_requested = self
            .statistics
            .captures_requested
            .checked_add(1)
            .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
        let ticket = self.issue(kind, None)?;
        Ok(Request::Observe {
            ticket,
            action_index,
            phase,
        })
    }


    /// Remove and account for one operation before exposing it to application I/O.
    fn dispatch(&mut self) -> Result<Request<Operation>, ExecutionError> {
        let Some(current) = self.current.as_mut() else {
            return Err(self.stop(ExecutionError::InvalidState));
        };
        let action_index = current.action_index;
        let gesture_index = current.dispatched;
        let Some(operation) = current.operations.pop_front() else {
            return Err(self.stop(ExecutionError::InvalidState));
        };
        let Some(dispatched) = current.dispatched.checked_add(1) else {
            return Err(self.stop(ExecutionError::CounterOverflow));
        };
        current.dispatched = dispatched;
        self.statistics.gestures_dispatched = self
            .statistics
            .gestures_dispatched
            .checked_add(1)
            .ok_or_else(|| self.stop(ExecutionError::CounterOverflow))?;
        let ticket = self.issue(TicketKind::Delivery, None)?;
        Ok(Request::Dispatch {
            ticket,
            action_index,
            gesture_index,
            operation,
        })
    }


    /// Reject exhausted re-observation phases before scheduling another wait.
    fn reobserve(&mut self, after: AfterWait, now: Duration) -> Result<(), ExecutionError> {
        let Some(current) = self.current.as_ref() else {
            return Err(self.stop(ExecutionError::InvalidState));
        };
        let (count, maximum, budget) = match after {
            AfterWait::Precondition => (
                current.precondition_observations,
                self.limits.max_precondition_observations,
                Budget::PreconditionObservations,
            ),
            AfterWait::Effect => (
                current.effect_observations,
                self.limits.max_effect_observations,
                Budget::EffectObservations,
            ),
        };
        if count >= maximum {
            return Err(self.stop(ExecutionError::BudgetExceeded(budget)));
        }
        let delay = current.reobserve_delay;
        self.wait(delay, after, now)
    }


    /// Schedule a finite wait that fits strictly before the cooperative deadline.
    fn wait(
        &mut self,
        delay: Duration,
        after: AfterWait,
        now: Duration,
    ) -> Result<(), ExecutionError> {
        let until = now
            .checked_add(delay)
            .ok_or_else(|| self.stop(ExecutionError::TimeOverflow))?;
        if until >= self.deadline {
            return Err(self.stop(ExecutionError::DeadlineExceeded));
        }
        self.state = State::Wait { until, after };
        Ok(())
    }
}
