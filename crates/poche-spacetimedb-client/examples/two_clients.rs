// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use poche_spacetimedb_client::{ClientConfig, PocheClient};
use std::time::{Duration, Instant, SystemTime};

const TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (alice, bob) = connect_disposable_clients()?;
    let _alice_cleanup = LeaveOnDrop(&alice);
    let _bob_cleanup = LeaveOnDrop(&bob);

    let capability = alice.create_room("Alice".into())?;
    wait_for("Alice room creation", || !alice.snapshot().rooms.is_empty())?;
    bob.join_room(capability.join_code.clone(), "Bob".into())?;
    wait_for("Bob room join", || bob.snapshot().members.len() == 2)?;

    alice.take_seat(capability.room_id.clone(), 0)?;
    bob.take_seat(capability.room_id.clone(), 1)?;
    wait_for("seats and physical money", || {
        let alice = alice.snapshot();
        let bob = bob.snapshot();
        alice.own_seat() == Some(0)
            && bob.own_seat() == Some(1)
            && alice.coins.len() == 400
            && bob.coins.len() == 400
    })?;
    assert!(
        alice.snapshot().hand.is_empty(),
        "the first deal must wait for real antes"
    );
    pay_ante(&alice, &capability.room_id)?;
    wait_for("Alice ante visible to Bob", || {
        bob.snapshot()
            .coins
            .iter()
            .any(|coin| !coin.is_own && coin.container == "bowl")
    })?;
    assert!(
        bob.snapshot().hand.is_empty(),
        "one ante alone must not deal"
    );
    println!("after_one_quarter=waiting_for_other_ante; private_cards=0");
    pay_ante(&bob, &capability.room_id)?;
    wait_for("private deal", || {
        let alice = alice.snapshot();
        let bob = bob.snapshot();
        alice.own_seat() == Some(0)
            && bob.own_seat() == Some(1)
            && alice.hand.len() == 1
            && bob.hand.len() == 1
            && alice.card_poses.len() == 2
            && bob.card_poses.len() == 2
    })?;

    let alice_before = alice.snapshot();
    let card = alice_before.hand.first().expect("Alice has a dealt card");
    let started = Instant::now();
    alice.set_card_pose(
        capability.room_id.clone(),
        card.card_id.clone(),
        1,
        [-900, 160, 650],
        [12_000, 30_000, -8_000],
    )?;
    wait_for("Bob pose observation", || {
        bob.snapshot().card_poses.iter().any(|pose| {
            pose.card_id == card.card_id
                && pose.sequence == 1
                && pose.position_mm == [-900, 160, 650]
        })
    })?;
    let peer_latency = started.elapsed();

    let alice_after = alice.snapshot();
    let bob_after = bob.snapshot();
    assert_ne!(
        alice_after
            .hand
            .iter()
            .map(|card| &card.face)
            .collect::<Vec<_>>(),
        bob_after
            .hand
            .iter()
            .map(|card| &card.face)
            .collect::<Vec<_>>()
    );
    println!("room_code={}", capability.join_code);
    println!("members={}", alice_after.members.len());
    println!("alice_private_cards={}", alice_after.hand.len());
    println!("bob_private_cards={}", bob_after.hand.len());
    println!("shared_face_free_poses={}", bob_after.card_poses.len());
    println!(
        "single_pose_peer_latency_ms={:.2}",
        peer_latency.as_secs_f64() * 1000.0
    );
    alice.leave_room(capability.room_id.clone())?;
    bob.leave_room(capability.room_id)?;
    wait_for("both clients left", || {
        alice.snapshot().rooms.is_empty() && bob.snapshot().rooms.is_empty()
    })?;
    Ok(())
}

fn connect_disposable_clients() -> Result<(PocheClient, PocheClient), Box<dyn std::error::Error>> {
    let database =
        std::env::var("POCHE_SPACETIMEDB_DATABASE").unwrap_or_else(|_| "poche-desktop-v1".into());
    let run_id = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let alice = PocheClient::connect(
        config(&database, &format!("acceptance-alice-{run_id}")),
        TIMEOUT,
    )?;
    let bob = PocheClient::connect(
        config(&database, &format!("acceptance-bob-{run_id}")),
        TIMEOUT,
    )?;
    Ok((alice, bob))
}

/// Acceptance failures must not leave disposable test lobbies behind.
struct LeaveOnDrop<'a>(&'a PocheClient);

impl Drop for LeaveOnDrop<'_> {
    fn drop(&mut self) {
        let Some(room_id) = self.0.snapshot().room_id().map(str::to_owned) else {
            return;
        };
        if let Err(error) = self.0.leave_room(room_id) {
            eprintln!("test lobby cleanup request failed: {error}");
        } else if let Err(error) =
            wait_for("test lobby cleanup", || self.0.snapshot().rooms.is_empty())
        {
            eprintln!("{error}");
        }
    }
}

fn pay_ante(client: &PocheClient, room: &str) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = client.snapshot();
    let quarter = snapshot
        .coins
        .iter()
        .find(|coin| coin.is_own && coin.container == "lid" && coin.denomination_cents == 25)
        .ok_or("no lid quarter available")?;
    // This renderer-neutral example requests the logical transfer directly.
    // The server places the committed coin in its canonical bowl pile.
    client.move_coin(
        room.into(),
        quarter.coin_id.clone(),
        quarter.sequence + 1,
        "bowl".into(),
        quarter.position_mm,
        true,
    )?;
    Ok(())
}

fn config(database: &str, account_id: &str) -> ClientConfig {
    ClientConfig {
        uri: std::env::var("POCHE_SPACETIMEDB_URI")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".into()),
        database: database.into(),
        account_id: account_id.into(),
    }
}

fn wait_for(label: &str, predicate: impl Fn() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if predicate() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(format!("timed out waiting for {label}"))
}
