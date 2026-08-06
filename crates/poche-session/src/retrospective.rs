// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Append-only game-action knowledge and retrospective rule auditing.

use core::fmt;

use facet::Facet;
use poche_protocol::{AccusationId, EventId, FindingId, GameActionWire, PrincipalId};

const FOLLOW_SUIT_RULE_ID: &str = "R-TRICK-005";

/// Whether a structurally valid intent changed the strict game or was retained
/// only as an attempted tabletop action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum HistoryActionDisposition {
    Accepted,
    /// Did not change strict `Game`, but identity/ownership/card conservation
    /// passed the hard structural gate before the attempt entered history.
    AttemptedStructurallyValid,
}

/// Exact cards one detector could prove were held immediately before an action.
/// An empty set represents no private knowledge, not a known-empty hand.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ActionKnowledge {
    pub known_held_cards_before: Vec<u8>,
}

/// One accepted or attempted game action in stable logical history order.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct AuditedGameAction {
    pub event_id: EventId,
    pub sequence: u64,
    pub round_id: u32,
    pub actor: PrincipalId,
    pub disposition: HistoryActionDisposition,
    pub action: GameActionWire,
    /// Suit led before this action (`0..4`), or `None` for a lead/bid.
    pub led_suit: Option<u8>,
    pub knowledge_at_action: ActionKnowledge,
}

/// Complete remaining hand made public at a round boundary.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct HandDisclosure {
    pub evidence_id: EventId,
    pub sequence: u64,
    pub round_id: u32,
    pub player: PrincipalId,
    pub complete_remaining_hand: Vec<u8>,
}

/// Why the audit can confirm an earlier violation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum FindingConfidence {
    ImmediateHeldCard,
    DelayedPublicPlay,
    RoundEndDisclosure,
}

/// Immutable evidence for a confirmed violation.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct RuleFinding {
    /// Canonical ID depends on the rule and offending action, not detector.
    pub finding_id: FindingId,
    pub rule_id: String,
    pub offending_action_id: EventId,
    pub offending_disposition: HistoryActionDisposition,
    /// Action or disclosure that made the proof available.
    pub revealing_evidence_id: EventId,
    pub revealed_card: u8,
    /// Client/principal whose rule engine first submitted this finding record.
    pub detector: PrincipalId,
    pub confidence: FindingConfidence,
}

/// Player-authored request to evaluate one historical action. It cannot supply
/// a rule, confidence, or revealing card/evidence.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ManualAccusation {
    pub accusation_id: AccusationId,
    pub detector: PrincipalId,
    pub offending_action_id: EventId,
}

/// Stable reason a manual accusation did not confirm a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum UnfoundedReason {
    UnknownAction,
    ActionNotViolation,
    InsufficientEvidence,
}

/// Immutable outcome of one accusation attempt.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum AccusationOutcome {
    Confirmed { finding_id: FindingId },
    Unfounded { reason: UnfoundedReason },
}

/// Accusation plus its derived, replay-stable outcome.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct AccusationRecord {
    pub accusation: ManualAccusation,
    pub outcome: AccusationOutcome,
}

/// Append-only audit state. Strict game state remains elsewhere and valid.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetrospectiveAudit {
    actions: Vec<AuditedGameAction>,
    disclosures: Vec<HandDisclosure>,
    findings: Vec<RuleFinding>,
    accusations: Vec<AccusationRecord>,
}

impl RetrospectiveAudit {
    /// Append one action after validating IDs, cards, suits, and global order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate IDs, non-increasing sequence, or malformed card data.
    pub fn append_action(&mut self, action: AuditedGameAction) -> Result<(), AuditError> {
        if !action.event_id.validate() || !action.actor.validate() {
            return Err(AuditError::InvalidIdentifier);
        }
        self.validate_event(&action.event_id, action.sequence)?;
        validate_cards(&action.knowledge_at_action.known_held_cards_before)?;
        match action.action {
            GameActionWire::Bid { tricks } if tricks > 7 || action.led_suit.is_some() => {
                return Err(AuditError::InvalidAction);
            }
            GameActionWire::Play { card }
                if card >= 52 || action.led_suit.is_some_and(|suit| suit >= 4) =>
            {
                return Err(AuditError::InvalidAction);
            }
            _ => {}
        }
        self.actions.push(action);
        Ok(())
    }

    /// Append a complete public remaining-hand disclosure.
    ///
    /// # Errors
    ///
    /// Rejects duplicate IDs, non-increasing sequence, or malformed cards.
    pub fn append_disclosure(&mut self, disclosure: HandDisclosure) -> Result<(), AuditError> {
        if !disclosure.evidence_id.validate() || !disclosure.player.validate() {
            return Err(AuditError::InvalidIdentifier);
        }
        self.validate_event(&disclosure.evidence_id, disclosure.sequence)?;
        validate_cards(&disclosure.complete_remaining_hand)?;
        self.disclosures.push(disclosure);
        Ok(())
    }

    /// Re-run every registered rule against all currently available knowledge,
    /// retaining only newly confirmed canonical finding IDs.
    ///
    /// # Errors
    ///
    /// Returns an internal identifier error only if the fixed finding encoding
    /// unexpectedly violates the public identifier refinement.
    pub fn audit(&mut self, detector: &PrincipalId) -> Result<Vec<RuleFinding>, AuditError> {
        if !detector.validate() {
            return Err(AuditError::InvalidIdentifier);
        }
        let candidates = self
            .actions
            .iter()
            .map(|action| self.follow_suit_finding(action, detector))
            .collect::<Result<Vec<_>, _>>()?;
        let mut emitted = Vec::new();
        for finding in candidates.into_iter().flatten() {
            if self
                .findings
                .iter()
                .any(|existing| existing.finding_id == finding.finding_id)
            {
                continue;
            }
            self.findings.push(finding.clone());
            emitted.push(finding);
        }
        Ok(emitted)
    }

    /// Evaluate a manual accusation using the same rules/evidence as automatic
    /// auditing. Duplicate accusation IDs replay their original result.
    ///
    /// # Errors
    ///
    /// Rejects a reused accusation ID with different content or an internal
    /// finding-ID construction failure.
    pub fn accuse(&mut self, accusation: ManualAccusation) -> Result<AccusationRecord, AuditError> {
        if !accusation.accusation_id.validate()
            || !accusation.detector.validate()
            || !accusation.offending_action_id.validate()
        {
            return Err(AuditError::InvalidIdentifier);
        }
        if let Some(existing) = self
            .accusations
            .iter()
            .find(|record| record.accusation.accusation_id == accusation.accusation_id)
        {
            return if existing.accusation == accusation {
                Ok(existing.clone())
            } else {
                Err(AuditError::ConflictingAccusationId)
            };
        }
        self.audit(&accusation.detector)?;
        let outcome = if let Some(finding) = self
            .findings
            .iter()
            .find(|finding| finding.offending_action_id == accusation.offending_action_id)
        {
            AccusationOutcome::Confirmed {
                finding_id: finding.finding_id.clone(),
            }
        } else {
            AccusationOutcome::Unfounded {
                reason: self.unfounded_reason(&accusation.offending_action_id),
            }
        };
        let record = AccusationRecord {
            accusation,
            outcome,
        };
        self.accusations.push(record.clone());
        Ok(record)
    }

    /// Borrow actions in canonical history order.
    #[must_use]
    pub fn actions(&self) -> &[AuditedGameAction] {
        &self.actions
    }

    /// Borrow immutable confirmed findings.
    #[must_use]
    pub fn findings(&self) -> &[RuleFinding] {
        &self.findings
    }

    /// Borrow immutable accusation decisions.
    #[must_use]
    pub fn accusations(&self) -> &[AccusationRecord] {
        &self.accusations
    }

    fn validate_event(&self, event_id: &EventId, sequence: u64) -> Result<(), AuditError> {
        let last = self
            .actions
            .iter()
            .map(|action| action.sequence)
            .chain(self.disclosures.iter().map(|evidence| evidence.sequence))
            .max();
        if last.is_some_and(|last| sequence <= last) {
            return Err(AuditError::NonIncreasingSequence);
        }
        if self
            .actions
            .iter()
            .any(|action| &action.event_id == event_id)
            || self
                .disclosures
                .iter()
                .any(|evidence| &evidence.evidence_id == event_id)
        {
            return Err(AuditError::DuplicateEventId);
        }
        Ok(())
    }

    fn follow_suit_finding(
        &self,
        action: &AuditedGameAction,
        detector: &PrincipalId,
    ) -> Result<Option<RuleFinding>, AuditError> {
        let GameActionWire::Play { card } = action.action else {
            return Ok(None);
        };
        let Some(led_suit) = action.led_suit else {
            return Ok(None);
        };
        if card_suit(card) == led_suit {
            return Ok(None);
        }

        let immediate = action
            .knowledge_at_action
            .known_held_cards_before
            .iter()
            .copied()
            .find(|known| card_suit(*known) == led_suit)
            .map(|revealed| {
                (
                    action.sequence,
                    action.event_id.clone(),
                    revealed,
                    FindingConfidence::ImmediateHeldCard,
                )
            });
        let later_play = self.actions.iter().find_map(|candidate| {
            if candidate.sequence <= action.sequence
                || candidate.round_id != action.round_id
                || candidate.actor != action.actor
            {
                return None;
            }
            let GameActionWire::Play { card } = candidate.action else {
                return None;
            };
            (card_suit(card) == led_suit).then(|| {
                (
                    candidate.sequence,
                    candidate.event_id.clone(),
                    card,
                    FindingConfidence::DelayedPublicPlay,
                )
            })
        });
        let disclosure = self.disclosures.iter().find_map(|evidence| {
            if evidence.sequence <= action.sequence
                || evidence.round_id != action.round_id
                || evidence.player != action.actor
            {
                return None;
            }
            evidence
                .complete_remaining_hand
                .iter()
                .copied()
                .find(|known| card_suit(*known) == led_suit)
                .map(|revealed| {
                    (
                        evidence.sequence,
                        evidence.evidence_id.clone(),
                        revealed,
                        FindingConfidence::RoundEndDisclosure,
                    )
                })
        });
        let evidence = immediate.or(match (later_play, disclosure) {
            (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        });
        let Some((_, revealing_evidence_id, revealed_card, confidence)) = evidence else {
            return Ok(None);
        };
        Ok(Some(RuleFinding {
            finding_id: finding_id(&action.event_id)?,
            rule_id: FOLLOW_SUIT_RULE_ID.to_owned(),
            offending_action_id: action.event_id.clone(),
            offending_disposition: action.disposition,
            revealing_evidence_id,
            revealed_card,
            detector: detector.clone(),
            confidence,
        }))
    }

    fn unfounded_reason(&self, offending_action_id: &EventId) -> UnfoundedReason {
        let Some(action) = self
            .actions
            .iter()
            .find(|action| &action.event_id == offending_action_id)
        else {
            return UnfoundedReason::UnknownAction;
        };
        match action.action {
            GameActionWire::Play { card }
                if action
                    .led_suit
                    .is_some_and(|led_suit| card_suit(card) != led_suit) =>
            {
                UnfoundedReason::InsufficientEvidence
            }
            GameActionWire::Bid { .. } | GameActionWire::Play { .. } => {
                UnfoundedReason::ActionNotViolation
            }
        }
    }
}

/// Stable append/audit failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditError {
    DuplicateEventId,
    NonIncreasingSequence,
    InvalidAction,
    InvalidCards,
    InvalidIdentifier,
    InvalidFindingId,
    ConflictingAccusationId,
}

impl fmt::Display for AuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DuplicateEventId => "audit history event identifier is duplicated",
            Self::NonIncreasingSequence => "audit history sequence is not strictly increasing",
            Self::InvalidAction => "audited game action is malformed",
            Self::InvalidCards => "audit knowledge contains an invalid or duplicate card",
            Self::InvalidIdentifier => "audit history contains an invalid stable identifier",
            Self::InvalidFindingId => "derived finding identifier is invalid",
            Self::ConflictingAccusationId => "accusation identifier was reused with new content",
        })
    }
}

impl std::error::Error for AuditError {}

fn validate_cards(cards: &[u8]) -> Result<(), AuditError> {
    for (index, card) in cards.iter().enumerate() {
        if *card >= 52 || cards[..index].contains(card) {
            return Err(AuditError::InvalidCards);
        }
    }
    Ok(())
}

const fn card_suit(card: u8) -> u8 {
    card / 13
}

fn finding_id(offending: &EventId) -> Result<FindingId, AuditError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"poche-rule-finding-v1\0");
    hasher.update(FOLLOW_SUIT_RULE_ID.as_bytes());
    hasher.update(b"\0");
    hasher.update(offending.as_str().as_bytes());
    let digest = hasher.finalize();
    let mut suffix = String::with_capacity(24);
    for byte in &digest.as_bytes()[..12] {
        use core::fmt::Write;
        write!(&mut suffix, "{byte:02x}").map_err(|_| AuditError::InvalidFindingId)?;
    }
    FindingId::new(format!("finding-{suffix}")).map_err(|_| AuditError::InvalidFindingId)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id<T>(
        value: &str,
        constructor: impl FnOnce(String) -> Result<T, poche_protocol::IdentifierError>,
    ) -> T {
        constructor(value.to_owned()).unwrap()
    }

    fn player(value: &str) -> PrincipalId {
        id(value, PrincipalId::new)
    }

    fn action(
        event: &str,
        sequence: u64,
        card: u8,
        led_suit: Option<u8>,
        known: &[u8],
        disposition: HistoryActionDisposition,
    ) -> AuditedGameAction {
        AuditedGameAction {
            event_id: id(event, EventId::new),
            sequence,
            round_id: 7,
            actor: player("john"),
            disposition,
            action: GameActionWire::Play { card },
            led_suit,
            knowledge_at_action: ActionKnowledge {
                known_held_cards_before: known.to_vec(),
            },
        }
    }

    #[test]
    fn retrospective_immediate_delayed_round_end_and_duplicate_corpus() {
        let detector = player("alice-device");

        let mut immediate = RetrospectiveAudit::default();
        immediate
            .append_action(action(
                "off-immediate",
                1,
                26,
                Some(0),
                &[26, 3],
                HistoryActionDisposition::AttemptedStructurallyValid,
            ))
            .unwrap();
        let found = immediate.audit(&detector).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].confidence, FindingConfidence::ImmediateHeldCard);
        assert_eq!(found[0].revealed_card, 3);
        assert!(immediate.audit(&detector).unwrap().is_empty());
        assert_eq!(immediate.findings().len(), 1);

        let mut delayed = RetrospectiveAudit::default();
        delayed
            .append_action(action(
                "off-delayed",
                1,
                26,
                Some(0),
                &[],
                HistoryActionDisposition::Accepted,
            ))
            .unwrap();
        assert!(delayed.audit(&detector).unwrap().is_empty());
        delayed
            .append_action(action(
                "reveal-delayed",
                2,
                4,
                Some(1),
                &[],
                HistoryActionDisposition::Accepted,
            ))
            .unwrap();
        let found = delayed.audit(&detector).unwrap();
        assert_eq!(found[0].confidence, FindingConfidence::DelayedPublicPlay);
        assert_eq!(found[0].revealing_evidence_id.as_str(), "reveal-delayed");

        let mut round_end = RetrospectiveAudit::default();
        round_end
            .append_action(action(
                "off-round-end",
                1,
                26,
                Some(0),
                &[],
                HistoryActionDisposition::AttemptedStructurallyValid,
            ))
            .unwrap();
        round_end
            .append_disclosure(HandDisclosure {
                evidence_id: id("round-disclosure", EventId::new),
                sequence: 2,
                round_id: 7,
                player: player("john"),
                complete_remaining_hand: vec![5, 40],
            })
            .unwrap();
        let found = round_end.audit(&detector).unwrap();
        assert_eq!(found[0].confidence, FindingConfidence::RoundEndDisclosure);
        assert_eq!(found[0].revealing_evidence_id.as_str(), "round-disclosure");

        let expected = found[0].clone();
        let mut replay = RetrospectiveAudit::default();
        replay.append_action(round_end.actions[0].clone()).unwrap();
        replay
            .append_disclosure(round_end.disclosures[0].clone())
            .unwrap();
        assert_eq!(replay.audit(&detector).unwrap(), vec![expected]);
        assert_eq!(round_end.actions()[0].event_id.as_str(), "off-round-end");
    }

    #[test]
    fn retrospective_manual_accusation_cannot_forge_confirmation() {
        let mut audit = RetrospectiveAudit::default();
        audit
            .append_action(action(
                "legal-follow",
                1,
                3,
                Some(0),
                &[3],
                HistoryActionDisposition::Accepted,
            ))
            .unwrap();
        audit
            .append_action(action(
                "unproven-off-suit",
                2,
                27,
                Some(0),
                &[],
                HistoryActionDisposition::AttemptedStructurallyValid,
            ))
            .unwrap();
        let detector = player("alice-device");
        let legal = audit
            .accuse(ManualAccusation {
                accusation_id: id("accuse-legal", AccusationId::new),
                detector: detector.clone(),
                offending_action_id: id("legal-follow", EventId::new),
            })
            .unwrap();
        assert_eq!(
            legal.outcome,
            AccusationOutcome::Unfounded {
                reason: UnfoundedReason::ActionNotViolation
            }
        );
        let unproven = audit
            .accuse(ManualAccusation {
                accusation_id: id("accuse-unproven", AccusationId::new),
                detector: detector.clone(),
                offending_action_id: id("unproven-off-suit", EventId::new),
            })
            .unwrap();
        assert_eq!(
            unproven.outcome,
            AccusationOutcome::Unfounded {
                reason: UnfoundedReason::InsufficientEvidence
            }
        );
        let unknown = ManualAccusation {
            accusation_id: id("accuse-unknown", AccusationId::new),
            detector,
            offending_action_id: id("made-up-event", EventId::new),
        };
        assert_eq!(
            audit.accuse(unknown.clone()).unwrap().outcome,
            AccusationOutcome::Unfounded {
                reason: UnfoundedReason::UnknownAction
            }
        );
        assert_eq!(
            audit.accuse(unknown.clone()).unwrap(),
            audit.accuse(unknown).unwrap()
        );
        assert!(audit.findings().is_empty());
    }
}
