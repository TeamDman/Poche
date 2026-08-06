// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use super::*;

struct Fixture {
    rng: ChaCha20Rng,
    context: RoundContext,
    secrets: Vec<PlayerSecret>,
    keys: Vec<KeyRecord>,
    roster: VerifiedRoster,
    records: Vec<ShuffleRecord>,
    deck: VerifiedDeck,
}

fn fixture(seed: u8) -> Fixture {
    let players = ["alice", "bob", "carol"]
        .into_iter()
        .map(PlayerId::new)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let context = RoundContext::new("room-alpha", [7; 32], 4, 9, players).unwrap();
    let mut rng = ChaCha20Rng::from_seed([seed; 32]);
    let mut secrets = Vec::new();
    let mut keys = Vec::new();
    for player in context.roster() {
        let (secret, key) = generate_key(&mut rng, &context, player).unwrap();
        secrets.push(secret);
        keys.push(key);
    }
    let keys = keys
        .iter()
        .map(|record| KeyRecord::from_canonical_bytes(&record.canonical_bytes()).unwrap())
        .collect::<Vec<_>>();
    let roster = verify_roster(&context, &keys).unwrap();
    let (records, deck) = create_shuffle_transcript(&mut rng, &roster).unwrap();
    Fixture {
        rng,
        context,
        secrets,
        keys,
        roster,
        records,
        deck,
    }
}

fn shares(fixture: &mut Fixture, position: Position) -> Vec<RevealShareRecord> {
    fixture
        .secrets
        .iter()
        .map(|secret| {
            create_reveal_share(
                &mut fixture.rng,
                &fixture.roster,
                &fixture.deck,
                secret,
                position,
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn honest_private_deal_public_reveal_and_complete_audit() {
    let mut fixture = fixture(11);
    let decoded_records = fixture
        .records
        .iter()
        .map(|record| ShuffleRecord::from_canonical_bytes(&record.canonical_bytes()).unwrap())
        .collect::<Vec<_>>();
    let verified_again = verify_shuffle_transcript(&fixture.roster, &decoded_records).unwrap();
    assert_eq!(
        verified_again.transcript_digest(),
        fixture.deck.transcript_digest()
    );

    let assignment = Assignment {
        holder: fixture.context.roster()[0].clone(),
        position: Position::new(0).unwrap(),
    };
    let private_shares = shares(&mut fixture, assignment.position);
    let (holder, receipt) =
        deal_private(&fixture.roster, &fixture.deck, &assignment, &private_shares).unwrap();
    assert_eq!(holder.holder(), &assignment.holder);
    assert!(format!("{holder:?}").contains("<holder-only>"));

    let projection = PublicRoundProjection::new(
        &fixture.roster,
        &fixture.deck,
        vec![assignment.clone()],
        vec![receipt],
    )
    .unwrap();
    let debug = format!("{projection:?}");
    assert!(!debug.contains("token"));
    assert!(!debug.contains("proof"));
    assert!(!debug.contains("card:"));

    let reveal = create_public_reveal(
        &fixture.roster,
        &fixture.deck,
        assignment.position,
        private_shares,
    )
    .unwrap();
    let reveal = PublicRevealEvidence::from_canonical_bytes(&reveal.canonical_bytes()).unwrap();
    assert_eq!(
        verify_public_reveal(&fixture.roster, &fixture.deck, &reveal),
        Ok(holder.card())
    );

    let mut all_reveals = Vec::with_capacity(DECK_SIZE);
    all_reveals.push(reveal);
    for index in 1..DECK_SIZE {
        let position = Position::new(index).unwrap();
        let reveal_shares = shares(&mut fixture, position);
        all_reveals.push(
            create_public_reveal(&fixture.roster, &fixture.deck, position, reveal_shares).unwrap(),
        );
    }
    let cards = audit_complete_round(&fixture.roster, &fixture.deck, &all_reveals).unwrap();
    let unique = cards.into_iter().map(CardId::code).collect::<BTreeSet<_>>();
    assert_eq!(unique, (0_u8..52).collect());

    let mut duplicate_identity = all_reveals.clone();
    duplicate_identity[1].card = duplicate_identity[0].card;
    duplicate_identity[1].evidence_digest = public_reveal_digest(
        duplicate_identity[1].position,
        duplicate_identity[1].card,
        &duplicate_identity[1].shares,
    );
    assert_eq!(
        audit_complete_round(&fixture.roster, &fixture.deck, &duplicate_identity),
        Err(ProtocolError::RevealFailed)
    );

    let mut vector_bytes = Vec::new();
    vector_bytes.extend_from_slice(&fixture.context.digest());
    vector_bytes.extend_from_slice(&fixture.roster.transcript_digest());
    vector_bytes.extend_from_slice(&fixture.deck.transcript_digest());
    for reveal in &all_reveals {
        vector_bytes.extend_from_slice(&reveal.evidence_digest);
    }
    assert_eq!(
        hex(*blake3::hash(&vector_bytes).as_bytes()),
        "64a237dfcec61a71ec16bab96a7219b4a5079ab123395887d08a4affc6ab17a6"
    );
}

#[test]
fn tamper_replay_wrong_card_duplicate_and_incomplete_collusion_fail_closed() {
    let mut fixture = fixture(23);

    let mut tampered = fixture.records.clone();
    tampered[1].proof[17] ^= 0x80;
    tampered[1].digest = *blake3::hash(&tampered[1].canonical_bytes()).as_bytes();
    assert!(matches!(
        verify_shuffle_transcript(&fixture.roster, &tampered),
        Err(ProtocolError::InvalidEncoding("shuffle proof")
            | ProtocolError::InvalidProof("shuffle"))
    ));

    let position0 = Position::new(0).unwrap();
    let position1 = Position::new(1).unwrap();
    let shares0 = shares(&mut fixture, position0);
    assert!(matches!(
        create_public_reveal(&fixture.roster, &fixture.deck, position1, shares0.clone()),
        Err(ProtocolError::UnexpectedContribution("reveal share"))
    ));
    let mut forged_position = shares0.clone();
    for share in &mut forged_position {
        share.position = position1;
    }
    assert!(matches!(
        create_public_reveal(&fixture.roster, &fixture.deck, position1, forged_position),
        Err(ProtocolError::InvalidProof("reveal"))
    ));

    // Even all but one colluding players cannot cross the wrapper's complete-set gate.
    assert!(matches!(
        deal_private(
            &fixture.roster,
            &fixture.deck,
            &Assignment {
                holder: fixture.context.roster()[0].clone(),
                position: position0,
            },
            &shares0[..shares0.len() - 1],
        ),
        Err(ProtocolError::UnexpectedContribution("reveal share"))
    ));

    let duplicate = [
        Assignment {
            holder: fixture.context.roster()[0].clone(),
            position: position0,
        },
        Assignment {
            holder: fixture.context.roster()[1].clone(),
            position: position0,
        },
    ];
    assert_eq!(
        validate_assignments(&fixture.roster, &duplicate),
        Err(ProtocolError::DuplicatePosition)
    );

    // Context replay fails already at key ownership verification.
    let replay_context = RoundContext::new(
        fixture.context.room(),
        fixture.context.session(),
        fixture.context.membership_epoch(),
        fixture.context.round() + 1,
        fixture.context.roster().to_vec(),
    )
    .unwrap();
    assert!(matches!(
        verify_roster(&replay_context, &fixture.keys),
        Err(ProtocolError::InvalidProof("ownership"))
    ));
}

#[test]
fn context_and_artifact_bounds_are_exact() {
    let alice = PlayerId::new("alice").unwrap();
    let bob = PlayerId::new("bob").unwrap();
    assert!(RoundContext::new("room", [0; 32], 0, 0, vec![bob.clone(), alice.clone()]).is_err());
    assert!(RoundContext::new("room", [0; 32], 0, 0, vec![alice.clone(), alice]).is_err());
    assert!(Position::new(52).is_err());
    assert!(PlayerId::new("x".repeat(MAX_PLAYER_ID_BYTES + 1)).is_err());
    assert!(
        RoundContext::new(
            "x".repeat(MAX_ROOM_BYTES + 1),
            [0; 32],
            0,
            0,
            vec![PlayerId::new("a").unwrap(), PlayerId::new("b").unwrap()],
        )
        .is_err()
    );
}
