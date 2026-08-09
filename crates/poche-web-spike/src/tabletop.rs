// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Stateful semantic-tabletop lab over real session, audit, and governance reducers.

use poche_protocol::{
    AccusationId, CommandId, EventId, GameActionWire, GovernanceCommandV1, GovernanceCommandWire,
    GovernedActionWire, PrincipalId, RecoveryActionWire, VoteChoiceWire,
};
use poche_session::{
    AccusationOutcome, ActionKnowledge, AuditedGameAction, GovernanceInvocation, GovernanceState,
    HistoryActionDisposition, ManualAccusation, ProposalStatus, RetrospectiveAudit,
};
use poche_spatial::{LayoutId, SpatialScene, TableId, registered_layout};
use poche_ui::{
    LiveClientPresentation, TabletopFindingPresentation, TabletopHtmlSupplement,
    TabletopProposalPresentation, TabletopVotePresentation, realize_presentation_spatial,
};

use crate::demo::LiveDemo;

const AUDITED_ACTION: &str = "tabletop-off-suit-action";

pub struct TabletopLab {
    demo: LiveDemo,
    audit: RetrospectiveAudit,
    governance: GovernanceState,
    next_sidecar_command: u64,
    status: String,
}

impl TabletopLab {
    pub fn new() -> Result<Self, String> {
        let mut demo = LiveDemo::named("semantic-tabletop")?;
        demo.setup("running")?;
        let mut audit = RetrospectiveAudit::default();
        audit
            .append_action(AuditedGameAction {
                event_id: event(AUDITED_ACTION)?,
                sequence: 1,
                round_id: 1,
                actor: principal("bob")?,
                disposition: HistoryActionDisposition::AttemptedStructurallyValid,
                action: GameActionWire::Play { card: 26 },
                led_suit: Some(0),
                knowledge_at_action: ActionKnowledge {
                    known_held_cards_before: vec![26, 3],
                },
            })
            .map_err(debug_error)?;
        Ok(Self {
            demo,
            audit,
            governance: GovernanceState::new([principal("alice")?, principal("bob")?], 10)
                .map_err(debug_error)?,
            next_sidecar_command: 0,
            status: "ready: actions cross typed reducers; HTML is only a projection".to_owned(),
        })
    }

    pub fn projection(
        &self,
        viewer: &str,
    ) -> Result<(LiveClientPresentation, SpatialScene, TabletopHtmlSupplement), String> {
        let live = self.demo.view(viewer)?;
        let table = live
            .projection
            .table
            .as_ref()
            .ok_or_else(|| "tabletop lab has no running table".to_owned())?;
        let players = u8::try_from(table.hand_counts.len())
            .map_err(|_| "tabletop player count does not fit u8".to_owned())?;
        let layout_id = LayoutId::new(players, 1)
            .ok_or_else(|| "unsupported tabletop player count".to_owned())?;
        let layout =
            registered_layout(TableId::new(0x504f_4348_4533), layout_id).map_err(debug_error)?;
        let scene =
            realize_presentation_spatial(&layout, 1, &live.projection).map_err(debug_error)?;
        Ok((live, scene, self.supplement()))
    }

    pub fn action(&mut self, viewer: &str, control: &str) -> Result<String, String> {
        let status = match control {
            "accuse" => {
                self.require_connected_player(viewer)?;
                self.accuse(viewer)
            }
            "start-vote" => {
                self.require_connected_player(viewer)?;
                self.start_vote(viewer)
            }
            "vote-approve" => {
                self.require_connected_player(viewer)?;
                self.vote(viewer, VoteChoiceWire::Approve)
            }
            _ => {
                let result = self.demo.control(viewer, control);
                if control == "reconnect" && result.is_ok() && matches!(viewer, "alice" | "bob") {
                    self.governance
                        .set_connected(&principal(viewer)?, true)
                        .map_err(debug_error)?;
                }
                result
            }
        }?;
        self.status.clone_from(&status);
        Ok(status)
    }

    pub fn disconnect(&mut self, viewer: &str) -> Result<String, String> {
        let status = self.demo.disconnect(viewer)?;
        if matches!(viewer, "alice" | "bob") {
            self.governance
                .set_connected(&principal(viewer)?, false)
                .map_err(debug_error)?;
        }
        self.status.clone_from(&status);
        Ok(status)
    }

    fn require_connected_player(&self, viewer: &str) -> Result<(), String> {
        require_player(viewer)?;
        let view = self.demo.view(viewer)?;
        let connected =
            view.projection.members.iter().any(|member| {
                member.principal == viewer && member.connected && member.seat.is_some()
            });
        if connected {
            Ok(())
        } else {
            Err("disconnected players must reconnect before issuing governance commands".to_owned())
        }
    }

    fn accuse(&mut self, viewer: &str) -> Result<String, String> {
        require_player(viewer)?;
        let id = self.next_id("accuse");
        let record = self
            .audit
            .accuse(ManualAccusation {
                accusation_id: AccusationId::new(id).map_err(debug_error)?,
                detector: principal(viewer)?,
                offending_action_id: event(AUDITED_ACTION)?,
            })
            .map_err(debug_error)?;
        match record.outcome {
            AccusationOutcome::Confirmed { finding_id } => {
                let accused = self
                    .governance
                    .register_confirmed_finding(&self.audit, &finding_id)
                    .map_err(debug_error)?;
                Ok(format!(
                    "confirmed finding {}; accused {} remains visible but loses counted votes about their own sanction",
                    finding_id.as_str(),
                    accused.as_str()
                ))
            }
            AccusationOutcome::Unfounded { reason } => {
                Ok(format!("accusation unfounded: {reason:?}"))
            }
        }
    }

    fn start_vote(&mut self, viewer: &str) -> Result<String, String> {
        require_player(viewer)?;
        let command_id = self.next_id("start-redeal");
        let receipt = self
            .governance
            .submit(GovernanceInvocation {
                command_id: CommandId::new(command_id).map_err(debug_error)?,
                issuer: principal(viewer)?,
                logical_tick: self.next_sidecar_command,
                command: GovernanceCommandV1::new(GovernanceCommandWire::StartVote {
                    action: GovernedActionWire::Recover {
                        recovery: RecoveryActionWire::Redeal,
                    },
                })
                .map_err(debug_error)?,
            })
            .map_err(debug_error)?;
        Ok(format!(
            "opened typed redeal proposal {}",
            receipt
                .proposal_id
                .as_ref()
                .map_or("unknown", |id| id.as_str())
        ))
    }

    fn vote(&mut self, viewer: &str, choice: VoteChoiceWire) -> Result<String, String> {
        require_player(viewer)?;
        let proposal_id = self
            .governance
            .proposals()
            .iter()
            .rev()
            .find(|proposal| matches!(proposal.status, ProposalStatus::Pending))
            .map(|proposal| proposal.proposal_id.clone())
            .ok_or_else(|| "there is no pending proposal".to_owned())?;
        let command_id = self.next_id("vote");
        let receipt = self
            .governance
            .submit(GovernanceInvocation {
                command_id: CommandId::new(command_id).map_err(debug_error)?,
                issuer: principal(viewer)?,
                logical_tick: self.next_sidecar_command,
                command: GovernanceCommandV1::new(GovernanceCommandWire::Vote {
                    proposal_id: proposal_id.clone(),
                    choice,
                })
                .map_err(debug_error)?,
            })
            .map_err(debug_error)?;
        Ok(format!(
            "recorded vote on {}; counted={}",
            proposal_id.as_str(),
            receipt.vote_counted.unwrap_or(false)
        ))
    }

    fn next_id(&mut self, prefix: &str) -> String {
        let id = format!("tabletop-{prefix}-{}", self.next_sidecar_command);
        self.next_sidecar_command = self.next_sidecar_command.saturating_add(1);
        id
    }

    fn supplement(&self) -> TabletopHtmlSupplement {
        TabletopHtmlSupplement {
            status: Some(self.status.clone()),
            room_code: None,
            main_menu_href: Some("/".to_owned()),
            exit_endpoint: None,
            chat_endpoint: None,
            governance_commands: true,
            viewer_href_prefix: Some("/tabletop".to_owned()),
            findings: self
                .audit
                .findings()
                .iter()
                .map(|finding| TabletopFindingPresentation {
                    id: finding.finding_id.as_str().to_owned(),
                    summary: format!(
                        "{} proves event {} violated {} with revealed card {}",
                        finding.detector.as_str(),
                        finding.offending_action_id.as_str(),
                        finding.rule_id,
                        finding.revealed_card
                    ),
                    status: format!("confirmed ({:?})", finding.confidence),
                })
                .collect(),
            proposals: self
                .governance
                .proposals()
                .iter()
                .map(|proposal| TabletopProposalPresentation {
                    id: proposal.proposal_id.as_str().to_owned(),
                    action: format!("{:?}", proposal.action),
                    status: format!("{:?}", proposal.status),
                    votes: proposal
                        .votes
                        .iter()
                        .map(|vote| TabletopVotePresentation {
                            voter: vote.voter.as_str().to_owned(),
                            choice: format!("{:?}", vote.choice),
                            counted: vote.counted,
                            exclusion: vote.exclusion.map(|value| format!("{value:?}")),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

fn require_player(viewer: &str) -> Result<(), String> {
    if matches!(viewer, "alice" | "bob") {
        Ok(())
    } else {
        Err("only a seated active player may issue this governance command".to_owned())
    }
}

fn principal(value: &str) -> Result<PrincipalId, String> {
    PrincipalId::new(value).map_err(debug_error)
}

fn event(value: &str) -> Result<EventId, String> {
    EventId::new(value).map_err(debug_error)
}

fn debug_error(error: impl core::fmt::Debug) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use poche_protocol::{CommandPayload, PublicGamePhase, RoomPhase};
    use poche_spatial::spatial_scene_hash_hex;
    use poche_ui::render_tabletop_semantic_html;

    use super::TabletopLab;

    #[test]
    fn typed_tabletop_actions_preserve_privacy_and_surface_governance() {
        let mut lab = TabletopLab::new().expect("lab");
        let (alice, alice_scene, _) = lab.projection("alice").expect("Alice projection");
        let (_, spectator_scene, _) = lab.projection("spectator").expect("spectator projection");
        assert!(alice.projection.own_hand.is_some());
        assert!(spectator_scene.cards.iter().all(|card| card.face.is_none()
            || matches!(
                card.location,
                poche_spatial::CardLocation::Trump | poche_spatial::CardLocation::Play { .. }
            )));
        assert_ne!(
            spatial_scene_hash_hex(&alice_scene).expect("Alice hash"),
            spatial_scene_hash_hex(&spectator_scene).expect("spectator hash")
        );

        assert!(
            lab.action("alice", "accuse")
                .expect("accuse")
                .contains("confirmed")
        );
        assert!(
            lab.action("alice", "start-vote")
                .expect("proposal")
                .contains("opened")
        );
        assert!(
            lab.action("alice", "vote-approve")
                .expect("vote")
                .contains("counted=true")
        );
        let (_, _, supplement) = lab.projection("alice").expect("updated projection");
        assert_eq!(supplement.findings.len(), 1);
        assert_eq!(supplement.proposals.len(), 1);
        assert_eq!(supplement.proposals[0].votes.len(), 1);

        lab.disconnect("alice").expect("disconnect");
        assert!(lab.action("alice", "start-vote").is_err());
        lab.action("alice", "reconnect").expect("reconnect");
        assert!(lab.action("alice", "start-vote").is_ok());
    }

    #[test]
    fn rendered_controls_drive_a_complete_game_without_a_preprogrammed_action_script() {
        let mut lab = TabletopLab::new().expect("lab");
        let mut saw_bidding = false;
        let mut saw_playing = false;
        let mut submitted_actions = 0_usize;

        for _ in 0..512 {
            let mut next = None;
            let mut terminal = false;
            for viewer in ["alice", "bob"] {
                let (live, scene, supplement) = lab.projection(viewer).expect("viewer projection");
                terminal |= live.projection.room_phase == RoomPhase::PostGame;
                if let Some(table) = &live.projection.table {
                    saw_bidding |= table.phase == PublicGamePhase::Bidding;
                    saw_playing |= table.phase == PublicGamePhase::Playing;
                }
                let html = render_tabletop_semantic_html(
                    &live,
                    &scene,
                    "test-tabletop",
                    &format!("/tabletop/{viewer}/action"),
                    &supplement,
                )
                .expect("rendered tabletop");
                let game_controls = live
                    .controls
                    .iter()
                    .filter(|control| matches!(control.payload, CommandPayload::GameAction { .. }))
                    .collect::<Vec<_>>();
                for control in &game_controls {
                    assert!(
                        html.contains(&format!("data-command-id=\"{}\"", control.id)),
                        "every retained game action must be operable from rendered HTML"
                    );
                }
                if let Some(control) = game_controls.first() {
                    assert!(
                        next.is_none(),
                        "only the current actor may expose game actions"
                    );
                    next = Some((viewer, control.id.clone()));
                }
            }
            if terminal {
                assert!(next.is_none());
                break;
            }
            let (viewer, control) = next.expect("a nonterminal game must expose an actor control");
            lab.action(viewer, &control)
                .expect("rendered typed control must apply");
            submitted_actions += 1;
        }

        let (final_view, _, _) = lab.projection("alice").expect("final projection");
        assert_eq!(final_view.projection.room_phase, RoomPhase::PostGame);
        assert!(saw_bidding && saw_playing);
        assert!(
            submitted_actions > 100,
            "the test must drive the whole game"
        );
    }
}
