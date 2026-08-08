// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Accessible HTML projection of the renderer-neutral tabletop scene.

use core::fmt::Write as _;

use poche_protocol::{CommandPayload, GameActionWire, PublicGamePhase, RoomPhase};
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
    /// Opaque adapter-level room code, shown only to an admitted room member.
    pub room_code: Option<String>,
    /// Player-facing exit from the room client.
    pub main_menu_href: Option<String>,
    /// Adapter-owned route prefix for switching exact-recipient projections.
    pub viewer_href_prefix: Option<String>,
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
        "<main id=\"{}\" class=\"game-shell\" data-scene-hash=\"{}\"><header class=\"game-hud\"><div class=\"game-title\"><span>POCHE</span><small>card table</small></div>{}{}<div class=\"authority-chip\"><span>{:?}</span><small>{:?} · rev {}</small></div><output data-role=\"scene-hash\" hidden>{}</output></header>",
        escape_html(root_id),
        scene_hash,
        render_viewer_switcher(live, supplement),
        render_room_code(supplement, root_id),
        live.projection.room_phase,
        live.projection.connection,
        live.authority_revision,
        scene_hash,
    );
    if let Some(status) = &supplement.status {
        let _ = write!(
            html,
            "<p class=\"command-status\" role=\"status\" aria-live=\"polite\">{}</p>",
            escape_html(status)
        );
    }

    html.push_str("<div class=\"game-layout\"><div class=\"tabletop-primary\">");
    render_score_sheet(&mut html, live);
    render_turn_banner(&mut html, live);
    if matches!(
        live.projection.room_phase,
        RoomPhase::Lobby | RoomPhase::Countdown
    ) {
        render_lobby_table(&mut html, live, prefix);
    } else {
        render_card_zones(&mut html, live, scene, prefix);
    }
    render_controls(&mut html, live, prefix);
    html.push_str("</div><aside class=\"game-inspector\" aria-label=\"Game inspector\">");
    render_history_and_chat(&mut html, live);
    render_governance(&mut html, live, supplement, prefix);
    html.push_str("<details class=\"inspector-panel diagnostics-panel\"><summary>Diagnostics and formal evidence</summary>");
    html.push_str(&render_live_diagnostics(
        live,
        &format!("{root_id}-diagnostics"),
    ));
    html.push_str("</details></aside></div></main>");
    Ok(html)
}

fn render_room_code(supplement: &TabletopHtmlSupplement, root_id: &str) -> String {
    let Some(code) = &supplement.room_code else {
        return String::new();
    };
    let code_id = format!("{root_id}-room-code");
    format!(
        "<div class=\"room-code-chip\"><span>Room code</span><strong id=\"{}\">{}</strong><button type=\"button\" data-copy-text=\"{}\">Copy</button></div>",
        escape_html(&code_id),
        escape_html(code),
        escape_html(&code_id),
    )
}

fn render_viewer_switcher(
    live: &LiveClientPresentation,
    supplement: &TabletopHtmlSupplement,
) -> String {
    let viewer = &live.projection.viewer;
    let mut html = format!(
        "<details class=\"viewer-switcher\"><summary aria-label=\"Current projection: {}. Open player switcher\"><span>Viewing</span><strong>{}</strong></summary><nav aria-label=\"Switch viewer projection\">",
        escape_html(&live.projection.viewer_display_name),
        escape_html(&live.projection.viewer_display_name),
    );
    if let Some(prefix) = &supplement.viewer_href_prefix {
        for member in &live.projection.members {
            let href = format!("{prefix}/{}", member.principal);
            let current = if member.principal == *viewer {
                " aria-current=\"page\""
            } else {
                ""
            };
            let _ = write!(
                html,
                "<a href=\"{}\"{current}>{} <small>{}</small></a>",
                escape_html(&href),
                escape_html(&member.display_name),
                member.role,
            );
        }
        let refresh = format!("{prefix}/{viewer}");
        let _ = write!(
            html,
            "<a href=\"{}\">Refresh projection</a>",
            escape_html(&refresh),
        );
    } else {
        for member in &live.projection.members {
            let _ = write!(
                html,
                "<span>{} <small>{}</small></span>",
                escape_html(&member.display_name),
                member.role,
            );
        }
    }
    if let Some(href) = &supplement.main_menu_href {
        let _ = write!(html, "<a href=\"{}\">Main menu</a>", escape_html(href));
    }
    html.push_str("</nav></details>");
    html
}

fn render_lobby_table(html: &mut String, live: &LiveClientPresentation, prefix: &str) {
    html.push_str("<section class=\"table-surface lobby-surface\" aria-labelledby=\"lobby-heading\"><div class=\"lobby-center\"><span>LOBBY</span><h2 id=\"lobby-heading\">Choose a seat</h2><p>Take a seat, mark ready, then the coordinator starts the countdown.</p><p class=\"connected-players\"><strong>At the table:</strong> ");
    for (index, member) in live.projection.members.iter().enumerate() {
        if index > 0 {
            html.push_str(", ");
        }
        let _ = write!(html, "{}", escape_html(&member.display_name));
    }
    html.push_str("</p></div><ol class=\"seat-ring\">");
    for seat in 0..2_u8 {
        let occupant = live
            .projection
            .members
            .iter()
            .find(|member| member.seat == Some(seat));
        let _ = write!(
            html,
            "<li class=\"seat seat-{seat}\"><span>Seat {}</span>",
            seat + 1
        );
        if let Some(member) = occupant {
            let _ = write!(
                html,
                "<strong>{}</strong><small>{}</small>",
                escape_html(&member.display_name),
                if member.ready { "ready" } else { "not ready" }
            );
        } else if let Some(control) = live.controls.iter().find(|control| {
            matches!(control.payload, CommandPayload::TakeSeat { seat: candidate } if candidate == seat)
        }) {
            let endpoint = format!("{prefix}/{}", control.id);
            let _ = write!(
                html,
                "<form method=\"post\" action=\"{}\"><button class=\"seat-action\" type=\"submit\" data-command-id=\"{}\">Sit here</button></form>",
                escape_html(&endpoint),
                escape_html(&control.id),
            );
        } else {
            html.push_str("<strong>Open</strong>");
        }
        html.push_str("</li>");
    }
    html.push_str("</ol>");
    if let Some(countdown) = live.projection.countdown {
        let _ = write!(
            html,
            "<div class=\"countdown-orb\"><strong>{}</strong><span>starting</span></div>",
            countdown.remaining()
        );
    }
    html.push_str("</section>");
}

fn render_score_sheet(html: &mut String, live: &LiveClientPresentation) {
    html.push_str("<section class=\"score-strip\" aria-labelledby=\"score-heading\"><h2 id=\"score-heading\">Players and scores</h2><ol>");
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
        let current_turn = table.is_some_and(|value| value.actor == format!("player {ordinal}"));
        let viewer = member.principal == live.projection.viewer;
        let _ = write!(
            html,
            "<li class=\"player-chip{}{}\" data-seat=\"{}\"><span>{}</span><strong>{score}</strong><small>{cards} cards · {tricks} tricks</small></li>",
            if current_turn { " current-turn" } else { "" },
            if viewer { " current-viewer" } else { "" },
            member.seat.unwrap_or_default(),
            escape_html(&member.display_name),
        );
    }
    html.push_str("</ol></section>");
}

fn render_turn_banner(html: &mut String, live: &LiveClientPresentation) {
    let Some(table) = &live.projection.table else {
        return;
    };
    let viewer_seat = live
        .projection
        .members
        .iter()
        .find(|member| member.principal == live.projection.viewer)
        .and_then(|member| member.seat);
    let viewer_turn = viewer_seat.is_some_and(|seat| table.actor == format!("player {seat}"));
    let actor = actor_name(live, &table.actor);
    let action = match table.phase {
        PublicGamePhase::AwaitingDeal => "waiting for the deal",
        PublicGamePhase::Bidding => "choose a bid",
        PublicGamePhase::Playing => "play a card",
        PublicGamePhase::Scoring => "score the round",
        PublicGamePhase::Finished => "review the finished game",
    };
    let headline = if viewer_turn {
        format!("Your turn — {action}")
    } else if matches!(
        table.phase,
        PublicGamePhase::Bidding | PublicGamePhase::Playing
    ) {
        format!("Waiting for {actor} to {action}")
    } else {
        action.to_owned()
    };
    let _ = write!(
        html,
        "<section class=\"turn-banner{}\" data-viewer-turn=\"{}\" aria-labelledby=\"turn-heading\"><span>{:?} · round {}</span><h2 id=\"turn-heading\">{}</h2><p>Pot ${:.2} · trump {}</p></section>",
        if viewer_turn { " your-turn" } else { "" },
        viewer_turn,
        table.phase,
        table.round_index.saturating_add(1),
        escape_html(&headline),
        f64::from(table.pot_cents) / 100.0,
        table.trump.as_deref().unwrap_or("—"),
    );
}

fn actor_name(live: &LiveClientPresentation, actor: &str) -> String {
    let Some(seat) = actor
        .strip_prefix("player ")
        .and_then(|value| value.parse::<u8>().ok())
    else {
        return actor.to_owned();
    };
    live.projection
        .members
        .iter()
        .find(|member| member.seat == Some(seat))
        .map_or_else(|| actor.to_owned(), |member| member.display_name.clone())
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
    html.push_str("<section class=\"table-surface\" aria-labelledby=\"table-heading\"><h2 id=\"table-heading\">Table</h2><div class=\"deck-zone\" aria-label=\"Deck\">");
    let _ = write!(
        html,
        "<div class=\"deck-stack\" aria-label=\"{deck_count} opaque cards remain\"><span>{deck_count}</span></div><small>deck</small></div><div class=\"trick-zone\" id=\"play-target\" tabindex=\"0\" aria-label=\"Current trick play target\"><span>PLAY</span><ol>"
    );
    if let Some(table) = &live.projection.table {
        for (seat, card) in &table.trick {
            let _ = write!(
                html,
                "<li><span class=\"playing-card table-card\"><strong>{}</strong><small>seat {seat}</small></span></li>",
                escape_html(card)
            );
        }
    }
    html.push_str("</ol></div><div class=\"trump-zone\" aria-label=\"Trump card\">");
    if let Some(trump) = live
        .projection
        .table
        .as_ref()
        .and_then(|table| table.trump.as_deref())
    {
        let _ = write!(
            html,
            "<span class=\"playing-card trump-card\"><strong>{}</strong></span>",
            escape_html(trump)
        );
    } else {
        html.push_str("<span class=\"playing-card card-back\">?</span>");
    }
    html.push_str("<small>trump</small></div></section>");

    if let Some(hand) = &live.projection.own_hand {
        let _ = write!(
            html,
            "<section class=\"hand-dock\" aria-labelledby=\"own-hand-heading\"><div><h2 id=\"own-hand-heading\">{}'s hand</h2><p id=\"hand-help\">Legal cards lift forward. Click or drag one to PLAY.</p></div><ul class=\"hand\">",
            escape_html(&live.projection.viewer_display_name)
        );
        for (index, (code, label)) in hand.card_codes.iter().zip(&hand.cards).enumerate() {
            let offset = i32::try_from(index).unwrap_or_default() * 2
                - (i32::try_from(hand.cards.len()).unwrap_or_default() - 1);
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
                    "<li style=\"--card-offset:{offset}\"><form method=\"post\" action=\"{}\"><button class=\"playing-card legal-card\" type=\"submit\" draggable=\"true\" data-card-code=\"{}\" data-command-id=\"{}\" aria-describedby=\"hand-help\"><strong>{}</strong><small>play</small></button></form></li>",
                    escape_html(&endpoint),
                    code,
                    escape_html(&control.id),
                    escape_html(label)
                );
            } else {
                let _ = write!(
                    html,
                    "<li style=\"--card-offset:{offset}\"><span class=\"playing-card held-card\" aria-label=\"{}; not currently legal\"><strong>{}</strong></span></li>",
                    escape_html(label),
                    escape_html(label)
                );
            }
        }
        html.push_str("</ul></section>");
    }
    for hand in &live.projection.granted_hands {
        let _ = write!(
            html,
            "<details class=\"granted-hand\"><summary>Granted hand — {}</summary><ul>",
            escape_html(&hand.player)
        );
        for card in &hand.cards {
            let _ = write!(html, "<li>{}</li>", escape_html(card));
        }
        html.push_str("</ul></details>");
    }
}

fn render_controls(html: &mut String, live: &LiveClientPresentation, prefix: &str) {
    let primary_controls = live
        .controls
        .iter()
        .filter(|control| {
            matches!(
                control.payload,
                CommandPayload::Ready
                    | CommandPayload::Unready
                    | CommandPayload::ArmCountdown { .. }
                    | CommandPayload::AbortCountdown
                    | CommandPayload::Pause
                    | CommandPayload::Unpause
                    | CommandPayload::ResetLobby
                    | CommandPayload::Reconnect
                    | CommandPayload::GameAction { .. }
            ) && !matches!(
                control.payload,
                CommandPayload::GameAction {
                    action: GameActionWire::Play { .. }
                }
            )
        })
        .collect::<Vec<_>>();
    if !primary_controls.is_empty() {
        html.push_str("<section class=\"action-dock\" aria-labelledby=\"turn-actions-heading\"><h2 id=\"turn-actions-heading\">Choose an action</h2><div class=\"actions\">");
        for control in primary_controls {
            let endpoint = format!("{prefix}/{}", control.id);
            let _ = write!(
                html,
                "<form method=\"post\" action=\"{}\"><button class=\"primary-action\" type=\"submit\" data-command-id=\"{}\">{}</button></form>",
                escape_html(&endpoint),
                escape_html(&control.id),
                escape_html(&control.label)
            );
        }
        html.push_str("</div></section>");
    }

    html.push_str("<nav class=\"utility-bar\" aria-label=\"Room commands\">");
    for control in &live.controls {
        if matches!(
            control.payload,
            CommandPayload::Ready
                | CommandPayload::Unready
                | CommandPayload::ArmCountdown { .. }
                | CommandPayload::AbortCountdown
                | CommandPayload::Pause
                | CommandPayload::Unpause
                | CommandPayload::ResetLobby
                | CommandPayload::Reconnect
                | CommandPayload::TakeSeat { .. }
                | CommandPayload::GameAction { .. }
        ) {
            continue;
        }
        let endpoint = format!("{prefix}/{}", control.id);
        let _ = write!(
            html,
            "<form method=\"post\" action=\"{}\"><button class=\"utility-action\" type=\"submit\" data-command-id=\"{}\">{}</button></form>",
            escape_html(&endpoint),
            escape_html(&control.id),
            escape_html(&control.label)
        );
    }
    html.push_str("</nav>");
}

fn render_history_and_chat(html: &mut String, live: &LiveClientPresentation) {
    let _ = write!(
        html,
        "<details class=\"inspector-panel\"><summary>Activity <span>{}</span></summary><section aria-labelledby=\"history-heading\"><h2 id=\"history-heading\">Public history</h2><ol class=\"event-log\">",
        live.projection.history.len(),
    );
    for event in &live.projection.history {
        let _ = write!(html, "<li>{}</li>", escape_html(event));
    }
    html.push_str("</ol></section><section aria-labelledby=\"chat-heading\"><h2 id=\"chat-heading\">Room chat</h2><ol class=\"event-log\">");
    for message in &live.projection.chat {
        let _ = write!(
            html,
            "<li><strong>{}</strong>: {}</li>",
            escape_html(&message.principal),
            escape_html(&message.text)
        );
    }
    if live.projection.chat.is_empty() {
        html.push_str("<li>No chat yet.</li>");
    }
    html.push_str("</ol></section>");
    if !live.projection.notices.is_empty() {
        html.push_str("<section aria-labelledby=\"client-events-heading\"><h2 id=\"client-events-heading\">This device</h2><ol class=\"event-log\">");
        for notice in &live.projection.notices {
            let _ = write!(
                html,
                "<li><strong>{}</strong> {}</li>",
                escape_html(&notice.reason_code),
                escape_html(&notice.message),
            );
        }
        html.push_str("</ol></section>");
    }
    html.push_str("</details>");
}

fn render_governance(
    html: &mut String,
    live: &LiveClientPresentation,
    supplement: &TabletopHtmlSupplement,
    prefix: &str,
) {
    let evidence_count = supplement.findings.len() + supplement.proposals.len();
    let _ = write!(
        html,
        "<details class=\"inspector-panel\"><summary>Rules and votes <span>{evidence_count}</span></summary><section aria-labelledby=\"findings-heading\"><h2 id=\"findings-heading\">Rule findings</h2><ul>"
    );
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
    html.push_str("</section></details>");
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
                room_invites: Vec::new(),
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
        let supplement = TabletopHtmlSupplement {
            viewer_href_prefix: Some("/tabletop".to_owned()),
            ..TabletopHtmlSupplement::default()
        };
        let html = render_tabletop_semantic_html(
            &live,
            &fixture.scene,
            "tabletop",
            "/tabletop/action",
            &supplement,
        )
        .expect("HTML");
        let hash = spatial_scene_hash_hex(&fixture.scene).expect("hash");
        assert!(html.contains(&format!("data-scene-hash=\"{hash}\"")));
        assert!(html.contains("Players and scores"));
        assert!(html.contains("class=\"game-shell\""));
        assert!(html.contains("class=\"table-surface\""));
        assert!(html.contains("Open player switcher"));
        assert!(html.contains("href=\"/tabletop/alice\""));
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
