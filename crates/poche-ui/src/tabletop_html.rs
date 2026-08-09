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
    /// Adapter-owned recoverable transport exit; membership is retained.
    pub exit_endpoint: Option<String>,
    /// Adapter-owned endpoint accepting a `text` form field for typed chat.
    pub chat_endpoint: Option<String>,
    /// Whether this adapter implements the governance sidecar command IDs.
    pub governance_commands: bool,
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
    html.push_str("<div class=\"game-layout\"><div class=\"tabletop-primary\">");
    if matches!(
        live.projection.room_phase,
        RoomPhase::Lobby | RoomPhase::Countdown
    ) {
        render_lobby_table(&mut html, live, prefix, supplement, root_id);
    } else {
        render_card_zones(&mut html, live, scene, prefix, supplement, root_id);
    }
    render_turn_banner(&mut html, live);
    render_controls(&mut html, live, prefix, supplement, root_id);
    html.push_str("</div><aside class=\"game-inspector\" aria-label=\"Game inspector\">");
    render_activity(&mut html, live, supplement);
    render_score_sheet(&mut html, live, root_id);
    render_chat(&mut html, live, supplement, root_id);
    render_governance(&mut html, live, supplement, prefix, root_id);
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

fn render_lobby_table(
    html: &mut String,
    live: &LiveClientPresentation,
    prefix: &str,
    supplement: &TabletopHtmlSupplement,
    root_id: &str,
) {
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
            "<li class=\"seat seat-{seat}\" data-layout-footprint=\"seat-{seat}\"><span>Seat {}</span>",
            seat + 1
        );
        if let Some(member) = occupant {
            let _ = write!(
                html,
                "<strong>{}</strong><small>{}</small>",
                escape_html(&member.display_name),
                if member.ready { "ready" } else { "not ready" }
            );
            if member.principal == live.projection.viewer
                && let Some(control) = live.controls.iter().find(|control| {
                    matches!(control.payload, CommandPayload::Ready | CommandPayload::Unready)
                })
            {
                render_control(html, control, prefix, "seat-action ready-action", None);
            }
            if member.principal == live.projection.viewer
                && let Some(control) = live
                    .controls
                    .iter()
                    .find(|control| matches!(control.payload, CommandPayload::ReleaseSeat))
            {
                render_control(html, control, prefix, "seat-action stand-action", None);
            }
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
    if let Some(control) = live
        .controls
        .iter()
        .find(|control| matches!(control.payload, CommandPayload::ArmCountdown { .. }))
    {
        html.push_str("<div class=\"lobby-start\" data-layout-footprint=\"countdown-control\">");
        render_control(html, control, prefix, "seat-action", None);
        html.push_str("</div>");
    }
    if let Some(countdown) = live.projection.countdown {
        let _ = write!(
            html,
            "<div class=\"countdown-orb\" data-layout-footprint=\"countdown\"><strong>{}</strong><span>starting</span>",
            countdown.remaining()
        );
        if let Some(control) = live
            .controls
            .iter()
            .find(|control| matches!(control.payload, CommandPayload::AbortCountdown))
        {
            render_control(html, control, prefix, "countdown-action", None);
        }
        html.push_str("</div>");
    }
    render_table_props(html, live, prefix, supplement, root_id);
    html.push_str("</section>");
}

fn render_score_sheet(html: &mut String, live: &LiveClientPresentation, root_id: &str) {
    let score_sheet_id = format!("{root_id}-score-sheet");
    let completed_rounds = live.projection.round_scores.len();
    let _ = write!(
        html,
        "<details id=\"{}\" class=\"inspector-panel score-sheet-panel\"><summary>Score sheet <span>{completed_rounds}</span></summary>",
        escape_html(&score_sheet_id),
    );
    let Some(table) = &live.projection.table else {
        html.push_str("<section><p>The paper-style score sheet appears after the game is dealt.</p></section></details>");
        return;
    };

    let mut players = live
        .projection
        .members
        .iter()
        .filter(|member| member.seat.is_some())
        .collect::<Vec<_>>();
    players.sort_by_key(|member| member.seat);
    let player_count = players.len();
    html.push_str("<section><h2>Visible table score sheet</h2><div class=\"score-sheet-scroll\"><table class=\"score-sheet\"><caption>Each completed round retains its score-cell notation; the active row shows bids until scoring.</caption><thead><tr><th scope=\"col\">Round</th><th scope=\"col\">Dealer</th><th scope=\"col\">Cards</th>");
    for player in &players {
        let _ = write!(
            html,
            "<th scope=\"col\">{}</th>",
            escape_html(&player.display_name)
        );
    }
    html.push_str("</tr></thead><tbody>");

    let first_dealer = table.dealer.and_then(|dealer| {
        let count = u8::try_from(player_count).ok()?;
        if count == 0 {
            return None;
        }
        let offset = u8::try_from(table.round_index % u16::from(count)).ok()?;
        Some((dealer + count - offset) % count)
    });
    for (round_index, scores) in live.projection.round_scores.iter().enumerate() {
        render_score_sheet_row(
            html,
            round_index,
            first_dealer,
            &players,
            scheduled_hand_size(round_index, player_count),
            scores.iter().copied().map(Some),
            false,
        );
    }
    if table.phase != PublicGamePhase::Finished {
        render_score_sheet_row(
            html,
            usize::from(table.round_index),
            first_dealer,
            &players,
            usize::from(table.hand_size),
            table.bids.iter().map(|bid| bid.map(i32::from)),
            true,
        );
    }
    html.push_str("</tbody><tfoot><tr><th scope=\"row\" colspan=\"3\">Total</th>");
    for player in &players {
        let score = player
            .seat
            .and_then(|seat| table.scores.get(usize::from(seat)))
            .copied()
            .unwrap_or_default();
        let _ = write!(html, "<td>{score}</td>");
    }
    html.push_str("</tr></tfoot></table></div><p class=\"score-legend\"><strong>Notation:</strong> ● failed bid; 1<var>n</var> met an ordinary bid; 2<var>n</var> took every trick.</p></section></details>");
}

fn render_score_sheet_row<I>(
    html: &mut String,
    round_index: usize,
    first_dealer: Option<u8>,
    players: &[&crate::MemberPresentation],
    hand_size: usize,
    cells: I,
    active: bool,
) where
    I: IntoIterator<Item = Option<i32>>,
{
    let dealer = first_dealer.and_then(|first| {
        let count = u8::try_from(players.len()).ok()?;
        if count == 0 {
            return None;
        }
        let offset = u8::try_from(round_index % usize::from(count)).ok()?;
        Some((first + offset) % count)
    });
    let dealer_name = dealer
        .and_then(|seat| players.iter().find(|member| member.seat == Some(seat)))
        .map_or_else(|| "—".to_owned(), |member| member.display_name.clone());
    let _ = write!(
        html,
        "<tr{}><th scope=\"row\">{}</th><td>{}</td><td>{hand_size}</td>",
        if active {
            " class=\"active-round\""
        } else {
            ""
        },
        round_index.saturating_add(1),
        escape_html(&dealer_name),
    );
    for cell in cells.into_iter().take(players.len()) {
        match cell {
            Some(0) if !active => {
                html.push_str("<td><span aria-label=\"failed bid; zero points\">●</span></td>");
            }
            Some(value) => {
                let _ = write!(html, "<td>{value}</td>");
            }
            None => html.push_str("<td>—</td>"),
        }
    }
    html.push_str("</tr>");
}

fn scheduled_hand_size(round_index: usize, players: usize) -> usize {
    if players == 0 {
        return 0;
    }
    let maximum = (51 / players).min(7);
    if round_index < maximum {
        round_index + 1
    } else {
        (maximum.saturating_mul(2))
            .saturating_sub(round_index)
            .saturating_sub(1)
    }
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
    supplement: &TabletopHtmlSupplement,
    root_id: &str,
) {
    let deck_count = scene
        .cards
        .iter()
        .filter(|card| matches!(card.location, CardLocation::Deck { .. }))
        .count();
    let table = live.projection.table.as_ref();
    let mut players = live
        .projection
        .members
        .iter()
        .filter(|member| member.seat.is_some())
        .collect::<Vec<_>>();
    players.sort_by_key(|member| member.seat);
    if let Some(viewer_index) = players
        .iter()
        .position(|member| member.principal == live.projection.viewer)
    {
        players.rotate_left(viewer_index);
    }
    let actor_seat = table.and_then(|value| seat_from_actor(&value.actor));

    html.push_str("<section class=\"table-surface round-table\" aria-labelledby=\"table-heading\"><h2 id=\"table-heading\">Table and players</h2><div class=\"table-rim\" aria-hidden=\"true\"></div>");
    render_table_players(html, live, table, &players, actor_seat);
    render_actor_cue(html, live, table, &players, actor_seat, prefix);
    render_deck_and_trick(html, table, &players, deck_count);
    render_private_hand(html, live, prefix);
    render_table_props(html, live, prefix, supplement, root_id);
    html.push_str("</section>");
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

fn render_table_players(
    html: &mut String,
    live: &LiveClientPresentation,
    table: Option<&crate::TablePresentation>,
    players: &[&crate::MemberPresentation],
    actor_seat: Option<u8>,
) {
    let player_count = players.len();
    for (orbit_index, member) in players.iter().enumerate() {
        let (left, top) = orbit_position(orbit_index, player_count);
        let seat = member.seat.unwrap_or_default();
        let cards = table
            .and_then(|value| value.hand_counts.get(usize::from(seat)))
            .copied()
            .unwrap_or_default();
        let tricks = table
            .and_then(|value| value.tricks_won.get(usize::from(seat)))
            .copied()
            .unwrap_or_default();
        let score = table
            .and_then(|value| value.scores.get(usize::from(seat)))
            .copied()
            .unwrap_or_default();
        let viewer = member.principal == live.projection.viewer;
        let actor = actor_seat == Some(seat);
        let dealer = table.and_then(|value| value.dealer) == Some(seat);
        let _ = write!(
            html,
            "<article class=\"table-player{}{}{}\" style=\"--seat-left:{left:.2}%;--seat-top:{top:.2}%\" data-seat=\"{seat}\" data-layout-footprint=\"player-{seat}\"><span class=\"player-avatar\" aria-hidden=\"true\">{}</span><span class=\"player-identity\"><strong>{}{}</strong><small>{score} points · {cards} cards · {tricks} tricks</small></span>",
            if viewer { " current-viewer" } else { "" },
            if actor { " current-actor" } else { "" },
            if dealer { " dealer" } else { "" },
            escape_html(&avatar_initial(&member.display_name)),
            escape_html(&member.display_name),
            if viewer { " <em>you</em>" } else { "" },
        );
        if dealer {
            html.push_str("<span class=\"dealer-marker\">dealer</span>");
        }
        if !viewer {
            let _ = write!(
                html,
                "<span class=\"opponent-hand\" aria-label=\"{} holds {cards} cards\">",
                escape_html(&member.display_name)
            );
            for card_index in 0..usize::from(cards.min(5)) {
                let _ = write!(
                    html,
                    "<i style=\"--mini-card:{card_index}\" aria-hidden=\"true\"></i>"
                );
            }
            if cards > 5 {
                let _ = write!(html, "<b aria-hidden=\"true\">+{}</b>", cards - 5);
            }
            html.push_str("</span>");
        }
        html.push_str("</article>");
    }
}

fn render_actor_cue(
    html: &mut String,
    live: &LiveClientPresentation,
    table: Option<&crate::TablePresentation>,
    players: &[&crate::MemberPresentation],
    actor_seat: Option<u8>,
    prefix: &str,
) {
    if let (Some(table), Some(seat)) = (table, actor_seat)
        && let Some((orbit_index, _)) = players
            .iter()
            .enumerate()
            .find(|(_, member)| member.seat == Some(seat))
        && matches!(
            table.phase,
            PublicGamePhase::Bidding | PublicGamePhase::Playing
        )
    {
        let (player_left, player_top) = orbit_position(orbit_index, players.len());
        let (cue_left, cue_top) = if table.phase == PublicGamePhase::Bidding {
            (50.0, 38.0)
        } else {
            (
                f64::midpoint(player_left, 50.0) + if player_top > 50.0 { -12.0 } else { 12.0 },
                if player_top > 50.0 {
                    player_top - 10.0
                } else {
                    player_top + 10.0
                },
            )
        };
        let cue = if table.phase == PublicGamePhase::Bidding {
            "Placing bid"
        } else {
            "Choosing card"
        };
        let viewer_is_actor = players
            .iter()
            .any(|member| member.seat == Some(seat) && member.principal == live.projection.viewer);
        if viewer_is_actor && table.phase == PublicGamePhase::Playing {
            // The actionable private hand is the viewer's diegetic choosing
            // surface; a second floating cue would only obscure those cards.
            return;
        }
        if viewer_is_actor && table.phase == PublicGamePhase::Bidding {
            let bids = live
                .controls
                .iter()
                .filter(|control| {
                    matches!(
                        control.payload,
                        CommandPayload::GameAction {
                            action: GameActionWire::Bid { .. }
                        }
                    )
                })
                .collect::<Vec<_>>();
            if !bids.is_empty() {
                let _ = write!(
                    html,
                    "<div class=\"actor-cue bid-console\" data-layout-footprint=\"actor-cue\" style=\"--cue-left:{cue_left:.2}%;--cue-top:{cue_top:.2}%\" role=\"group\" aria-label=\"Place your bid at the table\"><span>Say your bid</span><div>"
                );
                for control in bids {
                    render_control(html, control, prefix, "diegetic-bid", None);
                }
                html.push_str("</div></div>");
                return;
            }
        }
        let _ = write!(
            html,
            "<div class=\"actor-cue\" data-layout-footprint=\"actor-cue\" style=\"--cue-left:{cue_left:.2}%;--cue-top:{cue_top:.2}%\" role=\"status\" aria-label=\"{}\"><span aria-hidden=\"true\">",
            escape_html(cue)
        );
        for (index, character) in cue.chars().enumerate() {
            let _ = write!(
                html,
                "<i style=\"--wave-index:{index}\">{}</i>",
                if character == ' ' {
                    "&nbsp;".to_owned()
                } else {
                    escape_html(&character.to_string())
                }
            );
        }
        html.push_str("</span></div>");
    }
}

fn render_table_props(
    html: &mut String,
    live: &LiveClientPresentation,
    prefix: &str,
    supplement: &TabletopHtmlSupplement,
    root_id: &str,
) {
    let score_sheet_id = format!("{root_id}-score-sheet");
    let governance_id = format!("{root_id}-governance");
    let chat_id = format!("{root_id}-chat");
    let _ = write!(
        html,
        "<div class=\"table-props\" aria-label=\"Objects on the table\"><button type=\"button\" class=\"table-paper score-paper\" data-open-details=\"{}\" data-layout-footprint=\"score-sheet\">Scores</button><button type=\"button\" class=\"table-paper rules-paper\" data-open-details=\"{}\" data-layout-footprint=\"rules\">Rules</button>",
        escape_html(&score_sheet_id),
        escape_html(&governance_id),
    );
    if supplement.chat_endpoint.is_some() {
        let _ = write!(
            html,
            "<button type=\"button\" class=\"table-chat\" data-open-details=\"{}\" data-layout-footprint=\"chat\" aria-label=\"Open table chat\">Chat</button>",
            escape_html(&chat_id),
        );
    }
    if let Some(endpoint) = &supplement.exit_endpoint {
        let _ = write!(
            html,
            "<button type=\"button\" class=\"table-door\" data-exit-table=\"{}\" data-layout-footprint=\"door\"><span aria-hidden=\"true\">↪</span> Exit</button>",
            escape_html(endpoint),
        );
    }
    if let Some(control) = live.controls.iter().find(|control| {
        matches!(
            control.payload,
            CommandPayload::Pause | CommandPayload::Unpause | CommandPayload::ResetLobby
        )
    }) {
        render_diegetic_control(html, control, prefix, "table-clock", "table-clock", None);
    }
    if let Some(control) = live.controls.iter().find(|control| {
        matches!(
            control.payload,
            CommandPayload::Leave | CommandPayload::CloseRoom
        )
    }) {
        let confirmation = match control.payload {
            CommandPayload::CloseRoom => "Close this room for everyone? This cannot be undone.",
            CommandPayload::Leave => {
                "Permanently leave this membership? Exit the table instead if you want to return later."
            }
            _ => unreachable!("filtered room control"),
        };
        render_diegetic_control(
            html,
            control,
            prefix,
            "table-room-danger",
            "room-danger",
            Some(confirmation),
        );
    }
    html.push_str("</div>");
}

fn render_diegetic_control(
    html: &mut String,
    control: &crate::TypedUiControl,
    prefix: &str,
    class_name: &str,
    footprint: &str,
    confirmation: Option<&str>,
) {
    let endpoint = format!("{prefix}/{}", control.id);
    let confirmation = confirmation.map_or_else(String::new, |message| {
        format!(" data-confirm=\"{}\"", escape_html(message))
    });
    let _ = write!(
        html,
        "<form method=\"post\" action=\"{}\"><button class=\"{}\" type=\"submit\" data-command-id=\"{}\" data-layout-footprint=\"{}\"{}>{}</button></form>",
        escape_html(&endpoint),
        class_name,
        escape_html(&control.id),
        escape_html(footprint),
        confirmation,
        escape_html(&control.label),
    );
}

fn render_deck_and_trick(
    html: &mut String,
    table: Option<&crate::TablePresentation>,
    players: &[&crate::MemberPresentation],
    deck_count: usize,
) {
    let player_count = players.len();
    let (deck_left, deck_top) = table
        .and_then(|value| value.dealer)
        .and_then(|dealer| {
            players
                .iter()
                .position(|member| member.seat == Some(dealer))
        })
        .map_or((14.0, 50.0), |orbit_index| {
            let (left, top) = orbit_position(orbit_index, player_count);
            let inward = if top < 50.0 { 0.55 } else { 0.34 };
            (
                (left - 20.0).clamp(8.0, 84.0),
                (top + (50.0 - top) * inward).clamp(15.0, 82.0),
            )
        });
    let _ = write!(
        html,
        "<div class=\"deck-zone\" data-layout-footprint=\"deck\" style=\"--deck-left:{deck_left:.2}%;--deck-top:{deck_top:.2}%\" aria-label=\"Deck near the dealer\">"
    );
    let _ = write!(
        html,
        "<div class=\"deck-stack\" aria-label=\"{deck_count} opaque cards remain\"><span>{deck_count}</span></div><small>deck</small><div class=\"trump-zone\" aria-label=\"Trump card\">"
    );
    if let Some(trump) = table.and_then(|value| value.trump.as_deref()) {
        let _ = write!(
            html,
            "<span class=\"playing-card trump-card\"><strong>{}</strong></span>",
            escape_html(trump)
        );
    } else {
        html.push_str("<span class=\"playing-card card-back\">?</span>");
    }
    let bidding = table.is_some_and(|value| value.phase == PublicGamePhase::Bidding);
    let _ = write!(
        html,
        "<small>trump</small></div></div><div class=\"trick-zone{}\" id=\"play-target\"{} tabindex=\"0\" aria-label=\"Current trick play target\"><span>PLAY</span><ol>",
        if bidding { " bidding-stage" } else { "" },
        if bidding {
            ""
        } else {
            " data-layout-footprint=\"current-trick\""
        },
    );
    if let Some(table) = table {
        for (seat, card) in &table.trick {
            let _ = write!(
                html,
                "<li><span class=\"playing-card table-card\"><strong>{}</strong><small>seat {seat}</small></span></li>",
                escape_html(card)
            );
        }
    }
    html.push_str("</ol></div>");
}

fn render_private_hand(html: &mut String, live: &LiveClientPresentation, prefix: &str) {
    if let Some(hand) = &live.projection.own_hand {
        let _ = write!(
            html,
            "<section class=\"table-hand\" aria-labelledby=\"own-hand-heading\"><div class=\"hand-label\"><h3 id=\"own-hand-heading\">{}'s hand</h3><p id=\"hand-help\">Legal cards lift forward. Click or drag one to PLAY.</p></div><ul class=\"hand\">",
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
                    "<li style=\"--card-offset:{offset}\" data-layout-footprint=\"hand-card-{index}\"><form method=\"post\" action=\"{}\"><button class=\"playing-card legal-card\" type=\"submit\" draggable=\"true\" data-card-code=\"{}\" data-command-id=\"{}\" aria-describedby=\"hand-help\"><strong>{}</strong><small>play</small></button></form></li>",
                    escape_html(&endpoint),
                    code,
                    escape_html(&control.id),
                    escape_html(label)
                );
            } else {
                let _ = write!(
                    html,
                    "<li style=\"--card-offset:{offset}\" data-layout-footprint=\"hand-card-{index}\"><span class=\"playing-card held-card\" aria-label=\"{}; not currently legal\"><strong>{}</strong></span></li>",
                    escape_html(label),
                    escape_html(label)
                );
            }
        }
        html.push_str("</ul></section>");
    }
}

fn seat_from_actor(actor: &str) -> Option<u8> {
    actor
        .strip_prefix("player ")
        .and_then(|value| value.parse().ok())
}

fn orbit_position(index: usize, count: usize) -> (f64, f64) {
    if count == 0 {
        return (50.0, 50.0);
    }
    let (Ok(index), Ok(count)) = (u32::try_from(index), u32::try_from(count)) else {
        return (50.0, 50.0);
    };
    let angle = (90.0 + (f64::from(index) * 360.0 / f64::from(count))).to_radians();
    (50.0 + 41.0 * angle.cos(), 50.0 + 39.0 * angle.sin())
}

fn avatar_initial(name: &str) -> String {
    let mut initials = name
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect::<String>();
    if initials.is_empty() {
        initials.push('?');
    }
    initials
}

fn render_controls(
    html: &mut String,
    live: &LiveClientPresentation,
    prefix: &str,
    supplement: &TabletopHtmlSupplement,
    root_id: &str,
) {
    let primary_controls = live
        .controls
        .iter()
        .filter(|control| is_primary_control(&control.payload))
        .collect::<Vec<_>>();
    let room_controls = live
        .controls
        .iter()
        .filter(|control| is_dangerous_control(&control.payload))
        .collect::<Vec<_>>();
    let secondary_controls = live
        .controls
        .iter()
        .filter(|control| {
            !(is_primary_control(&control.payload)
                || is_dangerous_control(&control.payload)
                || supplement.chat_endpoint.is_some()
                    && matches!(control.payload, CommandPayload::Chat { .. }))
        })
        .collect::<Vec<_>>();
    let chat_id = format!("{root_id}-chat");
    let governance_id = format!("{root_id}-governance");
    let score_sheet_id = format!("{root_id}-score-sheet");

    html.push_str("<section class=\"action-dock\" aria-labelledby=\"turn-actions-heading\"><h2 id=\"turn-actions-heading\">Choose an action</h2><div class=\"command-groups\">");
    if !primary_controls.is_empty() {
        html.push_str("<section class=\"command-group primary-commands\"><h3>Available now</h3><div class=\"actions\">");
        for control in primary_controls {
            render_control(html, control, prefix, "primary-action", None);
        }
        html.push_str("</div></section>");
    }

    html.push_str(
        "<section class=\"command-group table-commands\"><h3>Table</h3><div class=\"actions\">",
    );
    if supplement.chat_endpoint.is_some() {
        let _ = write!(
            html,
            "<button class=\"secondary-action\" type=\"button\" data-open-details=\"{}\">Chat</button>",
            escape_html(&chat_id)
        );
    }
    let _ = write!(
        html,
        "<button class=\"secondary-action\" type=\"button\" data-open-details=\"{}\">Rules &amp; votes</button>",
        escape_html(&governance_id)
    );
    let _ = write!(
        html,
        "<button class=\"secondary-action\" type=\"button\" data-open-details=\"{}\">Score sheet</button>",
        escape_html(&score_sheet_id)
    );
    for control in secondary_controls {
        render_control(html, control, prefix, "secondary-action", None);
    }
    html.push_str("</div></section>");

    if !room_controls.is_empty() || supplement.exit_endpoint.is_some() {
        html.push_str(
            "<section class=\"command-group room-commands\"><h3>Room</h3><div class=\"actions\">",
        );
        if let Some(endpoint) = &supplement.exit_endpoint {
            let _ = write!(
                html,
                "<button class=\"secondary-action\" type=\"button\" data-exit-table=\"{}\">Exit table</button>",
                escape_html(endpoint),
            );
        }
        for control in room_controls {
            let confirmation = match control.payload {
                CommandPayload::CloseRoom => {
                    Some("Close this room for everyone? This cannot be undone.")
                }
                CommandPayload::Leave => Some(
                    "Permanently leave this membership? Use Exit table if you only want to return to the menu and come back later.",
                ),
                CommandPayload::RemoveMember { .. } => Some("Remove this person from the room?"),
                _ => None,
            };
            render_control(html, control, prefix, "danger-action", confirmation);
        }
        html.push_str("</div></section>");
    }
    html.push_str("</div></section>");
}

fn is_primary_control(payload: &CommandPayload) -> bool {
    matches!(
        payload,
        CommandPayload::TakeSeat { .. }
            | CommandPayload::ReleaseSeat
            | CommandPayload::Ready
            | CommandPayload::Unready
            | CommandPayload::ArmCountdown { .. }
            | CommandPayload::AbortCountdown
            | CommandPayload::Pause
            | CommandPayload::Unpause
            | CommandPayload::ResetLobby
            | CommandPayload::Reconnect
            | CommandPayload::GameAction { .. }
    )
}

fn is_dangerous_control(payload: &CommandPayload) -> bool {
    matches!(
        payload,
        CommandPayload::Leave | CommandPayload::CloseRoom | CommandPayload::RemoveMember { .. }
    )
}

fn render_control(
    html: &mut String,
    control: &crate::TypedUiControl,
    prefix: &str,
    class_name: &str,
    confirmation: Option<&str>,
) {
    let endpoint = format!("{prefix}/{}", control.id);
    let confirmation = confirmation.map_or_else(String::new, |message| {
        format!(" data-confirm=\"{}\"", escape_html(message))
    });
    let _ = write!(
        html,
        "<form method=\"post\" action=\"{}\"><button class=\"{}\" type=\"submit\" data-command-id=\"{}\"{}>{}</button></form>",
        escape_html(&endpoint),
        class_name,
        escape_html(&control.id),
        confirmation,
        escape_html(&control.label)
    );
}

fn render_activity(
    html: &mut String,
    live: &LiveClientPresentation,
    supplement: &TabletopHtmlSupplement,
) {
    let _ = write!(
        html,
        "<details class=\"inspector-panel activity-panel\"><summary>Activity <span>{}</span></summary>",
        live.projection.history.len(),
    );
    if let Some(status) = &supplement.status {
        let _ = write!(
            html,
            "<p class=\"latest-client-status\" role=\"status\" aria-live=\"polite\"><strong>Latest client result</strong><span>{}</span></p>",
            escape_html(status)
        );
    }
    html.push_str("<section aria-labelledby=\"history-heading\"><h2 id=\"history-heading\">Public history</h2><ol class=\"event-log\">");
    for event in &live.projection.history {
        let _ = write!(html, "<li>{}</li>", escape_html(event));
    }
    if live.projection.history.is_empty() {
        html.push_str("<li>No public actions yet.</li>");
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

fn render_chat(
    html: &mut String,
    live: &LiveClientPresentation,
    supplement: &TabletopHtmlSupplement,
    root_id: &str,
) {
    let chat_id = format!("{root_id}-chat");
    let status_id = format!("{root_id}-chat-status");
    let _ = write!(
        html,
        "<details id=\"{}\" class=\"inspector-panel chat-panel\"><summary>Chat <span>{}</span></summary><section aria-labelledby=\"chat-heading\"><h2 id=\"chat-heading\">Room chat</h2><ol class=\"chat-log\">",
        escape_html(&chat_id),
        live.projection.chat.len(),
    );
    for message in &live.projection.chat {
        let _ = write!(
            html,
            "<li><strong>{}</strong><span>{}</span></li>",
            escape_html(&message.principal),
            escape_html(&message.text)
        );
    }
    if live.projection.chat.is_empty() {
        html.push_str("<li class=\"empty-chat\">No messages yet.</li>");
    }
    html.push_str("</ol>");
    if let Some(endpoint) = &supplement.chat_endpoint {
        let _ = write!(
            html,
            "<form class=\"chat-composer\" method=\"post\" action=\"{}\" data-chat-form><label for=\"{}-message\">Message</label><div><input id=\"{}-message\" name=\"text\" type=\"text\" maxlength=\"2048\" autocomplete=\"off\" placeholder=\"Say something to the table\" required><button type=\"submit\">Send</button></div><output id=\"{}\" data-chat-status role=\"status\" aria-live=\"polite\"></output></form>",
            escape_html(endpoint),
            escape_html(root_id),
            escape_html(root_id),
            escape_html(&status_id),
        );
    }
    html.push_str("</section></details>");
}

fn render_governance(
    html: &mut String,
    live: &LiveClientPresentation,
    supplement: &TabletopHtmlSupplement,
    prefix: &str,
    root_id: &str,
) {
    let evidence_count = supplement.findings.len() + supplement.proposals.len();
    let governance_id = format!("{root_id}-governance");
    let _ = write!(
        html,
        "<details id=\"{}\" class=\"inspector-panel\"><summary>Rules and votes <span>{evidence_count}</span></summary><section aria-labelledby=\"findings-heading\"><h2 id=\"findings-heading\">Rule findings</h2><ul>",
        escape_html(&governance_id),
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
    let viewer_can_govern = supplement.governance_commands
        && live.projection.members.iter().any(|member| {
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
    use poche_protocol::{CommandPayload, GameActionWire};
    use poche_spatial::spatial_scene_hash_hex;

    use crate::{
        LiveClientInput, LiveClientPresentation, TabletopHtmlSupplement, embedded_spatial_fixture,
        render_tabletop_semantic_html,
    };

    #[test]
    fn semantic_tabletop_uses_shared_hash_and_never_emits_hidden_faces() {
        let fixture = embedded_spatial_fixture().expect("fixture");
        let live = LiveClientPresentation::from_input(
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
            status: Some("COMMAND · applied; revision 7; events 1".to_owned()),
            chat_endpoint: Some("/tabletop/chat".to_owned()),
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
        assert!(html.contains("Visible table score sheet"));
        assert!(html.contains("<th scope=\"col\">Round</th>"));
        assert!(html.contains("data-open-details=\"tabletop-score-sheet\""));
        assert!(html.contains("class=\"game-shell\""));
        assert!(html.contains("class=\"table-surface round-table\""));
        assert!(html.contains("class=\"table-player"));
        assert!(html.contains("class=\"opponent-hand\""));
        assert!(html.contains("class=\"table-hand\""));
        assert!(html.contains("class=\"dealer-marker\""));
        assert!(html.contains("Open player switcher"));
        assert!(html.contains("href=\"/tabletop/alice\""));
        assert!(html.contains("What state am I in?"));
        assert!(html.contains("Copy diagnostic context"));
        assert!(html.contains("data-chat-form"));
        assert!(html.contains("data-open-details=\"tabletop-chat\""));
        assert!(!html.contains("Leave membership permanently"));
        assert!(!html.contains("class=\"command-status\""));
        assert!(html.contains("Latest client result"));
        let table_position = html.find("class=\"table-surface").expect("table surface");
        let turn_position = html.find("class=\"turn-banner").expect("turn banner");
        let actions_position = html.find("class=\"action-dock").expect("action dock");
        assert!(table_position < turn_position && turn_position < actions_position);
        assert!(html.contains("draggable=\"true\"") || html.contains("not currently legal"));
        for control in live.controls.iter().filter(|control| {
            matches!(
                control.payload,
                CommandPayload::GameAction {
                    action: GameActionWire::Play { .. }
                }
            )
        }) {
            assert_eq!(
                html.matches(&format!("data-command-id=\"{}\"", control.id))
                    .count(),
                2,
                "playable card must appear in both the hand and command palette"
            );
            assert!(control.label.contains(['♣', '♦', '♥', '♠']));
        }
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
    }

    #[test]
    fn disconnected_viewer_cannot_receive_governance_controls() {
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
