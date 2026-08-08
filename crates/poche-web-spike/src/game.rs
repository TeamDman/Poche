// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Player-facing in-memory rooms over the same typed live reducer as the labs.

use std::collections::BTreeMap;

use poche_spatial::{LayoutId, SPATIAL_SCHEMA_VERSION, SpatialScene, TableId, registered_layout};
use poche_ui::{LiveClientPresentation, TabletopHtmlSupplement, realize_presentation_spatial};

use crate::demo::LiveDemo;

const ROOM_RANDOM_BYTES: usize = 10;
const SESSION_RANDOM_BYTES: usize = 16;
const PRINCIPAL_RANDOM_BYTES: usize = 8;
const MAX_NAME_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlayerSession {
    room_code: String,
    principal: String,
}

struct BrowserRoom {
    demo: LiveDemo,
}

/// Exact player-facing room projection for one unguessable browser session.
pub struct BrowserRoomView {
    pub live: LiveClientPresentation,
    pub scene: SpatialScene,
    pub supplement: TabletopHtmlSupplement,
}

/// Process-local room/session registry used by the web vertical slice.
#[derive(Default)]
pub struct BrowserRooms {
    rooms: BTreeMap<String, BrowserRoom>,
    sessions: BTreeMap<String, PlayerSession>,
}

impl BrowserRooms {
    /// Create a new room and admit its creator under an independent principal.
    pub fn create(&mut self, requested_name: &str) -> Result<String, String> {
        let display_name = validate_name(requested_name)?;
        let room_code = self.unique_room_code()?;
        let session_token = self.unique_session_token()?;
        let principal = self.unique_principal()?;
        let mut demo =
            LiveDemo::dynamic(&format!("player-room-{}", &compact_code(&room_code)[3..11]))?;
        demo.register_peer(&principal, &display_name)?;
        demo.create_room_as(&principal)?;
        self.rooms.insert(room_code.clone(), BrowserRoom { demo });
        self.sessions.insert(
            session_token.clone(),
            PlayerSession {
                room_code,
                principal,
            },
        );
        Ok(session_token)
    }

    /// Redeem a shared room code as a newly generated principal and device session.
    pub fn join(&mut self, requested_name: &str, requested_code: &str) -> Result<String, String> {
        let display_name = validate_name(requested_name)?;
        let room_code = canonical_room_code(requested_code)?;
        if !self.rooms.contains_key(&room_code) {
            return Err("No active room matches that code.".to_owned());
        }
        let session_token = self.unique_session_token()?;
        let principal = self.unique_principal()?;
        let invite = format!("invite-{}", random_hex::<SESSION_RANDOM_BYTES>()?);
        let room = self
            .rooms
            .get_mut(&room_code)
            .ok_or_else(|| "Room disappeared before the join completed.".to_owned())?;
        room.demo.register_peer(&principal, &display_name)?;
        room.demo.join_room_as(&principal, &invite)?;
        self.sessions.insert(
            session_token.clone(),
            PlayerSession {
                room_code,
                principal,
            },
        );
        Ok(session_token)
    }

    /// Resolve one retained opaque control and cross the typed reducer boundary.
    pub fn command(&mut self, session_token: &str, control: &str) -> Result<String, String> {
        let session = self.session(session_token)?.clone();
        self.rooms
            .get_mut(&session.room_code)
            .ok_or_else(|| "This room is no longer active.".to_owned())?
            .demo
            .control(&session.principal, control)
    }

    /// Simulate transport loss for the current device without changing membership.
    pub fn disconnect(&mut self, session_token: &str) -> Result<String, String> {
        let session = self.session(session_token)?.clone();
        self.rooms
            .get_mut(&session.room_code)
            .ok_or_else(|| "This room is no longer active.".to_owned())?
            .demo
            .disconnect(&session.principal)
    }

    /// Render-neutral exact-recipient state plus the registered two-seat layout.
    pub fn view(&self, session_token: &str) -> Result<BrowserRoomView, String> {
        let session = self.session(session_token)?;
        let room = self
            .rooms
            .get(&session.room_code)
            .ok_or_else(|| "This room is no longer active.".to_owned())?;
        let live = room.demo.view(&session.principal)?;
        let layout = registered_layout(
            TableId::new(0x504f_4348_4557_4542),
            LayoutId::new(2, 1).ok_or_else(|| "two-player layout is unavailable".to_owned())?,
        )
        .map_err(debug_error)?;
        let scene = if live.projection.table.is_none() {
            SpatialScene {
                schema_version: SPATIAL_SCHEMA_VERSION,
                table_id: layout.table_id(),
                layout: layout.id(),
                projection_epoch: 1,
                objects: Vec::new(),
                cards: Vec::new(),
                text: Vec::new(),
            }
        } else {
            realize_presentation_spatial(&layout, 1, &live.projection).map_err(debug_error)?
        };
        Ok(BrowserRoomView {
            supplement: TabletopHtmlSupplement {
                status: live
                    .projection
                    .notices
                    .last()
                    .map(|notice| format!("{} · {}", notice.reason_code, notice.message)),
                room_code: Some(session.room_code.clone()),
                main_menu_href: Some("/".to_owned()),
                viewer_href_prefix: None,
                findings: Vec::new(),
                proposals: Vec::new(),
            },
            live,
            scene,
        })
    }

    /// Advance every active countdown by one logical second.
    pub fn tick_countdowns(&mut self) -> Result<bool, String> {
        let mut changed = false;
        for room in self.rooms.values_mut() {
            changed |= room.demo.tick_countdown()?;
        }
        Ok(changed)
    }

    fn session(&self, token: &str) -> Result<&PlayerSession, String> {
        self.sessions
            .get(token)
            .ok_or_else(|| "This player session is unknown or expired.".to_owned())
    }

    fn unique_room_code(&self) -> Result<String, String> {
        for _ in 0..8 {
            let compact = format!(
                "PCH{}",
                random_hex::<ROOM_RANDOM_BYTES>()?.to_ascii_uppercase()
            );
            let code = grouped_code(&compact);
            if !self.rooms.contains_key(&code) {
                return Ok(code);
            }
        }
        Err("Could not allocate a unique room code.".to_owned())
    }

    fn unique_session_token(&self) -> Result<String, String> {
        for _ in 0..8 {
            let token = random_hex::<SESSION_RANDOM_BYTES>()?;
            if !self.sessions.contains_key(&token) {
                return Ok(token);
            }
        }
        Err("Could not allocate a unique player session.".to_owned())
    }

    fn unique_principal(&self) -> Result<String, String> {
        for _ in 0..8 {
            let principal = format!("player-{}", random_hex::<PRINCIPAL_RANDOM_BYTES>()?);
            if self
                .sessions
                .values()
                .all(|session| session.principal != principal)
            {
                return Ok(principal);
            }
        }
        Err("Could not allocate a unique player principal.".to_owned())
    }
}

fn validate_name(requested: &str) -> Result<String, String> {
    let name = requested.trim();
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.chars().any(char::is_control) {
        return Err("Choose a name between 1 and 32 bytes without control characters.".to_owned());
    }
    Ok(name.to_owned())
}

fn canonical_room_code(requested: &str) -> Result<String, String> {
    let compact = compact_code(requested);
    if compact.len() != 23
        || !compact.starts_with("PCH")
        || !compact[3..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Enter a PCH room code in the form shown by the creator.".to_owned());
    }
    Ok(grouped_code(&compact))
}

fn compact_code(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .flat_map(char::to_uppercase)
        .collect()
}

fn grouped_code(compact: &str) -> String {
    let payload = compact.strip_prefix("PCH").unwrap_or(compact);
    let groups = payload
        .as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join("-");
    format!("PCH-{groups}")
}

fn random_hex<const N: usize>() -> Result<String, String> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| "Operating-system randomness is unavailable.".to_owned())?;
    let mut output = String::with_capacity(N * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    Ok(output)
}

fn debug_error(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use super::BrowserRooms;

    #[test]
    fn arbitrary_named_tabs_share_one_opaque_room_without_identity_bound_codes() {
        let mut rooms = BrowserRooms::default();
        let teamy = rooms.create("Teamy").expect("create room");
        let creator = rooms.view(&teamy).expect("creator view");
        let code = creator.supplement.room_code.clone().expect("room code");
        assert!(code.starts_with("PCH-"));
        assert_eq!(code.len(), 28);
        assert!(!code.to_ascii_lowercase().contains("teamy"));
        assert_eq!(creator.live.projection.viewer_display_name, "Teamy");
        assert!(creator.live.command("take-seat-0").is_some());

        let friend = rooms.join("A Friend", &code).expect("join room");
        let joined = rooms.view(&friend).expect("joined view");
        assert_eq!(joined.live.projection.viewer_display_name, "A Friend");
        assert_eq!(joined.live.projection.members.len(), 2);
        assert!(joined.live.command("take-seat-1").is_some());
        assert_ne!(teamy, friend);
    }

    #[test]
    fn unknown_room_and_invalid_names_fail_before_membership() {
        let mut rooms = BrowserRooms::default();
        assert!(rooms.create("   ").is_err());
        assert!(rooms.join("Bob", "PCH-0000-0000-0000-0000-0000").is_err());
    }

    #[test]
    fn controls_presented_to_two_tabs_drive_the_room_from_lobby_to_gameplay() {
        let mut rooms = BrowserRooms::default();
        let first = rooms.create("First player").expect("create room");
        let code = rooms
            .view(&first)
            .expect("creator view")
            .supplement
            .room_code
            .expect("room code");
        let second = rooms.join("Second player", &code).expect("join room");

        rooms
            .command(&first, "take-seat-0")
            .expect("creator takes seat");
        rooms
            .command(&second, "take-seat-1")
            .expect("joiner takes seat");
        rooms.command(&first, "ready").expect("creator ready");
        rooms.command(&second, "ready").expect("joiner ready");

        let creator_ready = rooms.view(&first).expect("ready creator view");
        assert!(creator_ready.live.command("arm-countdown").is_some());
        rooms
            .command(&first, "arm-countdown")
            .expect("start countdown");
        for _ in 0..3 {
            assert!(rooms.tick_countdowns().expect("countdown tick"));
        }

        let first_running = rooms.view(&first).expect("first running view");
        let second_running = rooms.view(&second).expect("second running view");
        assert_eq!(
            first_running.live.projection.room_phase,
            poche_protocol::RoomPhase::Running
        );
        assert_eq!(
            second_running.live.projection.room_phase,
            poche_protocol::RoomPhase::Running
        );
        assert!(first_running.live.projection.table.is_some());
        assert!(second_running.live.projection.table.is_some());
        let first_can_act = first_running.live.controls.iter().any(|control| {
            matches!(
                control.payload,
                poche_protocol::CommandPayload::GameAction { .. }
            )
        });
        let second_can_act = second_running.live.controls.iter().any(|control| {
            matches!(
                control.payload,
                poche_protocol::CommandPayload::GameAction { .. }
            )
        });
        assert_ne!(first_can_act, second_can_act);
    }
}
