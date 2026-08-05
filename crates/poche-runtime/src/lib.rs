// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! I/O composition boundary around pure Poche game and session semantics.
//!
//! Direct loopback, Veilid, CLI, web, and RL adapters share these ports. The RL
//! path can use the game environment directly and never needs text or network
//! framing.

use core::marker::PhantomData;

use poche_environment::GameEnvironment;
use poche_protocol::{CountdownToken, ProtocolFrame, RoomId};
use poche_session::PureSessionMachine;

/// External logical-time scheduling port.
pub trait ClockPort {
    /// Adapter-specific error.
    type Error;

    /// Arrange delivery of one logical expiry token at an adapter deadline.
    ///
    /// # Errors
    ///
    /// Returns an adapter failure without mutating semantic session state.
    fn arm_countdown(
        &mut self,
        room_id: &RoomId,
        deadline_tick: u64,
        token: &CountdownToken,
    ) -> Result<(), Self::Error>;

    /// Cancel future delivery of a token when possible. A late delivery remains
    /// safe because the pure reducer rejects stale/inactive tokens.
    ///
    /// # Errors
    ///
    /// Returns an adapter failure without changing the reducer's truth.
    fn cancel_countdown(
        &mut self,
        room_id: &RoomId,
        token: &CountdownToken,
    ) -> Result<(), Self::Error>;
}

/// Transport-facing delivery port for already viewer-scoped protocol frames.
pub trait TransportPort {
    /// Adapter-specific error.
    type Error;

    /// Deliver one complete typed frame to an exact connection/recipient.
    ///
    /// # Errors
    ///
    /// Returns a transport failure, never a semantic allow/deny result.
    fn send(&mut self, frame: &ProtocolFrame) -> Result<(), Self::Error>;
}

/// Explicit composition of pure session state with one game type and I/O ports.
///
/// The environment type is carried at the type level because
/// [`GameEnvironment`] operations are pure associated functions.
pub struct RuntimeComposition<E, S, C, T>
where
    E: GameEnvironment,
    S: PureSessionMachine,
{
    /// Authoritative pure session state.
    pub session: S::State,
    /// Logical-time adapter.
    pub clock: C,
    /// Transport adapter receiving viewer-scoped frames only.
    pub transport: T,
    environment: PhantomData<fn() -> E>,
}

impl<E, S, C, T> RuntimeComposition<E, S, C, T>
where
    E: GameEnvironment,
    S: PureSessionMachine,
{
    /// Compose pure state with concrete clock and transport adapters.
    #[must_use]
    pub const fn new(session: S::State, clock: C, transport: T) -> Self {
        Self {
            session,
            clock,
            transport,
            environment: PhantomData,
        }
    }

    /// Decompose the runtime without hidden background work.
    #[must_use]
    pub fn into_parts(self) -> (S::State, C, T) {
        (self.session, self.clock, self.transport)
    }
}
