// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Strict, finite, phase-specific Poche model for the named six-card scope.
//!
//! This crate is independent of the conventional Rust oracle. Its semantic
//! state contains no runtime-sized collection, policy, I/O, clock, or RNG.

mod finite;
mod formal;
mod property;
mod semantics;
mod state;

#[cfg(test)]
mod proptest_tests;

pub use finite::{
    Bid, Card, CardSet, Deal, ModelError, OneCardDeal, Player, Pot, RoundId, Score, Tricks,
    TwoCardDeal,
};
pub use formal::{FormalKernelInfo, KernelId, formal_kernel_catalog};
pub use property::{
    NativeCheckRef, PropertyClass, PropertyExpression, PropertyId, PropertySpec,
    VerificationStrategy, evaluate_state_property, evaluate_transition_property, property_catalog,
};
pub use semantics::{
    GameOutcome, ModelAction, PotDivision, RoundOutcome, RoundScoreEvent, RuleOrigin, SemanticDiff,
    Transition,
};
pub use state::{
    AwaitingDeal, BidProgress, Bidding, ChanceAction, Finished, Game, LegalActions, Observation,
    Phase, PlayedCard, PlayerAction, Playing, Scoring, StateSpaceMeasure, TrickProgress, TurnOwner,
    state_space_measure,
};
