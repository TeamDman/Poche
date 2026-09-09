//! Authority-side physical identities, independent of projection ordering.
//! Do not expose the face-to-handle mapping to unauthorized viewers.

use crate::CardFace;

/// Stable during one round, including changes between hand/deck/play.
/// Unlike a scene ordinal this does not change when another card is removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysicalCardId([u8; 32]);

/// Full-pose reducer keyed by durable round identity rather than scene ordinal.
pub type PhysicalCardManipulation = crate::CardManipulation<PhysicalCardId>;

impl PhysicalCardId {
    /// Opaque bytes suitable for a versioned transport envelope.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Authority-only mapping. Deliberately has no Debug/serialization implementation.
/// Knowledge of an ID conveys neither its face nor permission to move/reveal it.
pub struct PhysicalDeckIdentity {
    ids: [PhysicalCardId; 52],
}

impl PhysicalDeckIdentity {
    /// Allocate the mapping from independent cryptographic entropy, NOT the
    /// game seed, invitation, public transcript hash, name, or device identity.
    /// Persist/replicate this mapping only through the entitled recovery path.
    /// `round_epoch` also separates rounds if the same secret is retained.
    #[must_use]
    pub fn new(secret: &[u8; 32], round_epoch: u64) -> Self {
        Self {
            ids: std::array::from_fn(|face| {
                let mut hash = blake3::Hasher::new_keyed(secret);
                hash.update(b"poche.physical-card.v1\0");
                hash.update(&round_epoch.to_le_bytes());
                hash.update(&[u8::try_from(face).expect("52-card deck")]);
                PhysicalCardId(*hash.finalize().as_bytes())
            }),
        }
    }

    /// Called by an authority that already knows this card's face. Viewers
    /// receive only the selected handle and independently authorized fields.
    #[must_use]
    pub fn card(&self, face: CardFace) -> PhysicalCardId {
        self.ids[usize::from(face.code())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_survive_hand_reordering_but_not_new_rounds_or_secrets() {
        let identities = PhysicalDeckIdentity::new(&[7; 32], 4);
        let hand = [
            CardFace::new(0).unwrap(),
            CardFace::new(13).unwrap(),
            CardFace::new(51).unwrap(),
        ];
        let before = hand.map(|face| identities.card(face));
        let mut motion =
            PhysicalCardManipulation::new(before[2], crate::ManipulationPose::default(), 1)
                .unwrap();
        let pose = crate::ManipulationPose {
            translation: crate::Point3Mm::new(100, 200, 300),
            rotation: crate::RotationMilliDegrees::default(),
        };
        motion.update(1, 1, pose, false).unwrap();
        let remaining = [hand[2], hand[1]];
        assert_eq!(
            remaining.map(|face| identities.card(face)),
            [before[2], before[1]]
        );
        assert_eq!(motion.card(), identities.card(remaining[0]));
        assert_eq!(motion.pose(), pose);
        let next_round = PhysicalDeckIdentity::new(&[7; 32], 5);
        let other_secret = PhysicalDeckIdentity::new(&[8; 32], 4);
        let restored = PhysicalDeckIdentity::new(&[7; 32], 4);
        let mut unique = std::collections::HashSet::new();
        for code in 0..52 {
            let face = CardFace::new(code).unwrap();
            let id = identities.card(face);
            assert!(unique.insert(id));
            assert_ne!(id, next_round.card(face));
            assert_ne!(id, other_secret.card(face));
            assert_eq!(id, restored.card(face));
        }
    }
}
