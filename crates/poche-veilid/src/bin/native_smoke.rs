// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Opt-in full Poche lifecycle over released Veilid DHT/private-route calls.

use std::{
    collections::{BTreeMap, VecDeque},
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use poche_protocol::{
    ChanceWire, CommandId, CommandPayload, CorrelationId, CountdownToken, EventId, EventPayload,
    GameActionWire, HandCapabilityExpiryWire, InviteProof, PROTOCOL_VERSION_V1, PrincipalId,
    ProtocolFrame, RoomId, RoomPhase, SIGNATURE_DOMAIN_V1, SemanticHash, SignatureAlgorithm,
    SignatureIntent, UnsignedCommandEnvelope, UnsignedEventEnvelope,
    verified_command_semantic_hash,
};
use poche_runtime::{
    AuthorityDisposition, InProcessAuthority, InProcessTransport, LoopbackCodec, OracleSessionGame,
};
use poche_session::{
    GameTurn, HandCapabilityExpiry, InviteRecord, PublicGameChange, SessionEvent, SessionEventKind,
    SessionGame, SessionPhase, SessionState,
};
use poche_veilid::{
    ApplicationIdentity, ApplicationPublicIdentity, CommandRetryState, ExplicitInsecureDevelopment,
    IdentityStoragePolicy, InsecureMemoryIdentityStore, PublicRoomMetadata, ResolvedRoom,
    RetryAction, RoomCode, RoomNetwork, TransportCommandCall, TransportCommandReply,
    TransportDisposition, VeilidCommandError, VeilidRendezvous, open_projection, seal_projection,
};
use serde::Serialize;
use veilid_core::{VeilidAPI, VeilidAppCall, VeilidConfig, VeilidUpdate, api_startup};

const READY_TIMEOUT: Duration = Duration::from_secs(90);
const DHT_TIMEOUT: Duration = Duration::from_secs(45);
const PUBLIC_OPT_IN: &str = "I_ACCEPT_PUBLIC_NETWORK_TRAFFIC";

#[derive(Serialize)]
struct SmokeReport {
    schema_version: u16,
    transport: &'static str,
    veilid_version: &'static str,
    calls: u64,
    event_frames: u64,
    duplicate_replies: u64,
    denied_replies: u64,
    reconnects: u64,
    final_revision: u64,
    final_scores: Vec<i32>,
    chat_messages: u64,
    spectator_grant_verified: bool,
    spectator_revoke_verified: bool,
    public_network_opt_in: bool,
}

struct HostRuntime {
    authority: InProcessAuthority<OracleSessionGame<2>>,
    clients: BTreeMap<PrincipalId, poche_runtime::ScriptedClient>,
    identities: BTreeMap<PrincipalId, ApplicationPublicIdentity>,
    host_identity: Arc<ApplicationIdentity>,
    adapter: VeilidRendezvous,
    event_frames: u64,
    duplicate_replies: u64,
    denied_replies: u64,
}

struct LiveRoom {
    room: Option<ResolvedRoom>,
    code: RoomCode,
}

impl HostRuntime {
    #[allow(
        clippy::too_many_lines,
        reason = "the acceptance boundary validates one complete command transaction"
    )]
    async fn process(&mut self, incoming: &VeilidAppCall) -> Result<(), String> {
        let call = VeilidRendezvous::decode_command_call(incoming)
            .map_err(|error| format!("decode command: {error:?}"))?;
        let principal = call.command.principal_id.clone();
        if let Some(known) = self.identities.get(&principal) {
            if known != &call.identity {
                return Err("stable principal changed public identity".to_owned());
            }
        } else {
            self.identities
                .insert(principal.clone(), call.identity.clone());
        }
        let base_revision = self.authority.state.revision;
        let command_hash = verified_command_semantic_hash(&call.command)
            .map_err(|error| format!("command hash: {error:?}"))?;
        let duplicate = self
            .authority
            .state
            .processed_commands
            .iter()
            .any(|record| {
                record.command_id == call.command.command_id && record.command_hash == command_hash
            });
        let reply = if duplicate {
            self.duplicate_replies = self.duplicate_replies.saturating_add(1);
            TransportCommandReply::new(
                call.command.command_id.clone(),
                TransportDisposition::Duplicate,
                None,
                base_revision,
                base_revision,
                Vec::new(),
                None,
            )
            .map_err(|error| format!("duplicate reply: {error:?}"))?
        } else {
            let client = if let Some(client) = self.clients.get(&principal) {
                client.clone()
            } else {
                let client = self
                    .authority
                    .transport
                    .connect(principal.clone())
                    .map_err(|error| format!("connect principal: {error:?}"))?;
                self.clients.insert(principal.clone(), client.clone());
                client
            };
            poche_runtime::ClientPort::submit(
                &client,
                &mut self.authority.transport,
                call.command.clone(),
            )
            .map_err(|error| format!("submit command: {error:?}"))?;
            let outcome = self
                .authority
                .drive_one()
                .map_err(|error| format!("drive authority: {error:?}"))?
                .ok_or_else(|| "authority consumed no command".to_owned())?;
            let current_revision = self.authority.state.revision;
            let disposition = match outcome.disposition {
                AuthorityDisposition::Applied => TransportDisposition::Applied,
                AuthorityDisposition::Denied(_) => TransportDisposition::Denied,
                AuthorityDisposition::Disconnected => TransportDisposition::RecoveryRequired,
            };
            let denial = match outcome.disposition {
                AuthorityDisposition::Denied(reason) => {
                    self.denied_replies = self.denied_replies.saturating_add(1);
                    Some(reason)
                }
                _ => None,
            };
            let mut frames = self
                .authority
                .committed_events_after(base_revision)
                .into_iter()
                .map(|entry| {
                    self.signed_event(
                        entry.revision,
                        &entry.event,
                        self.authority.state.session_epoch,
                    )
                    .map(ProtocolFrame::Event)
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.event_frames = self
                .event_frames
                .saturating_add(u64::try_from(frames.len()).unwrap_or(u64::MAX));
            let mut latest_projection = None;
            while let Some(frame) =
                poche_runtime::ClientPort::receive(&client, &mut self.authority.transport)
                    .map_err(|error| format!("receive authority frame: {error:?}"))?
            {
                match frame {
                    ProtocolFrame::Projection(projection)
                        if projection.current_revision == current_revision =>
                    {
                        latest_projection = Some(projection);
                    }
                    ProtocolFrame::Error(error) => frames.push(ProtocolFrame::Error(error)),
                    ProtocolFrame::Projection(_) => {}
                    _ => return Err("unexpected plaintext authority frame".to_owned()),
                }
            }
            let encrypted_projection = match (disposition, latest_projection) {
                (TransportDisposition::Denied, _) | (_, None) => None,
                (_, Some(projection)) => Some(
                    seal_projection(
                        &self.adapter_api(),
                        &self.host_identity,
                        &call.identity,
                        &projection,
                    )
                    .await
                    .map_err(|error| format!("seal projection: {error:?}"))?,
                ),
            };
            TransportCommandReply::new(
                call.command.command_id.clone(),
                disposition,
                denial,
                base_revision,
                current_revision,
                frames,
                encrypted_projection,
            )
            .map_err(|error| format!("authority reply: {error:?}"))?
        };
        self.adapter
            .reply_command_call(incoming, &reply)
            .await
            .map_err(|error| format!("reply command call: {error:?}"))
    }

    fn adapter_api(&self) -> VeilidAPI {
        self.adapter.api()
    }

    fn signed_event(
        &self,
        revision: u64,
        event: &SessionEvent<OracleSessionGame<2>>,
        session_epoch: u64,
    ) -> Result<poche_protocol::EventEnvelope, String> {
        let public = self.host_identity.public();
        let payload = event_payload(&event.kind)?;
        self.host_identity
            .sign_event(UnsignedEventEnvelope {
                protocol_version: PROTOCOL_VERSION_V1,
                room_id: self.authority.state.room_id.clone(),
                session_epoch,
                event_id: EventId::new(format!("veilid-event-{revision}"))
                    .map_err(|error| format!("{error:?}"))?,
                principal_id: public.principal_id.clone(),
                current_revision: revision,
                correlation_id: event.provenance.correlation_id.clone(),
                causation_id: event.provenance.command_id.clone(),
                payload,
                signature_intent: SignatureIntent {
                    domain_version: SIGNATURE_DOMAIN_V1,
                    algorithm: SignatureAlgorithm::Ed25519,
                    key_id: public.principal_id,
                },
            })
            .map_err(|error| format!("sign event: {error:?}"))
    }

    fn disconnect(&mut self, principal: &PrincipalId) -> Result<(), String> {
        let client = self
            .clients
            .remove(principal)
            .ok_or_else(|| "disconnect principal has no route".to_owned())?;
        self.authority
            .transport
            .disconnect(client.connection_id())
            .map_err(|error| format!("disconnect route: {error:?}"))?;
        let outcome = self
            .authority
            .drive_one()
            .map_err(|error| format!("drive disconnect: {error:?}"))?
            .ok_or_else(|| "disconnect produced no outcome".to_owned())?;
        if !matches!(outcome.disposition, AuthorityDisposition::Disconnected) {
            return Err("disconnect did not reach the semantic boundary".to_owned());
        }
        Ok(())
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the exhaustive mapping keeps every session event explicit"
)]
fn event_payload(kind: &SessionEventKind<OracleSessionGame<2>>) -> Result<EventPayload, String> {
    Ok(match kind {
        SessionEventKind::RoomCreated { .. } => EventPayload::RoomCreated,
        SessionEventKind::MemberJoined { principal, .. } => EventPayload::MemberJoined {
            principal: principal.clone(),
        },
        SessionEventKind::SeatTaken { principal, seat } => EventPayload::SeatTaken {
            principal: principal.clone(),
            seat: *seat,
        },
        SessionEventKind::SeatReleased { principal, seat } => EventPayload::SeatReleased {
            principal: principal.clone(),
            seat: *seat,
        },
        SessionEventKind::ReadyChanged { principal, ready } => EventPayload::ReadyChanged {
            principal: principal.clone(),
            ready: *ready,
        },
        SessionEventKind::CountdownArmed {
            deadline_tick,
            token,
        } => EventPayload::CountdownArmed {
            deadline_tick: *deadline_tick,
            countdown_token: token.clone(),
        },
        SessionEventKind::CountdownCancelled { token } => EventPayload::CountdownAborted {
            countdown_token: token.clone(),
        },
        SessionEventKind::GameStarted { .. } | SessionEventKind::Unpaused => {
            EventPayload::PhaseChanged {
                phase: RoomPhase::Running,
            }
        }
        SessionEventKind::GameAdvanced {
            game,
            round_scores,
            public_change,
            ..
        } => {
            if let Some(scores) = round_scores {
                EventPayload::RoundScored {
                    scores: scores.clone(),
                }
            } else if let Some(PublicGameChange::RoundScored { scores, .. }) = public_change {
                EventPayload::RoundScored {
                    scores: scores.clone(),
                }
            } else {
                let public = game
                    .public_projection()
                    .map_err(|error| format!("public game projection: {error:?}"))?;
                let encoded = serde_json::to_vec(&public).map_err(|error| error.to_string())?;
                EventPayload::GameTransitioned {
                    game_state_hash: SemanticHash(*blake3::hash(&encoded).as_bytes()),
                }
            }
        }
        SessionEventKind::Paused => EventPayload::PhaseChanged {
            phase: RoomPhase::Paused,
        },
        SessionEventKind::MemberDisconnected { principal } => EventPayload::MemberDisconnected {
            principal: principal.clone(),
        },
        SessionEventKind::MemberReconnected { principal } => EventPayload::MemberReconnected {
            principal: principal.clone(),
        },
        SessionEventKind::MemberLeft { principal }
        | SessionEventKind::MemberRemoved { principal } => EventPayload::MemberLeft {
            principal: principal.clone(),
        },
        SessionEventKind::LobbyReset => EventPayload::PhaseChanged {
            phase: RoomPhase::Lobby,
        },
        SessionEventKind::ChatPosted { principal, text } => EventPayload::ChatPosted {
            principal: principal.clone(),
            text: text.clone(),
        },
        SessionEventKind::HandViewRequested {
            request_id,
            player,
            recipient,
        } => EventPayload::HandRequested {
            request_id: request_id.clone(),
            player: player.clone(),
            recipient: recipient.clone(),
        },
        SessionEventKind::HandViewGranted {
            player,
            recipient,
            grant_epoch,
            ..
        } => EventPayload::HandGranted {
            player: player.clone(),
            recipient: recipient.clone(),
            grant_epoch: *grant_epoch,
        },
        SessionEventKind::HandViewDenied {
            request_id,
            player,
            recipient,
        } => EventPayload::HandDenied {
            request_id: request_id.clone(),
            player: player.clone(),
            recipient: recipient.clone(),
        },
        SessionEventKind::HandViewRevoked {
            player,
            recipient,
            grant_epoch,
        } => EventPayload::HandRevoked {
            player: player.clone(),
            recipient: recipient.clone(),
            grant_epoch: *grant_epoch,
        },
        SessionEventKind::HandCapabilitiesExpired { reason } => {
            EventPayload::HandCapabilitiesExpired {
                reason: match reason {
                    HandCapabilityExpiry::RoundBoundary => HandCapabilityExpiryWire::RoundBoundary,
                    HandCapabilityExpiry::SeatRoleChanged { principal } => {
                        HandCapabilityExpiryWire::SeatRoleChanged(principal.clone())
                    }
                    HandCapabilityExpiry::MembershipLost { principal } => {
                        HandCapabilityExpiryWire::MembershipLost(principal.clone())
                    }
                },
            }
        }
        SessionEventKind::RoomClosed => EventPayload::RoomClosed,
    })
}

fn free_udp_port() -> Result<u16, String> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| error.to_string())?;
    socket
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| error.to_string())
}

fn config(path: &str, namespace: &str, port: u16) -> VeilidConfig {
    let mut config = VeilidConfig::new(
        "poche_veilid_native_smoke",
        "teamdman",
        "org",
        Some(path),
        Some(path),
    );
    namespace.clone_into(&mut config.namespace);
    config.protected_store.always_use_insecure_storage = true;
    "test-only-password".clone_into(&mut config.protected_store.device_encryption_key_password);
    config.network.upnp = false;
    config.network.protocol.udp.listen_address = format!("0.0.0.0:{port}");
    config.network.protocol.tcp.listen = false;
    config.network.protocol.ws.listen = false;
    config
}

async fn wait_for_public_ready(api: &VeilidAPI, label: &str) -> Result<(), String> {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let state = api
            .get_state()
            .await
            .map_err(|error| format!("{label} state: {error:?}"))?;
        if state.attachment.public_internet_ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{label} public readiness timeout (attachment={:?}, peers={})",
                state.attachment.state,
                state.network.peers.len()
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn now_unix_ms() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    u64::try_from(millis).map_err(|error| error.to_string())
}

async fn identity() -> Result<Arc<ApplicationIdentity>, String> {
    let store = InsecureMemoryIdentityStore::new(
        ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
    );
    ApplicationIdentity::load_or_create(
        &store,
        IdentityStoragePolicy::AllowExplicitInsecure(
            ExplicitInsecureDevelopment::AcknowledgeSecretsAreNotProtected,
        ),
    )
    .await
    .map(Arc::new)
    .map_err(|error| format!("identity: {error:?}"))
}

fn unsigned_command(
    identity: &ApplicationIdentity,
    state: &SessionState<OracleSessionGame<2>>,
    command_id: &str,
    payload: CommandPayload,
) -> Result<UnsignedCommandEnvelope, String> {
    let public = identity.public();
    Ok(UnsignedCommandEnvelope {
        protocol_version: PROTOCOL_VERSION_V1,
        room_id: state.room_id.clone(),
        session_epoch: state.session_epoch,
        command_id: CommandId::new(command_id).map_err(|error| format!("{error:?}"))?,
        principal_id: public.principal_id.clone(),
        expected_revision: state.revision,
        correlation_id: CorrelationId::new(format!("cor-{command_id}"))
            .map_err(|error| format!("{error:?}"))?,
        causation_id: None,
        payload,
        signature_intent: SignatureIntent {
            domain_version: SIGNATURE_DOMAIN_V1,
            algorithm: SignatureAlgorithm::Ed25519,
            key_id: public.principal_id,
        },
    })
}

async fn make_call(
    identity: &ApplicationIdentity,
    runtime: &tokio::sync::Mutex<HostRuntime>,
    command_id: &str,
    payload: CommandPayload,
) -> Result<TransportCommandCall, String> {
    let state = runtime.lock().await.authority.state.clone();
    let unsigned = unsigned_command(identity, &state, command_id, payload)?;
    let command = identity
        .sign_command(unsigned)
        .map_err(|error| format!("sign command: {error:?}"))?;
    TransportCommandCall::new(identity.public(), command)
        .map_err(|error| format!("command call: {error:?}"))
}

async fn send(
    identity: &ApplicationIdentity,
    runtime: &tokio::sync::Mutex<HostRuntime>,
    client_adapter: &VeilidRendezvous,
    room: &tokio::sync::Mutex<LiveRoom>,
    calls: &mut u64,
    command_id: &str,
    payload: CommandPayload,
) -> Result<TransportCommandReply, String> {
    let call = make_call(identity, runtime, command_id, payload).await?;
    send_exact(client_adapter, room, calls, &call)
        .await
        .map_err(|error| format!("command {command_id}: {error}"))
}

async fn send_exact(
    client_adapter: &VeilidRendezvous,
    room: &tokio::sync::Mutex<LiveRoom>,
    calls: &mut u64,
    call: &TransportCommandCall,
) -> Result<TransportCommandReply, String> {
    let mut retry = CommandRetryState::new(&call.command, 5)
        .map_err(|error| format!("retry state: {error:?}"))?;
    loop {
        retry
            .begin_attempt(&call.command)
            .map_err(|error| format!("retry attempt: {error:?}"))?;
        *calls = calls.saturating_add(1);
        let result = {
            let locked = room.lock().await;
            let current = locked
                .room
                .as_ref()
                .ok_or_else(|| "validated room route is unavailable".to_owned())?;
            client_adapter.command_call(current, call).await
        };
        match result {
            Ok(reply) => return Ok(reply),
            Err(VeilidCommandError::Transport(failure))
                if retry.after_failure(failure) == RetryAction::RetrySameRoute =>
            {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(VeilidCommandError::Transport(failure))
                if retry.after_failure(failure) == RetryAction::RefreshRendezvousThenRetry =>
            {
                let mut locked = room.lock().await;
                if let Some(stale) = locked.room.take() {
                    let _ = stale.release(&client_adapter.api()).await;
                }
                let deadline = Instant::now() + DHT_TIMEOUT;
                loop {
                    match client_adapter
                        .resolve_room(&locked.code, now_unix_ms()?)
                        .await
                    {
                        Ok(replacement) => {
                            locked.room = Some(replacement);
                            break;
                        }
                        Err(_) if Instant::now() < deadline => {
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                        Err(error) => {
                            return Err(format!("validated rendezvous refresh: {error:?}"));
                        }
                    }
                }
            }
            Err(error) => return Err(format!("network command call: {error:?}")),
        }
    }
}

fn first_legal_action(
    state: &SessionState<OracleSessionGame<2>>,
) -> Result<GameActionWire, String> {
    let (SessionPhase::Running { game } | SessionPhase::Paused { game }) = &state.phase else {
        return Err("legal action requested outside a running game".to_owned());
    };
    let action = game
        .game
        .legal_player_actions()
        .into_iter()
        .next()
        .ok_or_else(|| "acting player has no legal action".to_owned())?;
    Ok(match action {
        poche_oracle_rust::Action::Bid { tricks, .. } => GameActionWire::Bid { tricks },
        poche_oracle_rust::Action::Play { card, .. } => GameActionWire::Play {
            card: poche_oracle_rust::Card::standard_deck()
                .iter()
                .position(|candidate| *candidate == card)
                .and_then(|index| u8::try_from(index).ok())
                .ok_or_else(|| "legal card has no canonical code".to_owned())?,
        },
        poche_oracle_rust::Action::Deal(_) | poche_oracle_rust::Action::SettleRound => {
            return Err("player legal-action list contained an environment action".to_owned());
        }
    })
}

fn chance_wire(seed: u64, deal_ordinal: u32) -> Result<ChanceWire, String> {
    let chance = poche_environment::OracleChanceAction::seeded(seed, deal_ordinal);
    let standard = poche_oracle_rust::Card::standard_deck();
    let cards = chance
        .deck
        .cards()
        .iter()
        .map(|card| {
            standard
                .iter()
                .position(|candidate| candidate == card)
                .and_then(|index| u8::try_from(index).ok())
                .ok_or_else(|| "seeded card has no canonical code".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ChanceWire {
        cards,
        seed: Some(seed),
        deal_ordinal: Some(deal_ordinal),
    })
}

async fn shutdown(api: VeilidAPI) {
    let _ = api.detach().await;
    api.shutdown().await;
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), String> {
    if std::env::var("POCHE_ALLOW_VEILID_PUBLIC_TEST").as_deref() != Ok(PUBLIC_OPT_IN) {
        return Err(format!(
            "set POCHE_ALLOW_VEILID_PUBLIC_TEST={PUBLIC_OPT_IN} to opt in"
        ));
    }
    let host_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let client_directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let incoming = Arc::new(Mutex::new(VecDeque::new()));
    let host_incoming = Arc::clone(&incoming);
    let host_api = api_startup(
        Arc::new(move |update| {
            if let VeilidUpdate::AppCall(call) = update {
                host_incoming
                    .lock()
                    .expect("incoming queue poisoned")
                    .push_back(*call);
            }
        }),
        config(
            &host_directory.path().to_string_lossy(),
            "host",
            free_udp_port()?,
        ),
    )
    .await
    .map_err(|error| format!("host startup: {error:?}"))?;
    host_api
        .attach()
        .await
        .map_err(|error| format!("host attach: {error:?}"))?;
    wait_for_public_ready(&host_api, "host").await?;
    let client_api = api_startup(
        Arc::new(drop),
        config(
            &client_directory.path().to_string_lossy(),
            "client",
            free_udp_port()?,
        ),
    )
    .await
    .map_err(|error| format!("client startup: {error:?}"))?;
    client_api
        .attach()
        .await
        .map_err(|error| format!("client attach: {error:?}"))?;
    wait_for_public_ready(&client_api, "client").await?;

    let host_identity = identity().await?;
    let player_identity = identity().await?;
    let spectator_identity = identity().await?;
    let game_identity = identity().await?;
    let clock_identity = identity().await?;
    let room_id = RoomId::new("veilid-native-smoke").map_err(|error| format!("{error:?}"))?;
    let host_adapter = VeilidRendezvous::new(host_api.clone())
        .map_err(|error| format!("host adapter: {error:?}"))?;
    let now = now_unix_ms()?;
    let publish_deadline = Instant::now() + DHT_TIMEOUT;
    let published = loop {
        match host_adapter
            .publish_room(
                &host_identity,
                RoomNetwork::VeilidPublic,
                room_id.clone(),
                PublicRoomMetadata::new("Poche native acceptance", 2, true)
                    .map_err(|error| format!("{error:?}"))?,
                1,
                1,
                now.saturating_add(10 * 60 * 1_000),
                now,
            )
            .await
        {
            Ok(room) => break room,
            Err(_) if Instant::now() < publish_deadline => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(error) => return Err(format!("publish room: {error:?}")),
        }
    };
    let code_text = published
        .room_code()
        .encode()
        .map_err(|error| format!("encode room code: {error:?}"))?;
    let code = RoomCode::decode(code_text.expose(), now)
        .map_err(|error| format!("decode room code: {error:?}"))?;
    let client_adapter = VeilidRendezvous::new(client_api.clone())
        .map_err(|error| format!("client adapter: {error:?}"))?;
    let deadline = Instant::now() + DHT_TIMEOUT;
    let resolved_room = loop {
        match client_adapter.resolve_room(&code, now_unix_ms()?).await {
            Ok(room) => break room,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(error) => return Err(format!("resolve room: {error:?}")),
        }
    };
    let resolved = tokio::sync::Mutex::new(LiveRoom {
        room: Some(resolved_room),
        code,
    });

    let spectator_invite = "poche-native-spectator-v1";
    let mut state = SessionState::pending(
        room_id,
        clock_identity.public().principal_id,
        game_identity.public().principal_id,
    );
    state.session_epoch = 1;
    state.invites.push(
        InviteRecord::new(code_text.expose(), u64::MAX).map_err(|error| format!("{error:?}"))?,
    );
    state
        .invites
        .push(InviteRecord::new(spectator_invite, u64::MAX).map_err(|error| format!("{error:?}"))?);
    let runtime = Arc::new(tokio::sync::Mutex::new(HostRuntime {
        authority: InProcessAuthority::new(
            state,
            InProcessTransport::new(LoopbackCodec::CanonicalNdjson),
        ),
        clients: BTreeMap::new(),
        identities: BTreeMap::new(),
        host_identity: Arc::clone(&host_identity),
        adapter: host_adapter,
        event_frames: 0,
        duplicate_replies: 0,
        denied_replies: 0,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let responder_runtime = Arc::clone(&runtime);
    let responder_incoming = Arc::clone(&incoming);
    let responder_stop = Arc::clone(&stop);
    let responder = tokio::spawn(async move {
        while !responder_stop.load(Ordering::Acquire) {
            let next = responder_incoming
                .lock()
                .expect("incoming queue poisoned")
                .pop_front();
            if let Some(call) = next {
                if let Err(error) = responder_runtime.lock().await.process(&call).await {
                    eprintln!("veilid native responder failed: {error}");
                    return Err(error);
                }
            } else {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
        Ok::<(), String>(())
    });

    let mut calls = 0_u64;
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "create",
        CommandPayload::CreateRoom,
    )
    .await?;
    send(
        &player_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "join-player",
        CommandPayload::RedeemInvite {
            invite: InviteProof::new(code_text.expose()).map_err(|error| format!("{error:?}"))?,
        },
    )
    .await?;
    send(
        &spectator_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "join-spectator",
        CommandPayload::RedeemInvite {
            invite: InviteProof::new(spectator_invite).map_err(|error| format!("{error:?}"))?,
        },
    )
    .await?;
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "seat-host",
        CommandPayload::TakeSeat { seat: 0 },
    )
    .await?;
    send(
        &player_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "seat-player",
        CommandPayload::TakeSeat { seat: 1 },
    )
    .await?;
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "ready-host",
        CommandPayload::Ready,
    )
    .await?;
    send(
        &player_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "ready-player",
        CommandPayload::Ready,
    )
    .await?;
    let first_countdown =
        CountdownToken::new("native-countdown-one").map_err(|error| format!("{error:?}"))?;
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "countdown-one",
        CommandPayload::ArmCountdown {
            deadline_tick: 10,
            countdown_token: first_countdown,
        },
    )
    .await?;
    send(
        &player_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "abort-countdown",
        CommandPayload::AbortCountdown,
    )
    .await?;
    let second_countdown =
        CountdownToken::new("native-countdown-two").map_err(|error| format!("{error:?}"))?;
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "countdown-two",
        CommandPayload::ArmCountdown {
            deadline_tick: 20,
            countdown_token: second_countdown.clone(),
        },
    )
    .await?;
    send(
        &clock_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "countdown-expired",
        CommandPayload::CountdownExpired {
            countdown_token: second_countdown,
        },
    )
    .await?;
    send(
        &game_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "deal-zero",
        CommandPayload::ApplyChance {
            chance: chance_wire(0x5eed, 0)?,
        },
    )
    .await?;
    send(
        &player_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "pause-player",
        CommandPayload::Pause,
    )
    .await?;
    let paused_action = first_legal_action(&runtime.lock().await.authority.state)?;
    let denied = send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "denied-while-paused",
        CommandPayload::GameAction {
            action: paused_action,
        },
    )
    .await?;
    if denied.disposition != TransportDisposition::Denied {
        return Err("paused game action was not denied".to_owned());
    }
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "unpause-host",
        CommandPayload::Unpause,
    )
    .await?;
    send(
        &spectator_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "request-host-hand",
        CommandPayload::RequestHand {
            player: host_identity.public().principal_id.clone(),
        },
    )
    .await?;
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "grant-host-hand",
        CommandPayload::GrantHand {
            request_id: CommandId::new("request-host-hand")
                .map_err(|error| format!("{error:?}"))?,
            player: host_identity.public().principal_id.clone(),
            recipient: spectator_identity.public().principal_id.clone(),
            grant_epoch: 1,
        },
    )
    .await?;
    let grant_reply = send(
        &spectator_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "spectator-grant-observe",
        CommandPayload::Chat {
            text: "grant observed".to_owned(),
        },
    )
    .await?;
    let grant_epoch = runtime.lock().await.authority.state.projection_epoch;
    let grant_packet = grant_reply
        .encrypted_projection
        .as_ref()
        .ok_or_else(|| "spectator received no encrypted grant projection".to_owned())?;
    let grant_projection = open_projection(
        &client_api,
        &host_identity.public(),
        &spectator_identity,
        grant_epoch,
        grant_packet,
    )
    .await
    .map_err(|error| format!("open grant projection: {error:?}"))?;
    let spectator_grant_verified =
        grant_projection.payload.granted_hands.iter().any(|hand| {
            hand.player == host_identity.public().principal_id && !hand.cards.is_empty()
        });
    send(
        &host_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "revoke-host-hand",
        CommandPayload::RevokeHand {
            player: host_identity.public().principal_id.clone(),
            recipient: spectator_identity.public().principal_id.clone(),
            grant_epoch: 1,
        },
    )
    .await?;
    let revoke_reply = send(
        &spectator_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "spectator-revoke-observe",
        CommandPayload::Chat {
            text: "revoke observed".to_owned(),
        },
    )
    .await?;
    let revoke_epoch = runtime.lock().await.authority.state.projection_epoch;
    let revoke_projection = open_projection(
        &client_api,
        &host_identity.public(),
        &spectator_identity,
        revoke_epoch,
        revoke_reply
            .encrypted_projection
            .as_ref()
            .ok_or_else(|| "spectator received no encrypted revoke projection".to_owned())?,
    )
    .await
    .map_err(|error| format!("open revoke projection: {error:?}"))?;
    let spectator_revoke_verified = revoke_projection.payload.granted_hands.is_empty();
    runtime
        .lock()
        .await
        .disconnect(&player_identity.public().principal_id)?;
    send(
        &player_identity,
        &runtime,
        &client_adapter,
        &resolved,
        &mut calls,
        "reconnect-player",
        CommandPayload::Reconnect,
    )
    .await?;

    let duplicate_call = make_call(
        &host_identity,
        &runtime,
        "duplicate-chat",
        CommandPayload::Chat {
            text: "duplicate exactly once".to_owned(),
        },
    )
    .await?;
    send_exact(&client_adapter, &resolved, &mut calls, &duplicate_call).await?;
    let duplicate_reply =
        send_exact(&client_adapter, &resolved, &mut calls, &duplicate_call).await?;
    if duplicate_reply.disposition != TransportDisposition::Duplicate {
        return Err("duplicate command did not receive duplicate disposition".to_owned());
    }

    let mut deal_ordinal = 1_u32;
    let mut game_sequence = 0_u32;
    loop {
        let state = runtime.lock().await.authority.state.clone();
        if matches!(state.phase, SessionPhase::PostGame { .. }) {
            break;
        }
        let SessionPhase::Running { game } = &state.phase else {
            return Err("game left running phase unexpectedly".to_owned());
        };
        let (identity, payload) = match game.turn() {
            GameTurn::Chance => {
                let payload = CommandPayload::ApplyChance {
                    chance: chance_wire(0x5eed, deal_ordinal)?,
                };
                deal_ordinal = deal_ordinal.saturating_add(1);
                (&game_identity, payload)
            }
            GameTurn::Player(seat) => {
                let identity = if seat == 0 {
                    &host_identity
                } else {
                    &player_identity
                };
                (
                    identity,
                    CommandPayload::GameAction {
                        action: first_legal_action(&state)?,
                    },
                )
            }
            GameTurn::Environment => (&game_identity, CommandPayload::Settle),
            GameTurn::Finished => return Err("finished game did not enter post-game".to_owned()),
        };
        send(
            identity,
            &runtime,
            &client_adapter,
            &resolved,
            &mut calls,
            &format!("game-{game_sequence}"),
            payload,
        )
        .await?;
        game_sequence = game_sequence.saturating_add(1);
    }
    let (
        final_revision,
        final_scores,
        chat_messages,
        event_frames,
        duplicate_replies,
        denied_replies,
    ) = {
        let locked = runtime.lock().await;
        let scores = match &locked.authority.state.phase {
            SessionPhase::PostGame { game } => game
                .public_projection()
                .map_err(|error| format!("final projection: {error:?}"))?
                .scores
                .into_iter()
                .map(i32::from)
                .collect(),
            _ => return Err("smoke did not finish in post-game".to_owned()),
        };
        (
            locked.authority.state.revision,
            scores,
            locked.authority.chat_tail().len() as u64,
            locked.event_frames,
            locked.duplicate_replies,
            locked.denied_replies,
        )
    };
    if !spectator_grant_verified || !spectator_revoke_verified {
        return Err("spectator projection acceptance failed".to_owned());
    }
    let report = SmokeReport {
        schema_version: 1,
        transport: "veilid-0.5.7-public-dht-private-route-app-call",
        veilid_version: "0.5.7",
        calls,
        event_frames,
        duplicate_replies,
        denied_replies,
        reconnects: 1,
        final_revision,
        final_scores,
        chat_messages,
        spectator_grant_verified,
        spectator_revoke_verified,
        public_network_opt_in: true,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    stop.store(true, Ordering::Release);
    responder.await.map_err(|error| error.to_string())??;
    if let Some(room) = resolved.into_inner().room {
        room.release(&client_api)
            .await
            .map_err(|error| format!("resolved cleanup: {error:?}"))?;
    }
    published
        .close(&host_api)
        .await
        .map_err(|error| format!("published cleanup: {error:?}"))?;
    shutdown(client_api).await;
    shutdown(host_api).await;
    Ok(())
}
