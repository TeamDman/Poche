use poche_protocol::{
    HandProjection, MemberProjection, PrincipalId, ProjectionPayload, RoomId, RoomPhase,
};

use crate::{ConnectionState, SessionGame, SessionPhase, SessionState};

/// Pure viewer-projection failure. It contains no hidden state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionError<E> {
    UnknownViewer,
    NotConnected,
    StaleProjectionEpoch,
    Game(E),
}

/// Separately named, local-only capability for trusted host diagnostics.
///
/// It deliberately has no serialization or reflection implementation and
/// cannot be accepted by [`project_viewer`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalHostDiagnosticCapability {
    room_id: RoomId,
    session_epoch: u64,
    host: PrincipalId,
}

/// Local diagnostic result. This is not a protocol projection payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalHostDiagnosticProjection {
    pub hands: Vec<HandProjection>,
}

/// Mint the explicit local diagnostics capability for the connected host.
#[must_use]
pub fn local_host_diagnostic_capability<G>(
    state: &SessionState<G>,
    host: &PrincipalId,
) -> Option<LocalHostDiagnosticCapability> {
    state.member(host).and_then(|member| {
        (member.host && member.connection == ConnectionState::Connected).then(|| {
            LocalHostDiagnosticCapability {
                room_id: state.room_id.clone(),
                session_epoch: state.session_epoch,
                host: host.clone(),
            }
        })
    })
}

/// Project the only network-safe state shape for one exact recipient.
///
/// Hidden cards are added only for the recipient's seat and exact active
/// spectator grants. Ordinary host, player, and spectator views all pass
/// through this same function.
///
/// # Errors
///
/// Rejects unknown/disconnected viewers, stale projection epochs, or a game
/// conversion failure.
pub fn project_viewer<G: SessionGame>(
    state: &SessionState<G>,
    recipient: &PrincipalId,
    expected_projection_epoch: u64,
) -> Result<ProjectionPayload, ProjectionError<G::Error>> {
    let member = state
        .member(recipient)
        .ok_or(ProjectionError::UnknownViewer)?;
    if member.connection != ConnectionState::Connected {
        return Err(ProjectionError::NotConnected);
    }
    if expected_projection_epoch != state.projection_epoch {
        return Err(ProjectionError::StaleProjectionEpoch);
    }

    let game = active_game(&state.phase);
    let public_game_state = game
        .map(SessionGame::public_projection)
        .transpose()
        .map_err(ProjectionError::Game)?;
    let own_hand = match (game, member.seat) {
        (Some(game), Some(seat)) => Some(HandProjection {
            player: recipient.clone(),
            grant_epoch: member.membership_epoch,
            cards: game.private_hand(seat).map_err(ProjectionError::Game)?,
        }),
        _ => None,
    };

    let mut granted_hands = Vec::new();
    if let Some(game) = game {
        for grant in state
            .hand_grants
            .iter()
            .filter(|grant| grant.recipient == *recipient)
        {
            let player = state
                .member(&grant.player)
                .and_then(|owner| owner.seat)
                .ok_or(ProjectionError::UnknownViewer)?;
            granted_hands.push(HandProjection {
                player: grant.player.clone(),
                grant_epoch: grant.grant_epoch,
                cards: game.private_hand(player).map_err(ProjectionError::Game)?,
            });
        }
    }

    Ok(ProjectionPayload {
        phase: room_phase(&state.phase),
        members: state
            .members
            .iter()
            .map(|member| MemberProjection {
                principal_id: member.principal_id.clone(),
                connected: member.connection == ConnectionState::Connected,
                seat: member.seat,
                ready: member.ready,
                host: member.host,
            })
            .collect(),
        public_game_state,
        own_hand,
        granted_hands,
        public_history: state.public_history.clone(),
    })
}

/// Reveal every active hand only to a separately authorized local diagnostic.
///
/// # Errors
///
/// Rejects a capability from another room/epoch/host or a game conversion
/// failure.
pub fn project_local_host_diagnostics<G: SessionGame>(
    state: &SessionState<G>,
    capability: &LocalHostDiagnosticCapability,
) -> Result<LocalHostDiagnosticProjection, ProjectionError<G::Error>> {
    let valid = capability.room_id == state.room_id
        && capability.session_epoch == state.session_epoch
        && state.host.as_ref() == Some(&capability.host)
        && state
            .member(&capability.host)
            .is_some_and(|member| member.host && member.connection == ConnectionState::Connected);
    if !valid {
        return Err(ProjectionError::UnknownViewer);
    }
    let Some(game) = active_game(&state.phase) else {
        return Ok(LocalHostDiagnosticProjection { hands: Vec::new() });
    };
    let mut hands = Vec::new();
    for (member, seat) in state
        .members
        .iter()
        .filter_map(|member| member.seat.map(|seat| (member, seat)))
    {
        hands.push(HandProjection {
            player: member.principal_id.clone(),
            grant_epoch: member.membership_epoch,
            cards: game.private_hand(seat).map_err(ProjectionError::Game)?,
        });
    }
    Ok(LocalHostDiagnosticProjection { hands })
}

fn active_game<G>(phase: &SessionPhase<G>) -> Option<&G> {
    match phase {
        SessionPhase::Running { game }
        | SessionPhase::Paused { game }
        | SessionPhase::PostGame { game } => Some(game),
        SessionPhase::Uninitialized
        | SessionPhase::Lobby
        | SessionPhase::Countdown { .. }
        | SessionPhase::Closed => None,
    }
}

fn room_phase<G>(phase: &SessionPhase<G>) -> RoomPhase {
    match phase {
        SessionPhase::Uninitialized | SessionPhase::Lobby => RoomPhase::Lobby,
        SessionPhase::Countdown { .. } => RoomPhase::Countdown,
        SessionPhase::Running { .. } => RoomPhase::Running,
        SessionPhase::Paused { .. } => RoomPhase::Paused,
        SessionPhase::PostGame { .. } => RoomPhase::PostGame,
        SessionPhase::Closed => RoomPhase::Closed,
    }
}
