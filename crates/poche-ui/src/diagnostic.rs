// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Copy-safe diagnostics and an SVG projection of the runtime state machines.

use core::fmt::Write as _;

use poche_protocol::{PublicGamePhase, RoomPhase};

use crate::{ConnectionPresentation, LiveClientPresentation, escape_html};

const ROOM_STATES: &[&str] = &[
    "Lobby",
    "Countdown",
    "Running",
    "Paused",
    "PostGame",
    "Closed",
];
const TRANSPORT_STATES: &[&str] = &["Replay", "Connected", "Reconnecting", "Disconnected"];
const GAME_STATES: &[&str] = &["AwaitingDeal", "Bidding", "Playing", "Scoring", "Finished"];

/// Renderer-neutral state-machine annotation derived from one exact-recipient view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateMachineDiagram {
    pub room: RoomPhase,
    pub transport: ConnectionPresentation,
    pub game: Option<PublicGamePhase>,
}

impl From<&LiveClientPresentation> for StateMachineDiagram {
    fn from(live: &LiveClientPresentation) -> Self {
        Self {
            room: live.projection.room_phase,
            transport: live.projection.connection,
            game: live.projection.table.as_ref().map(|table| table.phase),
        }
    }
}

impl StateMachineDiagram {
    /// Render a compact accessible SVG with the current node highlighted in each lane.
    #[must_use]
    pub fn render_svg(&self, root_id: &str) -> String {
        let marker_id = format!("{root_id}-arrow");
        let title_id = format!("{root_id}-title");
        let description_id = format!("{root_id}-description");
        let mut svg = format!(
            "<svg class=\"state-machine-diagram\" viewBox=\"0 0 980 268\" role=\"img\" aria-labelledby=\"{} {}\"><title id=\"{}\">Current Poche runtime state machines</title><desc id=\"{}\">Room, viewer transport, and game phase lanes. The current state in each lane is highlighted.</desc><defs><marker id=\"{}\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"currentColor\" /></marker></defs>",
            escape_html(&title_id),
            escape_html(&description_id),
            escape_html(&title_id),
            escape_html(&description_id),
            escape_html(&marker_id),
        );
        render_lane(
            &mut svg,
            "Room",
            ROOM_STATES,
            &format!("{:?}", self.room),
            18,
            &marker_id,
        );
        render_lane(
            &mut svg,
            "Transport",
            TRANSPORT_STATES,
            &format!("{:?}", self.transport),
            104,
            &marker_id,
        );
        render_lane(
            &mut svg,
            "Game",
            GAME_STATES,
            &self
                .game
                .map_or_else(|| "Inactive".to_owned(), |phase| format!("{phase:?}")),
            190,
            &marker_id,
        );
        svg.push_str("</svg>");
        svg
    }
}

fn render_lane(
    svg: &mut String,
    label: &str,
    states: &[&str],
    current: &str,
    y: u16,
    marker_id: &str,
) {
    let _ = write!(
        svg,
        "<g data-machine=\"{}\"><text class=\"lane-label\" x=\"8\" y=\"{}\">{}</text>",
        label.to_ascii_lowercase(),
        y + 31,
        label,
    );
    let node_width = 128_u16;
    let gap = 18_u16;
    let start_x = 112_u16;
    for (index, state) in states.iter().enumerate() {
        let x = start_x + u16::try_from(index).unwrap_or_default() * (node_width + gap);
        if index > 0 {
            let previous_x = x - gap;
            let _ = write!(
                svg,
                "<path class=\"state-edge\" d=\"M {previous_x} {} H {}\" marker-end=\"url(#{})\" />",
                y + 25,
                x - 3,
                escape_html(marker_id),
            );
        }
        let active = *state == current;
        let _ = write!(
            svg,
            "<g class=\"state-node{}\" data-state=\"{}\" data-current=\"{}\"><rect x=\"{x}\" y=\"{y}\" width=\"{node_width}\" height=\"50\" rx=\"8\" /><text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text></g>",
            if active { " current" } else { "" },
            escape_html(state),
            active,
            x + node_width / 2,
            y + 31,
            escape_html(state),
        );
    }
    if label == "Game" && current == "Inactive" {
        let _ = write!(
            svg,
            "<text class=\"inactive-note\" x=\"820\" y=\"{}\">inactive until a game exists</text>",
            y + 67,
        );
    }
    svg.push_str("</g>");
}

/// Build the default shareable context. Secrets and private card faces are omitted.
#[must_use]
pub fn live_diagnostic_context(live: &LiveClientPresentation) -> String {
    let model = &live.projection;
    let mut context = String::new();
    let _ = writeln!(context, "POCHE DIAGNOSTIC CONTEXT v1");
    let _ = writeln!(
        context,
        "authority_instance: {}",
        one_line(&live.authority_instance)
    );
    let _ = writeln!(context, "authority_revision: {}", live.authority_revision);
    let _ = writeln!(context, "room_label: {}", one_line(&live.room_id));
    let _ = writeln!(context, "viewer: {}", one_line(&model.viewer_display_name));
    let _ = writeln!(context, "viewer_principal: {}", one_line(&model.viewer));
    let _ = writeln!(context, "room_state: {:?}", model.room_phase);
    let _ = writeln!(context, "transport_state: {:?}", model.connection);
    let game_state = model.table.as_ref().map_or_else(
        || "Inactive".to_owned(),
        |table| format!("{:?}", table.phase),
    );
    let _ = writeln!(context, "game_state: {game_state}");
    let _ = writeln!(
        context,
        "state_explanation: {}",
        transport_explanation(model.connection)
    );

    write_members(&mut context, live);
    write_public_game(&mut context, live);
    write_commands(&mut context, live);
    write_histories(&mut context, live);
    write_privacy_and_evidence(&mut context, live);
    context
}

fn write_members(context: &mut String, live: &LiveClientPresentation) {
    context.push_str("\nmembers:\n");
    let model = &live.projection;
    for member in &model.members {
        let _ = writeln!(
            context,
            "- {} | principal={} | role={} | seat={} | ready={} | connected={}",
            one_line(&member.display_name),
            one_line(&member.principal),
            member.role,
            member
                .seat
                .map_or_else(|| "none".to_owned(), |seat| seat.to_string()),
            member.ready,
            member.connected,
        );
    }
}

fn write_public_game(context: &mut String, live: &LiveClientPresentation) {
    let model = &live.projection;
    if let Some(table) = &model.table {
        context.push_str("\npublic_game:\n");
        let _ = writeln!(context, "- actor: {}", one_line(&table.actor));
        let _ = writeln!(context, "- round: {}", table.round_index);
        let _ = writeln!(context, "- scores: {:?}", table.scores);
        let _ = writeln!(context, "- cards_remaining: {:?}", table.hand_counts);
        let _ = writeln!(context, "- bids: {:?}", table.bids);
        let _ = writeln!(context, "- tricks: {:?}", table.tricks_won);
        let _ = writeln!(context, "- pot_cents: {}", table.pot_cents);
        let _ = writeln!(
            context,
            "- trump: {}",
            table.trump.as_deref().unwrap_or("not revealed")
        );
    }
}

fn write_commands(context: &mut String, live: &LiveClientPresentation) {
    context.push_str("\navailable_commands:\n");
    if live.controls.is_empty() {
        context.push_str("- none\n");
    } else {
        for control in &live.controls {
            let _ = writeln!(
                context,
                "- {} ({})",
                one_line(&control.label),
                one_line(&control.id)
            );
        }
    }
}

fn write_histories(context: &mut String, live: &LiveClientPresentation) {
    let model = &live.projection;
    context.push_str("\npublic_history:\n");
    if model.history.is_empty() {
        context.push_str("- none\n");
    } else {
        for event in &model.history {
            let _ = writeln!(context, "- {}", one_line(event));
        }
    }

    context.push_str("\nclient_event_history:\n");
    if model.notices.is_empty() {
        context.push_str("- none\n");
    } else {
        for event in &model.notices {
            let _ = writeln!(
                context,
                "- [{}] {}",
                one_line(&event.reason_code),
                one_line(&event.message)
            );
        }
    }
}

fn write_privacy_and_evidence(context: &mut String, live: &LiveClientPresentation) {
    let model = &live.projection;
    context.push_str("\nprivacy_and_freshness:\n");
    let _ = writeln!(
        context,
        "- own_hand_cards: {}; faces omitted",
        model
            .own_hand
            .as_ref()
            .map_or(0, |hand| hand.card_codes.len())
    );
    let _ = writeln!(
        context,
        "- granted_hand_cards: {}; faces omitted",
        model
            .granted_hands
            .iter()
            .map(|hand| hand.card_codes.len())
            .sum::<usize>()
    );
    let _ = writeln!(context, "- room_join_code: omitted");
    let _ = writeln!(
        context,
        "- chat_messages: {}; contents omitted",
        model.chat.len()
    );
    let _ = writeln!(
        context,
        "- tab_freshness: this view is exact at the authority instance/revision above; the client adapter may stream or refresh later revisions"
    );

    context.push_str("\nevidence_boundary:\n");
    context.push_str(
        "- current state source: typed Rust projection accepted by the runtime reducer\n",
    );
    context.push_str("- SVG source: deterministic mapping from the three runtime enum states\n");
    context.push_str(
        "- Alloy/NuSMV/Prolog: release/developer evidence; not executed for this page request\n",
    );
}

/// Render the explanatory state diagram and copy-safe diagnostic surface.
#[must_use]
pub fn render_live_diagnostics(live: &LiveClientPresentation, root_id: &str) -> String {
    let diagram = StateMachineDiagram::from(live);
    let context_id = format!("{root_id}-context");
    let status_id = format!("{root_id}-copy-status");
    let context = live_diagnostic_context(live);
    let game = diagram
        .game
        .map_or_else(|| "Inactive".to_owned(), |phase| format!("{phase:?}"));
    format!(
        "<section class=\"diagnostics\" aria-labelledby=\"{root_id}-heading\"><h3 id=\"{root_id}-heading\">What state am I in?</h3><p><strong>Room:</strong> {:?} · <strong>this viewer's transport:</strong> {:?} · <strong>game:</strong> {}</p><p>{}</p><p><strong>Authority instance:</strong> <code>{}</code> at revision <strong>{}</strong>. This exact view may receive later revisions through its client adapter; compare these values when diagnosing freshness.</p>{}<details><summary>What this diagram proves—and does not prove</summary><p>The highlighted nodes come from the typed Rust projection accepted by the live reducer. The diagram does not run Alloy, NuSMV, or Prolog during an HTTP request; those remain independent bounded, symbolic, and query-oriented release gates.</p></details><h4>Copy-safe diagnostic context</h4><p>This intentionally omits join codes, chat contents, and private card faces. The ordinary page remains selectable if you deliberately need to share viewer-private content.</p><textarea id=\"{}\" class=\"diagnostic-context\" rows=\"24\" readonly spellcheck=\"false\">{}</textarea><div class=\"actions\"><button type=\"button\" data-copy-context=\"{}\" data-copy-status=\"{}\">Copy diagnostic context</button></div><output id=\"{}\" role=\"status\" aria-live=\"polite\"></output></section>",
        diagram.room,
        diagram.transport,
        escape_html(&game),
        escape_html(transport_explanation(diagram.transport)),
        escape_html(&live.authority_instance),
        live.authority_revision,
        diagram.render_svg(&format!("{root_id}-diagram")),
        escape_html(&context_id),
        escape_html(&context),
        escape_html(&context_id),
        escape_html(&status_id),
        escape_html(&status_id),
    )
}

fn transport_explanation(connection: ConnectionPresentation) -> &'static str {
    match connection {
        ConnectionPresentation::Replay => {
            "This is a retained replay projection; no live transport is expected."
        }
        ConnectionPresentation::Connected => {
            "This viewer is connected to the authority. The server and other viewers have independent transport states."
        }
        ConnectionPresentation::Reconnecting => {
            "The authority/server is still running, but this viewer's simulated transport was lost. Use Reconnect to bind the replacement transport."
        }
        ConnectionPresentation::Disconnected => {
            "This viewer has no current authority stream; server availability is a separate state."
        }
    }
}

fn one_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use poche_protocol::{
        GamePublicStateWire, HandProjection, MemberProjection, PrincipalId, ProjectionPayload,
        PublicGamePhase, PublicTurnWire, RoomPhase,
    };

    use crate::{
        ConnectionPresentation, LiveClientInput, LiveClientPresentation, PresentationInput,
        PresentationModel, live_diagnostic_context, render_live_diagnostics,
    };

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).expect("test principal")
    }

    fn fixture() -> LiveClientPresentation {
        let projection = PresentationModel::from_input(PresentationInput {
            viewer: "alice".to_owned(),
            projection: ProjectionPayload {
                phase: RoomPhase::Running,
                members: vec![MemberProjection {
                    principal_id: principal("alice"),
                    connected: false,
                    seat: Some(0),
                    ready: false,
                    host: true,
                }],
                public_game_state: Some(GamePublicStateWire {
                    schema_version: 1,
                    phase: PublicGamePhase::Bidding,
                    dealer: Some(0),
                    actor: PublicTurnWire::Player(0),
                    round_index: 2,
                    hand_size: 1,
                    hand_counts: vec![1],
                    trump: Some(2),
                    current_trick: Vec::new(),
                    bids: vec![None],
                    tricks_won: vec![0],
                    scores: vec![3],
                    pot_cents: 50,
                }),
                own_hand: Some(HandProjection {
                    player: principal("alice"),
                    grant_epoch: 0,
                    cards: vec![51],
                }),
                granted_hands: Vec::new(),
                public_history: Vec::new(),
            },
            legal_actions: Vec::new(),
            connection: ConnectionPresentation::Reconnecting,
            countdown: None,
            chat: Vec::new(),
            notices: Vec::new(),
        });
        LiveClientPresentation::from_input(
            projection,
            LiveClientInput {
                room_id: "room".to_owned(),
                authority_instance: "web-live/4".to_owned(),
                authority_revision: 19,
                room_invites: vec![crate::RoomInvitePresentation {
                    label: "Private".to_owned(),
                    code: "DO-NOT-COPY".to_owned(),
                }],
                join_proof: None,
                seat_count: 1,
                chat_draft: None,
                countdown_command: None,
                next_grant_epoch: 1,
                hand_requests: Vec::new(),
                hand_grants: Vec::new(),
                transcript_href: None,
                replay_href: None,
            },
        )
    }

    #[test]
    fn diagnostic_context_explains_transport_and_omits_private_material() {
        let context = live_diagnostic_context(&fixture());
        assert!(context.contains("room_state: Running"));
        assert!(context.contains("transport_state: Reconnecting"));
        assert!(context.contains("game_state: Bidding"));
        assert!(context.contains("authority_revision: 19"));
        assert!(context.contains("faces omitted"));
        assert!(!context.contains("AS"));
        assert!(!context.contains("DO-NOT-COPY"));
    }

    #[test]
    fn diagnostic_svg_highlights_each_current_typed_state() {
        let html = render_live_diagnostics(&fixture(), "diagnostic");
        assert!(html.contains("data-state=\"Running\" data-current=\"true\""));
        assert!(html.contains("data-state=\"Reconnecting\" data-current=\"true\""));
        assert!(html.contains("data-state=\"Bidding\" data-current=\"true\""));
        assert!(html.contains("Copy diagnostic context"));
    }
}
