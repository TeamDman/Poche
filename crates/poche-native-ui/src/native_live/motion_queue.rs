//! Coalesce unsigned, unsent motion intent only. Rules actions are barriers.
use super::NativeWorkerCommand;
use std::{sync::mpsc, time::Duration};

pub(super) const COMMAND_CAPACITY: usize = 8;

impl NativeWorkerCommand {
    pub(super) fn absorb_pose(&mut self, newer: &Self) -> bool {
        match (self, newer) {
            (
                Self::Pose {
                    ui_pose_id,
                    submitted_at,
                    card_id,
                    claim,
                    position_mm,
                    rotation_millidegrees,
                },
                Self::Pose {
                    ui_pose_id: next_ui_pose_id,
                    submitted_at: next_submitted_at,
                    card_id: next_id,
                    claim: next_claim,
                    position_mm: next_position,
                    rotation_millidegrees: next_rotation,
                },
            ) if card_id == next_id => {
                *ui_pose_id = *next_ui_pose_id;
                *submitted_at = *next_submitted_at;
                *claim |= *next_claim;
                *position_mm = *next_position;
                *rotation_millidegrees = *next_rotation;
                true
            }
            _ => false,
        }
    }
}

#[derive(Default)]
pub(super) struct CommandInbox {
    // A consumed barrier is the next command, never dropped or merged across.
    held: Option<NativeWorkerCommand>,
}

impl CommandInbox {
    pub(super) fn receive(
        &mut self,
        receiver: &mpsc::Receiver<NativeWorkerCommand>,
        timeout: Duration,
    ) -> Result<NativeWorkerCommand, mpsc::RecvTimeoutError> {
        let mut next = match self.held.take() {
            Some(command) => command,
            None => receiver.recv_timeout(timeout)?,
        };
        if !matches!(next, NativeWorkerCommand::Pose { .. }) {
            return Ok(next);
        }
        let mut superseded = 0;
        // Even a producer refilling concurrently cannot starve dispatch/reads.
        // One initial sample plus at most seven further samples per dispatch.
        for _ in 1..COMMAND_CAPACITY {
            let Ok(newer) = receiver.try_recv() else {
                break;
            };
            if !next.absorb_pose(&newer) {
                self.held = Some(newer);
                break;
            }
            superseded += 1;
        }
        if cfg!(feature = "input-probe") && superseded > 0 {
            eprintln!(
                "poche native motion: {superseded} older queued samples superseded before dispatch"
            );
        }
        if superseded > 0 {
            tracing::trace!(
                target: "poche_latency",
                event = "ui_pose_samples_coalesced",
                superseded,
            );
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_protocol::CommandId;

    fn pose(card: &str, claim: bool, n: i32) -> NativeWorkerCommand {
        NativeWorkerCommand::Pose {
            ui_pose_id: u64::try_from(n).unwrap_or(0),
            submitted_at: std::time::Instant::now(),
            card_id: card.to_owned(),
            claim,
            position_mm: [n, n + 1, n + 2],
            rotation_millidegrees: [n + 3, n + 4, n + 5],
        }
    }
    fn assert_pose(command: NativeWorkerCommand, card: &str, expected_claim: bool, n: i32) {
        let NativeWorkerCommand::Pose {
            card_id,
            claim,
            position_mm,
            rotation_millidegrees,
            ..
        } = command
        else {
            panic!("expected a pose");
        };
        assert_eq!(card_id, card);
        assert_eq!(claim, expected_claim);
        assert_eq!(position_mm, [n, n + 1, n + 2]);
        assert_eq!(rotation_millidegrees, [n + 3, n + 4, n + 5]);
    }

    #[test]
    fn absorbs_newest_complete_pose_and_preserves_unsent_claim() {
        for (first_claim, last_claim) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let mut first = pose("a", first_claim, 1);
            assert!(first.absorb_pose(&pose("a", last_claim, 9)));
            assert_pose(first, "a", first_claim || last_claim, 9);
        }
        let mut first = pose("a", true, 1);
        assert!(!first.absorb_pose(&pose("b", false, 9)));
        assert_pose(first, "a", true, 1);
    }

    #[test]
    fn inbox_preserves_other_cards_and_frozen_invoke_barriers() {
        let observation = crate::tests::live_play_observation();
        let expected = observation.clone();
        let id = CommandId::new("frozen-motion-barrier").unwrap();
        let invoke = NativeWorkerCommand::Invoke {
            observation,
            action_id: "play-2-clubs".into(),
            command_id: id.clone(),
        };
        let (sender, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        for command in [
            pose("a", true, 1),
            pose("a", false, 2),
            pose("b", true, 3),
            pose("a", false, 4),
            invoke,
            pose("a", false, 6),
        ] {
            sender.try_send(command).ok().unwrap();
        }
        drop(sender);
        let mut inbox = CommandInbox::default();
        assert_pose(
            inbox.receive(&receiver, Duration::ZERO).unwrap(),
            "a",
            true,
            2,
        );
        assert_pose(
            inbox.receive(&receiver, Duration::ZERO).unwrap(),
            "b",
            true,
            3,
        );
        assert_pose(
            inbox.receive(&receiver, Duration::ZERO).unwrap(),
            "a",
            false,
            4,
        );
        let NativeWorkerCommand::Invoke {
            observation,
            action_id,
            command_id,
        } = inbox.receive(&receiver, Duration::ZERO).unwrap()
        else {
            panic!("action was lost or crossed");
        };
        assert_eq!(observation, expected);
        assert_eq!(action_id, "play-2-clubs");
        assert_eq!(command_id, id);
        assert_pose(
            inbox.receive(&receiver, Duration::ZERO).unwrap(),
            "a",
            false,
            6,
        );
        assert!(
            matches!(
                inbox.receive(&receiver, Duration::ZERO),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ),
            "no duplicate action"
        );
    }

    #[test]
    fn constant_availability_cannot_exceed_the_finite_dispatch_budget() {
        // More available data than the production channel can hold at once
        // models a producer refilling it continuously, without a timing race.
        let (sender, receiver) = mpsc::channel();
        for n in 0..20 {
            sender.send(pose("a", n == 0, n)).ok().unwrap();
        }
        drop(sender);
        let mut inbox = CommandInbox::default();
        assert_pose(
            inbox.receive(&receiver, Duration::ZERO).unwrap(),
            "a",
            true,
            7,
        );
        assert_pose(
            inbox.receive(&receiver, Duration::ZERO).unwrap(),
            "a",
            false,
            15,
        );
        assert_pose(
            inbox.receive(&receiver, Duration::ZERO).unwrap(),
            "a",
            false,
            19,
        );
        assert!(matches!(
            inbox.receive(&receiver, Duration::ZERO),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn empty_inbox_times_out_without_inventing_work() {
        let (sender, receiver) = mpsc::channel();
        let mut inbox = CommandInbox::default();
        assert!(matches!(
            inbox.receive(&receiver, Duration::ZERO),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        drop(sender);
        assert!(matches!(
            inbox.receive(&receiver, Duration::ZERO),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}
