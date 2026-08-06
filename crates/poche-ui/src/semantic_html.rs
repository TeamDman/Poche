// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fmt::Write;

use crate::{
    HandPresentation, LiveClientPresentation, PresentationModel, action_label,
    render_live_diagnostics,
};

/// Render a semantic HTML fragment from the same deterministic model as egui.
///
/// The returned root always has `id="projection"`, making it suitable for a
/// Datastar `PatchElements` replacement. All projection text is escaped.
#[must_use]
pub fn render_semantic_html(model: &PresentationModel) -> String {
    render_semantic_html_with_root_id(model, "projection")
}

/// Render a semantic HTML fragment with a caller-selected root element ID.
///
/// This lets one page host independent projection surfaces without duplicate
/// DOM IDs. The ID and all projection text are escaped.
#[must_use]
pub fn render_semantic_html_with_root_id(model: &PresentationModel, root_id: &str) -> String {
    let mut html = format!("<main id=\"{}\">", escape_html(root_id));
    let _ = write!(
        html,
        "<h2>{} · {:?}</h2><p>Transport: {:?}</p>",
        escape_html(&model.viewer),
        model.room_phase,
        model.connection
    );
    html.push_str("<section aria-label=\"Members\"><h3>Members</h3><ul>");
    for member in &model.members {
        let _ = write!(
            html,
            "<li>{} — {} — seat {} — {} — {}</li>",
            escape_html(&member.principal),
            member.role,
            member
                .seat
                .map_or_else(|| "none".to_owned(), |seat| seat.to_string()),
            if member.ready { "ready" } else { "not ready" },
            if member.connected {
                "connected"
            } else {
                "disconnected"
            }
        );
    }
    html.push_str("</ul></section>");

    if let Some(countdown) = model.countdown {
        let _ = write!(
            html,
            "<section aria-label=\"Countdown\"><h3>Countdown</h3><p>Authority tick {}; deadline {}; {} ticks remaining.</p></section>",
            countdown.logical_now,
            countdown.deadline_tick,
            countdown.remaining()
        );
    }

    if let Some(table) = &model.table {
        let _ = write!(
            html,
            "<section aria-label=\"Public table\"><h3>Public table</h3><p>Phase: {:?}; round: {}; actor: {}; pot: ${:.2}</p><p>Scores: {:?}; cards remaining: {:?}; bids: {:?}; tricks: {:?}</p>",
            table.phase,
            table.round_index,
            escape_html(&table.actor),
            f64::from(table.pot_cents) / 100.0,
            table.scores,
            table.hand_counts,
            table.bids,
            table.tricks_won
        );
        if let Some(trump) = &table.trump {
            let _ = write!(html, "<p>Trump: {}</p>", escape_html(trump));
        }
        html.push_str("</section>");
    }

    if let Some(hand) = &model.own_hand {
        render_hand(&mut html, "Your hand", hand);
    }
    for hand in &model.granted_hands {
        render_hand(&mut html, "Granted spectator view", hand);
    }

    if !model.legal_actions.is_empty() {
        html.push_str("<section aria-label=\"Legal actions\"><h3>Legal actions</h3><ul>");
        for action in &model.legal_actions {
            let _ = write!(html, "<li>{}</li>", escape_html(&action_label(action)));
        }
        html.push_str("</ul></section>");
    }

    if !model.history.is_empty() {
        html.push_str("<section aria-label=\"Public history\"><h3>Public history</h3><ol>");
        for item in &model.history {
            let _ = write!(html, "<li>{}</li>", escape_html(item));
        }
        html.push_str("</ol></section>");
    }

    if !model.chat.is_empty() {
        html.push_str("<section aria-label=\"Chat\"><h3>Chat</h3><ol>");
        for message in &model.chat {
            let _ = write!(
                html,
                "<li><strong>{}</strong>: {}</li>",
                escape_html(&message.principal),
                escape_html(&message.text)
            );
        }
        html.push_str("</ol></section>");
    }

    render_client_events(&mut html, model);
    html.push_str("</main>");
    html
}

fn render_client_events(html: &mut String, model: &PresentationModel) {
    if model.notices.is_empty() {
        return;
    }
    html.push_str(
        "<section aria-label=\"Client event history\"><h3>Client event history</h3><p>These events belong to this viewer and are separate from the shared public game history.</p><ol>",
    );
    for notice in &model.notices {
        let _ = write!(
            html,
            "<li><strong>{}</strong>: {}</li>",
            escape_html(&notice.reason_code),
            escape_html(&notice.message)
        );
    }
    html.push_str("</ol></section>");
}

/// Render the complete live-client shell around an exact viewer projection.
///
/// Command payloads remain server-side. Buttons contain only escaped opaque
/// control IDs and endpoint paths; the adapter must resolve those IDs through
/// [`LiveClientPresentation::command`] before submitting to the authority.
#[must_use]
pub fn render_live_semantic_html(
    live: &LiveClientPresentation,
    root_id: &str,
    command_endpoint: &str,
) -> String {
    let mut html = format!("<article id=\"{}\">", escape_html(root_id));
    let _ = write!(
        html,
        "<header><h2>Live room client</h2><dl><dt>Identity</dt><dd>{}</dd><dt>Room label</dt><dd>{}</dd><dt>Authority instance</dt><dd><code>{}</code></dd><dt>Authority revision</dt><dd>{}</dd></dl>",
        escape_html(&live.projection.viewer),
        escape_html(&live.room_id),
        escape_html(&live.authority_instance),
        live.authority_revision,
    );
    if let Some(code) = &live.room_code {
        let _ = write!(html, "<p>Join code: <code>{}</code></p>", escape_html(code));
    }
    html.push_str("</header>");

    if !live.hand_requests.is_empty() {
        html.push_str(
            "<section aria-label=\"Pending hand requests\"><h3>Pending hand requests</h3><ul>",
        );
        for request in &live.hand_requests {
            let _ = write!(
                html,
                "<li>{} requests {} hand (request {})</li>",
                escape_html(request.recipient.as_str()),
                escape_html(request.player.as_str()),
                escape_html(request.request_id.as_str())
            );
        }
        html.push_str("</ul></section>");
    }
    if !live.hand_grants.is_empty() {
        html.push_str("<section aria-label=\"Active hand grants\"><h3>Active hand grants</h3><ul>");
        for grant in &live.hand_grants {
            let _ = write!(
                html,
                "<li>{} may view {} hand from epoch {}</li>",
                escape_html(grant.recipient.as_str()),
                escape_html(grant.player.as_str()),
                grant.grant_epoch
            );
        }
        html.push_str("</ul></section>");
    }

    if !live.controls.is_empty() {
        html.push_str(
            "<section aria-label=\"Typed commands\"><h3>Commands</h3><div class=\"actions\">",
        );
        let prefix = command_endpoint.trim_end_matches('/');
        for control in &live.controls {
            let endpoint = format!("{prefix}/{}", control.id);
            let _ = write!(
                html,
                "<button data-command-id=\"{}\" data-on:click=\"@post('{}')\">{}</button>",
                escape_html(&control.id),
                escape_html(&endpoint),
                escape_html(&control.label)
            );
        }
        html.push_str("</div></section>");
    }
    if let Some(refresh_endpoint) = command_endpoint.strip_suffix("/command") {
        let _ = write!(
            html,
            "<p><button type=\"button\" data-on:click=\"@get('{}')\">Refresh this viewer from the authority</button></p>",
            escape_html(refresh_endpoint)
        );
    }
    if let Some(href) = &live.transcript_href {
        let _ = write!(
            html,
            "<p><a href=\"{}\" download>Export canonical transcript</a></p>",
            escape_html(href)
        );
    }
    if let Some(href) = &live.replay_href {
        let _ = write!(
            html,
            "<p><a href=\"{}\">Replay exact projection history</a></p>",
            escape_html(href)
        );
    }
    html.push_str(&render_semantic_html_with_root_id(
        &live.projection,
        &format!("{root_id}-projection"),
    ));
    html.push_str(&render_live_diagnostics(
        live,
        &format!("{root_id}-diagnostics"),
    ));
    html.push_str("</article>");
    html
}

fn render_hand(html: &mut String, heading: &str, hand: &HandPresentation) {
    let _ = write!(
        html,
        "<section><h3>{}: {}</h3><p>",
        heading,
        escape_html(&hand.player)
    );
    for (index, card) in hand.cards.iter().enumerate() {
        if index > 0 {
            html.push(' ');
        }
        let _ = write!(html, "<code>{}</code>", escape_html(card));
    }
    html.push_str("</p></section>");
}

pub(crate) fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use poche_protocol::{InviteProof, ProjectionPayload, RoomPhase};

    use crate::{
        ChatPresentation, ConnectionPresentation, LiveClientInput, LiveClientPresentation,
        NoticePresentation, PresentationInput, PresentationModel, render_live_semantic_html,
        render_semantic_html, render_semantic_html_with_root_id,
    };

    #[test]
    fn semantic_fragment_escapes_all_viewer_controlled_text() {
        let model = PresentationModel::from_input(PresentationInput {
            viewer: "<viewer>".to_owned(),
            projection: ProjectionPayload {
                phase: RoomPhase::Lobby,
                members: Vec::new(),
                public_game_state: None,
                own_hand: None,
                granted_hands: Vec::new(),
                public_history: Vec::new(),
            },
            legal_actions: Vec::new(),
            connection: ConnectionPresentation::Connected,
            countdown: None,
            chat: vec![ChatPresentation {
                principal: "alice&bob".to_owned(),
                text: "<script>alert('no')</script>".to_owned(),
            }],
            notices: vec![NoticePresentation {
                reason_code: "D-TEST".to_owned(),
                message: "\"quoted\"".to_owned(),
            }],
        });

        let html = render_semantic_html(&model);
        assert!(html.starts_with("<main id=\"projection\">"));
        let custom = render_semantic_html_with_root_id(&model, "authority\" onload=\"bad");
        assert!(html.starts_with("<main id=\"projection\">"));
        assert!(custom.starts_with("<main id=\"authority&quot; onload=&quot;bad\">"));
        assert!(!custom.starts_with("<main id=\"authority\" onload="));
        assert!(html.contains("&lt;viewer&gt;"));
        assert!(html.contains("alice&amp;bob"));
        assert!(html.contains("&lt;script&gt;alert(&#39;no&#39;)&lt;/script&gt;"));
        assert!(html.contains("&quot;quoted&quot;"));
        assert!(html.contains("Client event history"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn live_html_exposes_only_control_ids_not_typed_payload_secrets() {
        let projection = PresentationModel::from_input(PresentationInput {
            viewer: "candidate".to_owned(),
            projection: ProjectionPayload {
                phase: RoomPhase::Lobby,
                members: Vec::new(),
                public_game_state: None,
                own_hand: None,
                granted_hands: Vec::new(),
                public_history: Vec::new(),
            },
            legal_actions: Vec::new(),
            connection: ConnectionPresentation::Connected,
            countdown: None,
            chat: Vec::new(),
            notices: Vec::new(),
        });
        let live = LiveClientPresentation::from_input(
            projection,
            LiveClientInput {
                room_id: "room".to_owned(),
                authority_instance: "semantic-html-test/0".to_owned(),
                authority_revision: 0,
                room_code: Some("DISPLAY-CODE".to_owned()),
                join_proof: Some(InviteProof::new("runtime-only-secret").unwrap()),
                seat_count: 2,
                chat_draft: None,
                countdown_command: None,
                next_grant_epoch: 1,
                hand_requests: Vec::new(),
                hand_grants: Vec::new(),
                transcript_href: None,
                replay_href: None,
            },
        );

        let html = render_live_semantic_html(&live, "live", "/command/candidate");
        assert!(html.contains("Join with room code"));
        assert!(html.contains("DISPLAY-CODE"));
        assert!(html.contains("data-command-id=\"join-room\""));
        assert!(html.contains("What state am I in?"));
        assert!(html.contains("POCHE DIAGNOSTIC CONTEXT v1"));
        assert!(html.contains("Copy diagnostic context"));
        assert!(!html.contains("runtime-only-secret"));
    }
}
