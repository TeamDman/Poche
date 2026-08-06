// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Accessible HTML projection of the renderer-neutral tabletop scene.

use core::fmt::Write as _;

use poche_protocol::{CommandPayload, GameActionWire};
use poche_spatial::{CardLocation, SpatialScene, spatial_scene_hash_hex};

use crate::{LiveClientPresentation, escape_html, render_live_diagnostics};

/// One rule-audit result safe for the current exact recipient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabletopFindingPresentation {
    pub id: String,
    pub summary: String,
    pub status: String,
}

/// One vote safe for ordinary semantic rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabletopVotePresentation {
    pub voter: String,
    pub choice: String,
    pub counted: bool,
    pub exclusion: Option<String>,
}

/// One governance proposal safe for ordinary semantic rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabletopProposalPresentation {
    pub id: String,
    pub action: String,
    pub status: String,
    pub votes: Vec<TabletopVotePresentation>,
}

/// Public/client-local additions which do not alter the canonical scene.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabletopHtmlSupplement {
    pub status: Option<String>,
    pub findings: Vec<TabletopFindingPresentation>,
    pub proposals: Vec<TabletopProposalPresentation>,
}

/// Render an accessible, progressively enhanced tabletop projection.
///
/// Card faces come only from the validated exact-recipient scene. Controls
/// carry opaque IDs whose typed payloads remain retained by the adapter.
///
/// # Errors
///
/// Returns a stable scene validation failure before rendering.
pub fn render_tabletop_semantic_html(
    live: &LiveClientPresentation,
    scene: &SpatialScene,
    root_id: &str,
    command_endpoint: &str,
    supplement: &TabletopHtmlSupplement,
) -> Result<String, poche_spatial::SceneError> {
    let scene_hash = spatial_scene_hash_hex(scene)?;
    let prefix = command_endpoint.trim_end_matches('/');
    let mut html = format!(
        "<main id=\"{}\" data-scene-hash=\"{}\"><header><h1>Poche semantic tabletop</h1><p>Viewer <strong>{}</strong>; room <code>{}</code></p><p>Shared scene fingerprint <output data-role=\"scene-hash\"><code>{}</code></output></p></header>",
        escape_html(root_id),
        scene_hash,
        escape_html(&live.projection.viewer),
        escape_html(&live.room_id),
        scene_hash,
    );
    if let Some(status) = &supplement.status {
        let _ = write!(
            html,
            "<p role=\"status\" aria-live=\"polite\">{}</p>",
            escape_html(status)
        );
    }

    render_score_sheet(&mut html, live);
    render_card_zones(&mut html, live, scene, prefix);
    render_controls(&mut html, live, prefix);
    render_history_and_chat(&mut html, live);
    render_governance(&mut html, live, supplement, prefix);
    html.push_str(&render_live_diagnostics(
        live,
        &format!("{root_id}-diagnostics"),
    ));
    html.push_str("</main>");
    Ok(html)
}

fn render_score_sheet(html: &mut String, live: &LiveClientPresentation) {
    html.push_str("<section aria-labelledby=\"score-heading\"><h2 id=\"score-heading\">Score sheet and seats</h2><table><thead><tr><th scope=\"col\">Seat</th><th scope=\"col\">Player</th><th scope=\"col\">Score</th><th scope=\"col\">Cards</th><th scope=\"col\">Tricks</th></tr></thead><tbody>");
    let table = live.projection.table.as_ref();
    for member in live
        .projection
        .members
        .iter()
        .filter(|member| member.seat.is_some())
    {
        let ordinal = usize::from(member.seat.unwrap_or_default());
        let score = table
            .and_then(|value| value.scores.get(ordinal))
            .copied()
            .unwrap_or_default();
        let cards = table
            .and_then(|value| value.hand_counts.get(ordinal))
            .copied()
            .unwrap_or_default();
        let tricks = table
            .and_then(|value| value.tricks_won.get(ordinal))
            .copied()
            .unwrap_or_default();
        let _ = write!(
            html,
            "<tr><th scope=\"row\">{}</th><td>{}</td><td>{score}</td><td>{cards}</td><td>{tricks}</td></tr>",
            member.seat.unwrap_or_default(),
            escape_html(&member.principal),
        );
    }
    html.push_str("</tbody></table></section>");
}

fn render_card_zones(
    html: &mut String,
    live: &LiveClientPresentation,
    scene: &SpatialScene,
    prefix: &str,
) {
    let deck_count = scene
        .cards
        .iter()
        .filter(|card| matches!(card.location, CardLocation::Deck { .. }))
        .count();
    let _ = write!(
        html,
        "<section aria-labelledby=\"deck-heading\"><h2 id=\"deck-heading\">Deck</h2><p>{deck_count} opaque cards remain in the deck zone.</p></section>"
    );
    html.push_str("<section aria-labelledby=\"trick-heading\"><h2 id=\"trick-heading\">Trump and current trick</h2>");
    if let Some(table) = &live.projection.table {
        let _ = write!(
            html,
            "<p>Trump: {}</p><ol>",
            table
                .trump
                .as_deref()
                .map_or_else(|| "not revealed".to_owned(), escape_html)
        );
        for (seat, card) in &table.trick {
            let _ = write!(html, "<li>Seat {seat}: {}</li>", escape_html(card));
        }
        html.push_str("</ol>");
    }
    html.push_str("</section>");

    if let Some(hand) = &live.projection.own_hand {
        let _ = write!(
            html,
            "<section aria-labelledby=\"own-hand-heading\"><h2 id=\"own-hand-heading\">Your hand — {}</h2><p id=\"hand-help\">Activate a card button, or drag it onto the play target. Both propose the same retained typed command.</p><ul class=\"hand\">",
            escape_html(&hand.player)
        );
        for (code, label) in hand.card_codes.iter().zip(&hand.cards) {
            let control = live.controls.iter().find(|control| {
                matches!(
                    control.payload,
                    CommandPayload::GameAction {
                        action: GameActionWire::Play { card }
                    } if card == *code
                )
            });
            if let Some(control) = control {
                let endpoint = format!("{prefix}/{}", control.id);
                let _ = write!(
                    html,
                    "<li><form method=\"post\" action=\"{}\"><button type=\"submit\" draggable=\"true\" data-card-code=\"{}\" data-command-id=\"{}\" aria-describedby=\"hand-help\">Play {}</button></form></li>",
                    escape_html(&endpoint),
                    code,
                    escape_html(&control.id),
                    escape_html(label)
                );
            } else {
                let _ = write!(
                    html,
                    "<li><span>{} (not currently legal)</span></li>",
                    escape_html(label)
                );
            }
        }
        html.push_str("</ul><div id=\"play-target\" tabindex=\"0\" aria-label=\"Current trick play target\">Play zone</div></section>");
    }
    for hand in &live.projection.granted_hands {
        let _ = write!(
            html,
            "<section aria-label=\"Granted hand view\"><h2>Granted hand — {}</h2><ul>",
            escape_html(&hand.player)
        );
        for card in &hand.cards {
            let _ = write!(html, "<li>{}</li>", escape_html(card));
        }
        html.push_str("</ul></section>");
    }
}

fn render_controls(html: &mut String, live: &LiveClientPresentation, prefix: &str) {
    html.push_str("<section aria-labelledby=\"commands-heading\"><h2 id=\"commands-heading\">Typed commands</h2><div class=\"actions\">");
    for control in &live.controls {
        if matches!(
            control.payload,
            CommandPayload::GameAction {
                action: GameActionWire::Play { .. }
            }
        ) {
            continue;
        }
        let endpoint = format!("{prefix}/{}", control.id);
        let _ = write!(
            html,
            "<form method=\"post\" action=\"{}\"><button type=\"submit\" data-command-id=\"{}\">{}</button></form>",
            escape_html(&endpoint),
            escape_html(&control.id),
            escape_html(&control.label)
        );
    }
    html.push_str("</div></section>");
}

fn render_history_and_chat(html: &mut String, live: &LiveClientPresentation) {
    html.push_str("<section aria-labelledby=\"history-heading\"><h2 id=\"history-heading\">Public history</h2><ol>");
    for event in &live.projection.history {
        let _ = write!(html, "<li>{}</li>", escape_html(event));
    }
    html.push_str("</ol></section><section aria-labelledby=\"chat-heading\"><h2 id=\"chat-heading\">Room chat</h2><ol>");
    for message in &live.projection.chat {
        let _ = write!(
            html,
            "<li><strong>{}</strong>: {}</li>",
            escape_html(&message.principal),
            escape_html(&message.text)
        );
    }
    html.push_str("</ol></section>");
}

fn render_governance(
    html: &mut String,
    live: &LiveClientPresentation,
    supplement: &TabletopHtmlSupplement,
    prefix: &str,
) {
    html.push_str("<section aria-labelledby=\"findings-heading\"><h2 id=\"findings-heading\">Rule findings</h2><ul>");
    for finding in &supplement.findings {
        let _ = write!(
            html,
            "<li><code>{}</code>: {} — {}</li>",
            escape_html(&finding.id),
            escape_html(&finding.summary),
            escape_html(&finding.status)
        );
    }
    html.push_str("</ul>");
    let viewer_can_govern = live.projection.members.iter().any(|member| {
        member.principal == live.projection.viewer && member.connected && member.seat.is_some()
    });
    if viewer_can_govern {
        let accuse = format!("{prefix}/accuse");
        let _ = write!(
            html,
            "<form method=\"post\" action=\"{}\"><button type=\"submit\" data-command-id=\"accuse\">Accuse recorded follow-suit action</button></form>",
            escape_html(&accuse)
        );
    }
    html.push_str("</section>");

    html.push_str("<section aria-labelledby=\"proposals-heading\"><h2 id=\"proposals-heading\">Proposals and votes</h2>");
    for proposal in &supplement.proposals {
        let _ = write!(
            html,
            "<article><h3><code>{}</code></h3><p>{} — {}</p><table><thead><tr><th>Voter</th><th>Choice</th><th>Counted</th><th>Exclusion</th></tr></thead><tbody>",
            escape_html(&proposal.id),
            escape_html(&proposal.action),
            escape_html(&proposal.status)
        );
        for vote in &proposal.votes {
            let _ = write!(
                html,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&vote.voter),
                escape_html(&vote.choice),
                vote.counted,
                vote.exclusion
                    .as_deref()
                    .map(escape_html)
                    .unwrap_or_default()
            );
        }
        html.push_str("</tbody></table></article>");
    }
    if viewer_can_govern {
        let start = format!("{prefix}/start-vote");
        let vote = format!("{prefix}/vote-approve");
        let _ = write!(
            html,
            "<div class=\"actions\"><form method=\"post\" action=\"{}\"><button type=\"submit\" data-command-id=\"start-vote\">Start redeal vote</button></form><form method=\"post\" action=\"{}\"><button type=\"submit\" data-command-id=\"vote-approve\">Vote to approve</button></form></div>",
            escape_html(&start),
            escape_html(&vote)
        );
    }
    html.push_str("</section>");
}

#[cfg(test)]
mod tests {
    use poche_spatial::spatial_scene_hash_hex;

    use crate::{
        LiveClientInput, LiveClientPresentation, TabletopHtmlSupplement, embedded_spatial_fixture,
        render_tabletop_semantic_html,
    };

    #[test]
    fn semantic_tabletop_uses_shared_hash_and_never_emits_hidden_faces() {
        let fixture = embedded_spatial_fixture().expect("fixture");
        let mut live = LiveClientPresentation::from_input(
            fixture.presentation,
            LiveClientInput {
                room_id: "fixture-room".to_owned(),
                authority_instance: "tabletop-test/0".to_owned(),
                authority_revision: 0,
                room_code: None,
                join_proof: None,
                seat_count: fixture.layout.id().players(),
                chat_draft: None,
                countdown_command: None,
                next_grant_epoch: 2,
                hand_requests: Vec::new(),
                hand_grants: Vec::new(),
                transcript_href: None,
                replay_href: None,
            },
        );
        let html = render_tabletop_semantic_html(
            &live,
            &fixture.scene,
            "tabletop",
            "/tabletop/action",
            &TabletopHtmlSupplement::default(),
        )
        .expect("HTML");
        let hash = spatial_scene_hash_hex(&fixture.scene).expect("hash");
        assert!(html.contains(&format!("data-scene-hash=\"{hash}\"")));
        assert!(html.contains("Score sheet and seats"));
        assert!(html.contains("What state am I in?"));
        assert!(html.contains("Copy diagnostic context"));
        assert!(html.contains("draggable=\"true\"") || html.contains("not currently legal"));
        let visible_faces = fixture
            .scene
            .cards
            .iter()
            .filter(|card| card.face.is_some())
            .count();
        assert_eq!(
            html.matches("data-card-code=").count(),
            visible_faces.min(live.projection.legal_actions.len())
        );
        assert!(!html.contains("camera"));
        assert!(!html.contains("position:"));

        let viewer = live.projection.viewer.clone();
        live.projection
            .members
            .iter_mut()
            .filter(|member| member.principal == viewer)
            .for_each(|member| member.connected = false);
        let disconnected = render_tabletop_semantic_html(
            &live,
            &fixture.scene,
            "tabletop",
            "/tabletop/action",
            &TabletopHtmlSupplement::default(),
        )
        .expect("disconnected HTML");
        assert!(!disconnected.contains("data-command-id=\"accuse\""));
        assert!(!disconnected.contains("data-command-id=\"start-vote\""));
    }
}
