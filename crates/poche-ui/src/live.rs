// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_protocol::{
    CommandId, CommandPayload, CountdownToken, InviteProof, PrincipalId, RoomPhase,
};

use crate::{ConnectionPresentation, PresentationModel, action_label};

/// A pending hand-view request safe to show only to an involved viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandRequestPresentation {
    pub request_id: CommandId,
    pub player: PrincipalId,
    pub recipient: PrincipalId,
}

/// Active capability metadata; card data remains solely in viewer projections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandGrantPresentation {
    pub player: PrincipalId,
    pub recipient: PrincipalId,
    pub grant_epoch: u64,
}

/// One explicitly displayable invite; its retained typed command stays adapter-side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomInvitePresentation {
    pub label: String,
    pub code: String,
}

/// One server-retained typed command represented by an opaque UI control ID.
///
/// Renderers expose `id` and `label`, never the serialized payload. A live
/// adapter resolves the ID back to this value and still submits it through the
/// authority's ordinary authorization and reducer boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedUiControl {
    pub id: String,
    pub label: String,
    pub payload: CommandPayload,
}

/// Explicit public/client-local values supplementing an exact projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveClientInput {
    pub room_id: String,
    /// Stable label for the concrete authority incarnation serving this view.
    pub authority_instance: String,
    /// Current committed authority revision for stale-tab diagnostics.
    pub authority_revision: u64,
    pub room_invites: Vec<RoomInvitePresentation>,
    pub join_proof: Option<InviteProof>,
    pub seat_count: u8,
    pub chat_draft: Option<String>,
    pub countdown_command: Option<(u64, CountdownToken)>,
    pub next_grant_epoch: u64,
    pub hand_requests: Vec<HandRequestPresentation>,
    pub hand_grants: Vec<HandGrantPresentation>,
    pub transcript_href: Option<String>,
    pub replay_href: Option<String>,
}

/// Complete deterministic live-client model shared by egui and semantic HTML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveClientPresentation {
    pub projection: PresentationModel,
    pub room_id: String,
    pub authority_instance: String,
    pub authority_revision: u64,
    pub room_invites: Vec<RoomInvitePresentation>,
    pub hand_requests: Vec<HandRequestPresentation>,
    pub hand_grants: Vec<HandGrantPresentation>,
    pub transcript_href: Option<String>,
    pub replay_href: Option<String>,
    pub controls: Vec<TypedUiControl>,
}

impl LiveClientPresentation {
    /// Combine one exact projection with public/client-local supplements.
    #[must_use]
    pub fn from_input(projection: PresentationModel, input: LiveClientInput) -> Self {
        let controls = derive_typed_controls(&projection, &input);
        Self {
            projection,
            room_id: input.room_id,
            authority_instance: input.authority_instance,
            authority_revision: input.authority_revision,
            room_invites: input.room_invites,
            hand_requests: input.hand_requests,
            hand_grants: input.hand_grants,
            transcript_href: input.transcript_href,
            replay_href: input.replay_href,
            controls,
        }
    }

    /// Resolve an opaque renderer control to a typed payload.
    #[must_use]
    pub fn command(&self, control_id: &str) -> Option<CommandPayload> {
        self.controls
            .iter()
            .find(|control| control.id == control_id)
            .map(|control| control.payload.clone())
    }
}

fn derive_typed_controls(
    model: &PresentationModel,
    input: &LiveClientInput,
) -> Vec<TypedUiControl> {
    let mut controls = Vec::new();
    let viewer = model.viewer.as_str();
    let member = model
        .members
        .iter()
        .find(|member| member.principal == viewer);

    if member.is_none() {
        if model.members.is_empty() {
            push_control(
                &mut controls,
                "create-room",
                "Create room",
                CommandPayload::CreateRoom,
            );
        }
        if let Some(invite) = &input.join_proof {
            push_control(
                &mut controls,
                "join-room",
                "Join this room",
                CommandPayload::RedeemInvite {
                    invite: invite.clone(),
                },
            );
        }
        return controls;
    }

    let member = member.expect("checked above");
    let seated = member.seat.is_some();
    let connected = member.connected;
    let is_host = member.role == "host";

    if !connected
        || model.connection == ConnectionPresentation::Reconnecting
        || model.connection == ConnectionPresentation::Disconnected
    {
        push_control(
            &mut controls,
            "reconnect",
            "Reconnect",
            CommandPayload::Reconnect,
        );
        return controls;
    }

    add_phase_controls(&mut controls, model, input, seated, is_host, member.ready);
    add_hand_controls(&mut controls, input, viewer);
    add_common_controls(&mut controls, model, input, connected, is_host);
    controls
}

fn add_phase_controls(
    controls: &mut Vec<TypedUiControl>,
    model: &PresentationModel,
    input: &LiveClientInput,
    seated: bool,
    is_host: bool,
    ready: bool,
) {
    match model.room_phase {
        RoomPhase::Lobby => add_lobby_controls(controls, model, input, seated, is_host, ready),
        RoomPhase::Countdown if seated => push_control(
            controls,
            "abort-countdown",
            "Abort countdown",
            CommandPayload::AbortCountdown,
        ),
        RoomPhase::Running => add_running_controls(controls, model, seated),
        RoomPhase::Paused if seated => {
            push_control(controls, "unpause", "Resume game", CommandPayload::Unpause);
        }
        RoomPhase::PostGame if is_host => push_control(
            controls,
            "reset-lobby",
            "Return to lobby",
            CommandPayload::ResetLobby,
        ),
        RoomPhase::Countdown | RoomPhase::Paused | RoomPhase::PostGame | RoomPhase::Closed => {}
    }
}

fn add_lobby_controls(
    controls: &mut Vec<TypedUiControl>,
    model: &PresentationModel,
    input: &LiveClientInput,
    seated: bool,
    is_host: bool,
    ready: bool,
) {
    if seated {
        push_control(
            controls,
            "release-seat",
            "Become spectator",
            CommandPayload::ReleaseSeat,
        );
        let (id, label, payload) = if ready {
            ("unready", "Not ready", CommandPayload::Unready)
        } else {
            ("ready", "Ready", CommandPayload::Ready)
        };
        push_control(controls, id, label, payload);
    } else {
        for seat in 0..input.seat_count {
            if !model
                .members
                .iter()
                .any(|candidate| candidate.seat == Some(seat))
            {
                push_control(
                    controls,
                    &format!("take-seat-{seat}"),
                    &format!("Take seat {seat}"),
                    CommandPayload::TakeSeat { seat },
                );
            }
        }
    }
    let seated_members = model
        .members
        .iter()
        .filter(|candidate| candidate.seat.is_some())
        .collect::<Vec<_>>();
    if is_host
        && seated_members.len() == usize::from(input.seat_count)
        && seated_members.iter().all(|candidate| candidate.ready)
        && let Some((deadline_tick, countdown_token)) = &input.countdown_command
    {
        push_control(
            controls,
            "arm-countdown",
            "Start countdown",
            CommandPayload::ArmCountdown {
                deadline_tick: *deadline_tick,
                countdown_token: countdown_token.clone(),
            },
        );
    }
}

fn add_running_controls(
    controls: &mut Vec<TypedUiControl>,
    model: &PresentationModel,
    seated: bool,
) {
    if seated {
        push_control(controls, "pause", "Pause game", CommandPayload::Pause);
        for (index, action) in model.legal_actions.iter().enumerate() {
            push_control(
                controls,
                &format!("game-action-{index}"),
                &action_label(action),
                CommandPayload::GameAction {
                    action: action.clone(),
                },
            );
        }
    } else {
        for (index, player) in model
            .members
            .iter()
            .filter(|candidate| candidate.seat.is_some())
            .enumerate()
        {
            if let Ok(player) = PrincipalId::new(player.principal.clone()) {
                push_control(
                    controls,
                    &format!("request-hand-{index}"),
                    &format!("Request {} hand", player.as_str()),
                    CommandPayload::RequestHand { player },
                );
            }
        }
    }
}

fn add_hand_controls(controls: &mut Vec<TypedUiControl>, input: &LiveClientInput, viewer: &str) {
    for (index, request) in input.hand_requests.iter().enumerate() {
        if request.player.as_str() == viewer {
            push_control(
                controls,
                &format!("grant-hand-{index}"),
                &format!("Grant hand to {}", request.recipient.as_str()),
                CommandPayload::GrantHand {
                    request_id: request.request_id.clone(),
                    player: request.player.clone(),
                    recipient: request.recipient.clone(),
                    grant_epoch: input.next_grant_epoch,
                },
            );
            push_control(
                controls,
                &format!("deny-hand-{index}"),
                &format!("Deny hand to {}", request.recipient.as_str()),
                CommandPayload::DenyHand {
                    request_id: request.request_id.clone(),
                    player: request.player.clone(),
                    recipient: request.recipient.clone(),
                },
            );
        }
    }
    for (index, grant) in input.hand_grants.iter().enumerate() {
        if grant.player.as_str() == viewer {
            push_control(
                controls,
                &format!("revoke-hand-{index}"),
                &format!("Revoke {} hand view", grant.recipient.as_str()),
                CommandPayload::RevokeHand {
                    player: grant.player.clone(),
                    recipient: grant.recipient.clone(),
                    grant_epoch: grant.grant_epoch,
                },
            );
        }
    }
}

fn add_common_controls(
    controls: &mut Vec<TypedUiControl>,
    model: &PresentationModel,
    input: &LiveClientInput,
    connected: bool,
    is_host: bool,
) {
    if !connected || model.room_phase == RoomPhase::Closed {
        return;
    }
    if let Some(text) = &input.chat_draft {
        push_control(
            controls,
            "send-chat",
            "Send chat",
            CommandPayload::Chat { text: text.clone() },
        );
    }
    if is_host {
        push_control(
            controls,
            "close-room",
            "Close room",
            CommandPayload::CloseRoom,
        );
    } else {
        push_control(controls, "leave-room", "Leave room", CommandPayload::Leave);
    }
}

fn push_control(
    controls: &mut Vec<TypedUiControl>,
    id: &str,
    label: &str,
    payload: CommandPayload,
) {
    controls.push(TypedUiControl {
        id: id.to_owned(),
        label: label.to_owned(),
        payload,
    });
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        CommandPayload, CountdownToken, GameActionWire, MemberProjection, PrincipalId,
        ProjectionPayload, RoomPhase,
    };

    use crate::{
        ConnectionPresentation, LiveClientInput, LiveClientPresentation, PresentationInput,
        PresentationModel, RoomInvitePresentation,
    };

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).expect("test principal")
    }

    fn model(viewer: &str, seated: bool) -> PresentationModel {
        PresentationModel::from_input(PresentationInput {
            viewer: viewer.to_owned(),
            projection: ProjectionPayload {
                phase: RoomPhase::Running,
                members: vec![
                    MemberProjection {
                        principal_id: principal("alice"),
                        connected: true,
                        seat: Some(0),
                        ready: false,
                        host: true,
                    },
                    MemberProjection {
                        principal_id: principal("spectator"),
                        connected: true,
                        seat: seated.then_some(1),
                        ready: false,
                        host: false,
                    },
                ],
                public_game_state: None,
                own_hand: None,
                granted_hands: Vec::new(),
                public_history: Vec::new(),
            },
            legal_actions: vec![GameActionWire::Bid { tricks: 0 }],
            connection: ConnectionPresentation::Connected,
            countdown: None,
            chat: Vec::new(),
            notices: Vec::new(),
        })
    }

    fn live(viewer: &str, seated: bool) -> LiveClientPresentation {
        LiveClientPresentation::from_input(model(viewer, seated), live_input())
    }

    fn live_input() -> LiveClientInput {
        LiveClientInput {
            room_id: "room".to_owned(),
            authority_instance: "test-live/0".to_owned(),
            authority_revision: 7,
            room_invites: vec![RoomInvitePresentation {
                label: "Test player".to_owned(),
                code: "CODE".to_owned(),
            }],
            join_proof: None,
            seat_count: 2,
            chat_draft: Some("hello".to_owned()),
            countdown_command: None,
            next_grant_epoch: 1,
            hand_requests: Vec::new(),
            hand_grants: Vec::new(),
            transcript_href: Some("/transcript".to_owned()),
            replay_href: Some("/replay".to_owned()),
        }
    }

    #[test]
    fn spectator_controls_are_typed_and_cannot_pause_or_act() {
        let live = live("spectator", false);
        assert!(
            live.controls
                .iter()
                .any(|control| matches!(control.payload, CommandPayload::RequestHand { .. }))
        );
        assert!(
            live.controls
                .iter()
                .any(|control| matches!(control.payload, CommandPayload::Chat { .. }))
        );
        assert!(!live.controls.iter().any(|control| matches!(
            control.payload,
            CommandPayload::Pause | CommandPayload::GameAction { .. }
        )));
    }

    #[test]
    fn seated_player_controls_resolve_to_exact_typed_payloads() {
        let live = live("spectator", true);
        assert_eq!(live.command("pause"), Some(CommandPayload::Pause));
        assert_eq!(
            live.command("game-action-0"),
            Some(CommandPayload::GameAction {
                action: GameActionWire::Bid { tricks: 0 },
            })
        );
        assert!(live.command("unknown").is_none());
    }

    #[test]
    fn disconnected_member_has_only_a_reconnect_command() {
        let mut disconnected = model("spectator", false);
        disconnected.connection = ConnectionPresentation::Reconnecting;
        disconnected.members[1].connected = false;
        let live = LiveClientPresentation::from_input(disconnected, live_input());
        assert_eq!(live.controls.len(), 1);
        assert_eq!(live.command("reconnect"), Some(CommandPayload::Reconnect));
    }

    #[test]
    fn lifecycle_phases_expose_only_their_typed_commands() {
        let mut lobby = model("alice", true);
        lobby.room_phase = RoomPhase::Lobby;
        for member in &mut lobby.members {
            member.ready = true;
        }
        let mut lobby_input = live_input();
        lobby_input.countdown_command = Some((
            7,
            CountdownToken::new("phase-test").expect("countdown token"),
        ));
        let lobby = LiveClientPresentation::from_input(lobby, lobby_input);
        assert_eq!(
            lobby.command("release-seat"),
            Some(CommandPayload::ReleaseSeat)
        );
        assert_eq!(lobby.command("unready"), Some(CommandPayload::Unready));
        assert!(matches!(
            lobby.command("arm-countdown"),
            Some(CommandPayload::ArmCountdown {
                deadline_tick: 7,
                ..
            })
        ));

        let mut countdown = model("spectator", true);
        countdown.room_phase = RoomPhase::Countdown;
        let countdown = LiveClientPresentation::from_input(countdown, live_input());
        assert_eq!(
            countdown.command("abort-countdown"),
            Some(CommandPayload::AbortCountdown)
        );

        let mut paused = model("spectator", true);
        paused.room_phase = RoomPhase::Paused;
        let paused = LiveClientPresentation::from_input(paused, live_input());
        assert_eq!(paused.command("unpause"), Some(CommandPayload::Unpause));

        let mut postgame = model("alice", true);
        postgame.room_phase = RoomPhase::PostGame;
        let postgame = LiveClientPresentation::from_input(postgame, live_input());
        assert_eq!(
            postgame.command("reset-lobby"),
            Some(CommandPayload::ResetLobby)
        );

        let mut closed = model("alice", true);
        closed.room_phase = RoomPhase::Closed;
        let closed = LiveClientPresentation::from_input(closed, live_input());
        assert!(closed.controls.is_empty());
    }
}
