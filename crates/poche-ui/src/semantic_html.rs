// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fmt::Write;

use crate::{HandPresentation, PresentationModel, action_label};

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

    for notice in &model.notices {
        let _ = write!(
            html,
            "<p role=\"alert\"><strong>{}</strong>: {}</p>",
            escape_html(&notice.reason_code),
            escape_html(&notice.message)
        );
    }
    html.push_str("</main>");
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

fn escape_html(value: &str) -> String {
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
    use poche_protocol::{ProjectionPayload, RoomPhase};

    use crate::{
        ChatPresentation, ConnectionPresentation, NoticePresentation, PresentationInput,
        PresentationModel, render_semantic_html, render_semantic_html_with_root_id,
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
        assert!(!html.contains("<script>"));
    }
}
