// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Runtime seam for submitting a typed accusation to the pure audit engine.

use poche_session::{AccusationRecord, AuditError, ManualAccusation, RetrospectiveAudit};

/// Submit one accusation without accepting caller-supplied proof or finding
/// status. The pure session audit derives the immutable outcome.
///
/// # Errors
///
/// Returns stable audit errors for a conflicting accusation ID or invalid
/// derived finding identifier.
pub fn process_accusation(
    audit: &mut RetrospectiveAudit,
    accusation: ManualAccusation,
) -> Result<AccusationRecord, AuditError> {
    audit.accuse(accusation)
}

#[cfg(test)]
mod tests {
    use poche_protocol::{AccusationId, EventId, GameActionWire, PrincipalId};
    use poche_session::{
        AccusationOutcome, ActionKnowledge, AuditedGameAction, HistoryActionDisposition,
        ManualAccusation, RetrospectiveAudit,
    };

    use super::*;

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).unwrap()
    }

    #[test]
    fn accusation_runtime_derives_confirmation_and_replays_idempotently() {
        let mut audit = RetrospectiveAudit::default();
        audit
            .append_action(AuditedGameAction {
                event_id: EventId::new("offending-action").unwrap(),
                sequence: 1,
                round_id: 1,
                actor: principal("john"),
                disposition: HistoryActionDisposition::AttemptedStructurallyValid,
                action: GameActionWire::Play { card: 26 },
                led_suit: Some(0),
                knowledge_at_action: ActionKnowledge {
                    known_held_cards_before: Vec::new(),
                },
            })
            .unwrap();
        audit
            .append_action(AuditedGameAction {
                event_id: EventId::new("revealing-action").unwrap(),
                sequence: 2,
                round_id: 1,
                actor: principal("john"),
                disposition: HistoryActionDisposition::Accepted,
                action: GameActionWire::Play { card: 4 },
                led_suit: Some(1),
                knowledge_at_action: ActionKnowledge {
                    known_held_cards_before: Vec::new(),
                },
            })
            .unwrap();
        let accusation = ManualAccusation {
            accusation_id: AccusationId::new("accusation-1").unwrap(),
            detector: principal("alice-device"),
            offending_action_id: EventId::new("offending-action").unwrap(),
        };
        let first = process_accusation(&mut audit, accusation.clone()).unwrap();
        let second = process_accusation(&mut audit, accusation).unwrap();
        assert_eq!(first, second);
        let AccusationOutcome::Confirmed { finding_id } = first.outcome else {
            panic!("later public play should confirm the accusation")
        };
        assert_eq!(audit.findings()[0].finding_id, finding_id);
        assert_eq!(audit.accusations().len(), 1);
    }
}
