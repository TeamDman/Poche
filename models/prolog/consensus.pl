% This Source Code Form is subject to the terms of the Mozilla Public
% License, v. 2.0. If a copy of the MPL was not distributed with this
% file, You can obtain one at https://mozilla.org/MPL/2.0/.

:- module(poche_consensus, [
    device_authority/5,
    proposal_decision/4,
    vote_decision/5,
    certificate_decision/4,
    recovery_step/4,
    predecessor/4,
    assumption_result/3,
    run_conformance_fixture/1
]).

:- use_module(library(format), [format/2]).
:- use_module(library(lists), [length/2]).

% Independent finite explanation oracle for consensus-micro.

device_authority(alice_native, alice, epoch1, active, native_local).
device_authority(alice_browser, alice, epoch1, active, browser_local).
device_authority(alice_revoked, alice, epoch2, revoked, browser_local).
device_authority(bob_device, bob, epoch2, active, native_local).
device_authority(carol_device, carol, epoch2, inactive, native_local).

proposal_decision(score_round, alice_native, accepted,
    because(active_device_current_epoch)).
proposal_decision(chat_public, bob_device, accepted,
    because(active_device_current_epoch)).
proposal_decision(score_adjust, alice_browser, accepted,
    because(active_device_current_epoch)).
proposal_decision(auto_native, alice_native, accepted,
    because(common_transition_key_and_hash)).
proposal_decision(auto_browser, alice_browser, superseded(auto_native),
    because(common_transition_key_and_hash)).
proposal_decision(auto_bob, bob_device, superseded(auto_native),
    because(common_transition_key_and_hash)).
proposal_decision(carol_stale, carol_device, denied(stale_epoch),
    because(epoch1_is_not_epoch2)).
proposal_decision(alice_revoked_attempt, alice_revoked, denied(revoked_device),
    because(revocation_effective_epoch2)).

vote_decision(event1, alice_native, alice, counted, because(one_vote_per_player)).
vote_decision(event1, alice_browser, alice, duplicate,
    because(player_vote_already_counted)).
vote_decision(event1, bob_device, bob, counted, because(one_vote_per_player)).
vote_decision(kick_carol, alice_native, alice, counted,
    because(old_and_new_member)).
vote_decision(kick_carol, bob_device, bob, counted,
    because(old_and_new_member)).
vote_decision(post_kick, carol_device, carol, denied,
    because(not_active_in_epoch2)).

certificate_decision(event1, epoch1, committed,
    because(strict_majority(2, 3))).
certificate_decision(kick_carol, joint(epoch1, epoch2), committed,
    because(old_majority(2, 3)-new_majority(2, 2))).
certificate_decision(minority_after_kick, epoch2, denied,
    because(one_is_not_strict_majority_of_two)).
certificate_decision(two_player_partition, epoch2, stalled,
    because(both_players_are_required)).

recovery_step(current_actor_carol, propose(kick_carol), allowed,
    because(governance_is_out_of_turn)).
recovery_step(kick_carol, certify(alice_bob), epoch2,
    because(joint_quorum)).
recovery_step(epoch2, reject(carol_stale), progressing,
    because(kicked_actor_cannot_hold_priority)).

predecessor(commit(event1), vote(event1, alice), proposed(event1),
    because(strict_player_majority)).
predecessor(commit(event1), vote(event1, bob), proposed(event1),
    because(strict_player_majority)).
predecessor(commit(kick_carol), vote(kick_carol, alice), epoch1,
    because(joint_majority)).
predecessor(commit(kick_carol), vote(kick_carol, bob), epoch1,
    because(joint_majority)).
predecessor(commit(auto_advance), proposal(auto_native), epoch2,
    because(duplicate_semantic_proposals_collapse)).

assumption_result(non_equivocation, retained, safe_common_prefix).
assumption_result(non_equivocation, removed,
    counterexample(two_majorities_intersect_at_equivocating_bob)).
assumption_result(eventual_delivery, removed,
    counterexample(connected_replica_can_remain_stalled)).

run_conformance_fixture(Name) :-
    findall(Row, conformance_row(Name, Row), RawRows),
    sort(RawRows, Rows),
    length(Rows, Count),
    format("POCHE_PROLOG_FIXTURE ~w BEGIN~n", [Name]),
    print_rows(Rows),
    format("POCHE_PROLOG_FIXTURE ~w END count=~d~n", [Name, Count]).

print_rows([]).
print_rows([Row|Rows]) :-
    format("POCHE_PROLOG_ANSWER ~q~n", [Row]),
    print_rows(Rows).

conformance_row(devices, device(Device, Player, Epoch, Status, Custody)) :-
    device_authority(Device, Player, Epoch, Status, Custody).
conformance_row(proposals, proposal(Id, Device, Decision, Why)) :-
    proposal_decision(Id, Device, Decision, Why).
conformance_row(votes, vote(Event, Device, Player, Decision, Why)) :-
    vote_decision(Event, Device, Player, Decision, Why).
conformance_row(certificates, certificate(Event, Epoch, Decision, Why)) :-
    certificate_decision(Event, Epoch, Decision, Why).
conformance_row(recovery, recovery(State, Action, Result, Why)) :-
    recovery_step(State, Action, Result, Why).
conformance_row(predecessors, predecessor(Event, Cause, Prior, Why)) :-
    predecessor(Event, Cause, Prior, Why).
conformance_row(assumptions, assumption(Name, Status, Result)) :-
    assumption_result(Name, Status, Result).
