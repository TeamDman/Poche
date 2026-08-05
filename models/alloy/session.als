// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

module session

// Independent bounded session/knowledge oracle. It is handwritten from
// docs/session-rules.md, not generated from the Rust reducer.

abstract sig Principal {}
one sig Host, PlayerA, PlayerB, Spectator, Outsider extends Principal {}

abstract sig Seat {}
one sig SeatA, SeatB extends Seat {}

abstract sig Phase {}
one sig Lobby, Countdown, Running, Paused, PostGame, Closed extends Phase {}

abstract sig Flag {}
one sig Yes, No extends Flag {}

abstract sig Command {}
one sig CreateRoom, JoinRoom, TakeSeat, Ready, ArmCountdown, AbortCountdown,
        ExpireCountdown, PauseGame, UnpauseGame, GameAction, RequestHand,
        GrantHand, RevokeHand, CloseRoom extends Command {}

sig Snapshot {
  host: one Principal,
  members: set Principal,
  seated: Principal -> lone Seat,
  ready: set Principal,
  phase: one Phase,
  startCount: one Int,
  startWasReady: one Flag,
  // player -> spectator capability edge
  grants: Principal -> Principal,
  // viewer -> hand owner knowledge edge
  seesHand: Principal -> Principal
}

fun players[s: Snapshot]: set Principal { s.seated.Seat }

pred valid[s: Snapshot] {
  s.host = Host
  Host in s.members
  s.seated in s.members -> Seat
  all seat: Seat | lone seat.~(s.seated)
  s.ready in players[s]
  s.startCount >= 0 and s.startCount <= 1
  s.startCount = 1 implies s.startWasReady = Yes

  all owner, viewer: Principal |
    owner->viewer in s.grants implies
      owner in players[s] and viewer in s.members - players[s] and owner != viewer

  // Knowledge is derived, never UI-filtered after room broadcast.
  s.seesHand = {
    viewer, owner: Principal |
      (viewer = owner and owner in players[s]) or owner->viewer in s.grants
  }

  s.phase in Running + Paused + PostGame implies s.startCount = 1
  s.phase = Countdown implies #players[s] = 2 and players[s] in s.ready
}

pred sameCore[pre, post: Snapshot] {
  post.host = pre.host
  post.members = pre.members
  post.seated = pre.seated
  post.ready = pre.ready
  post.startCount = pre.startCount
  post.startWasReady = pre.startWasReady
}

pred grantHand[pre, post: Snapshot, owner, viewer: Principal] {
  valid[pre]
  owner in players[pre]
  viewer in pre.members - players[pre]
  no pre.grants.viewer
  sameCore[pre, post]
  post.phase = pre.phase
  post.grants = pre.grants + owner->viewer
  valid[post]
}

pred revokeHand[pre, post: Snapshot, owner, viewer: Principal] {
  valid[pre]
  owner->viewer in pre.grants
  sameCore[pre, post]
  post.phase = pre.phase
  post.grants = pre.grants - owner->viewer
  valid[post]
}

pred startGame[pre, post: Snapshot] {
  valid[pre]
  pre.phase = Countdown
  pre.startCount = 0
  players[pre] in pre.ready
  post.host = pre.host
  post.members = pre.members
  post.seated = pre.seated
  no post.ready
  post.phase = Running
  post.startCount = add[pre.startCount, 1]
  post.startWasReady = Yes
  post.grants = pre.grants
  valid[post]
}

pred pauseGame[pre, post: Snapshot, actor: Principal] {
  valid[pre]
  pre.phase = Running
  actor in players[pre]
  sameCore[pre, post]
  post.phase = Paused
  post.grants = pre.grants
  valid[post]
}

pred unpauseGame[pre, post: Snapshot, actor: Principal] {
  valid[pre]
  pre.phase = Paused
  actor in players[pre]
  sameCore[pre, post]
  post.phase = Running
  post.grants = pre.grants
  valid[post]
}

pred allowed[s: Snapshot, actor: Principal, command: Command] {
  valid[s]
  actor in s.members
  actor != Outsider
  (command in TakeSeat + Ready + RequestHand and actor != Outsider) or
  (command in ArmCountdown + CloseRoom and actor = s.host) or
  (command in AbortCountdown + PauseGame + UnpauseGame + GameAction and
    actor in players[s]) or
  (command in GrantHand + RevokeHand and actor in players[s])
}

assert DefaultDeny {
  all s: Snapshot, command: Command |
    valid[s] implies not allowed[s, Outsider, command]
}

assert SingleSeatOwnership {
  all s: Snapshot | valid[s] implies all seat: Seat | lone seat.~(s.seated)
}

assert NoStartWithoutReadiness {
  all pre, post: Snapshot |
    startGame[pre, post] implies players[pre] in pre.ready
}

assert AtMostOnceStart {
  all pre, post: Snapshot |
    startGame[pre, post] implies post.startCount = 1
}

assert ScopedSpectatorGrant {
  all pre, post: Snapshot, owner, viewer: Principal |
    grantHand[pre, post, owner, viewer] implies
      viewer in post.members - players[post] and
      viewer->owner in post.seesHand and
      all other: Principal - viewer |
        (other->owner in post.seesHand iff other->owner in pre.seesHand)
}

assert RevocationStopsFutureKnowledge {
  all pre, post: Snapshot, owner, viewer: Principal |
    revokeHand[pre, post, owner, viewer] implies viewer->owner not in post.seesHand
}

assert NoUnauthorizedKnowledge {
  all s: Snapshot, viewer, owner: Principal |
    valid[s] and viewer->owner in s.seesHand implies
      viewer = owner or owner->viewer in s.grants
}

assert PauseResumePreserveRoom {
  all pre, post: Snapshot, actor: Principal |
    pauseGame[pre, post, actor] or unpauseGame[pre, post, actor] implies
      post.seated = pre.seated and post.members = pre.members and
      post.grants = pre.grants and post.startCount = pre.startCount
}

pred ValidRoomWitness {
  some s: Snapshot |
    valid[s] and s.phase = Running and
    s.members = Host + PlayerA + Spectator and
    s.seated = Host->SeatA + PlayerA->SeatB and
    s.startCount = 1 and s.startWasReady = Yes and
    s.grants = Host->Spectator
}

pred PauseResumeWitness {
  some running, paused, resumed: Snapshot, actor: Principal |
    pauseGame[running, paused, actor] and unpauseGame[paused, resumed, actor]
}

// Controlled defects must stay satisfiable so assertions discriminate errors.
pred DefectiveDuplicateSeatWitness {
  some s: Snapshot |
    s.seated = Host->SeatA + PlayerA->SeatA
}

pred DefectiveStartUnreadyWitness {
  some pre, post: Snapshot |
    pre.phase = Countdown and no pre.ready and
    post.phase = Running and post.startCount = 1
}

pred DefectiveRoomWideHandWitness {
  some s: Snapshot |
    s.members = Host + PlayerA + Spectator and
    s.seated = Host->SeatA + PlayerA->SeatB and
    Spectator->Host in s.seesHand and no s.grants
}

run ValidRoomWitness for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
run PauseResumeWitness for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check DefaultDeny for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check SingleSeatOwnership for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check NoStartWithoutReadiness for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check AtMostOnceStart for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check ScopedSpectatorGrant for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check RevocationStopsFutureKnowledge for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check NoUnauthorizedKnowledge for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
check PauseResumePreserveRoom for 5 but exactly 2 Seat, exactly 4 Snapshot, 5 Int
run DefectiveDuplicateSeatWitness for 5 but exactly 2 Seat, exactly 2 Snapshot, 5 Int
run DefectiveStartUnreadyWitness for 5 but exactly 2 Seat, exactly 2 Snapshot, 5 Int
run DefectiveRoomWideHandWitness for 5 but exactly 2 Seat, exactly 2 Snapshot, 5 Int
