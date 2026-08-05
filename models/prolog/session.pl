% This Source Code Form is subject to the terms of the Mozilla Public
% License, v. 2.0. If a copy of the MPL was not distributed with this
% file, You can obtain one at https://mozilla.org/MPL/2.0/.

:- module(poche_session, [
    initial_state/1,
    named_state/2,
    step/5,
    predecessor/5,
    decision/4,
    can_see/5,
    projection_chain/5,
    revocation_chain/5,
    possible_history/2,
    run_conformance_fixture/1
]).

:- use_module(library(format), [format/2]).
:- use_module(library(lists), [length/2]).

:- discontiguous(conformance_row/2).

% Independent relational session oracle handwritten from session-rules.md.
% The bounded state is room(Phase, ReadyHost, ReadyP1, ConnectedP1,
% Request, Grant, StartCount, AbstractGameStep). Stable membership and seats
% are host/seat0, p1/seat1, and unseated spectator spec. Cryptography, message
% bytes, real time, chat text, and concrete cards are intentionally absent.

principal(host).
principal(p1).
principal(spec).
principal(outsider).

member(host).
member(p1).
member(spec).

player(host).
player(p1).

spectator(spec).

open_phase(lobby).
open_phase(countdown).
open_phase(running).
open_phase(paused).
open_phase(post_game).

initial_state(room(lobby, no, no, yes, none, none, 0, 0)).

named_state(initial, room(lobby, no, no, yes, none, none, 0, 0)).
named_state(host_ready, room(lobby, yes, no, yes, none, none, 0, 0)).
named_state(all_ready, room(lobby, yes, yes, yes, none, none, 0, 0)).
named_state(countdown, room(countdown, yes, yes, yes, none, none, 0, 0)).
named_state(running, room(running, no, no, yes, none, none, 1, 0)).
named_state(running_step1, room(running, no, no, yes, none, none, 1, 1)).
named_state(paused, room(paused, no, no, yes, none, none, 1, 0)).
named_state(requested_host,
    room(running, no, no, yes, request(host, spec), none, 1, 0)).
named_state(granted_host,
    room(running, no, no, yes, none, grant(host, spec), 1, 0)).
named_state(post_game, room(post_game, no, no, yes, none, none, 1, 2)).
named_state(closed, room(closed, no, no, yes, none, none, 1, 0)).

% Productive modes include step(+State,+Actor,+Command,-Next,-Rule),
% step(-State,+Actor,+Command,+Next,-Rule) for bounded ground Next, and fully
% ground validation. No Rust callback participates in either direction.

step(room(lobby, _, R1, C1, Request, Grant, Starts, Game), host, ready,
    room(lobby, yes, R1, C1, Request, Grant, Starts, Game), 'S-ROOM-004').
step(room(lobby, R0, _, yes, Request, Grant, Starts, Game), p1, ready,
    room(lobby, R0, yes, yes, Request, Grant, Starts, Game), 'S-ROOM-004').

step(room(Phase, _, R1, C1, Request, Grant, Starts, Game), host, unready,
    room(lobby, no, R1, C1, Request, Grant, Starts, Game), 'S-ROOM-005') :-
    ( Phase = lobby ; Phase = countdown ).
step(room(Phase, R0, _, C1, Request, Grant, Starts, Game), p1, unready,
    room(lobby, R0, no, C1, Request, Grant, Starts, Game), 'S-ROOM-005') :-
    ( Phase = lobby ; Phase = countdown ).

step(room(lobby, yes, yes, yes, Request, Grant, Starts, Game), host,
    arm_countdown,
    room(countdown, yes, yes, yes, Request, Grant, Starts, Game),
    'S-ROOM-006').
step(room(countdown, R0, R1, C1, Request, Grant, Starts, Game), Actor,
    abort_countdown,
    room(lobby, R0, R1, C1, Request, Grant, Starts, Game), 'S-ROOM-007') :-
    player(Actor).
step(room(countdown, yes, yes, yes, Request, Grant, 0, Game), clock, expire,
    room(running, no, no, yes, Request, Grant, 1, Game), 'S-ROOM-008').

step(room(running, R0, R1, C1, Request, Grant, Starts, Game), Actor, pause,
    room(paused, R0, R1, C1, Request, Grant, Starts, Game), 'S-ROOM-011') :-
    player(Actor).
step(room(paused, R0, R1, C1, Request, Grant, Starts, Game), Actor, resume,
    room(running, R0, R1, C1, Request, Grant, Starts, Game), 'S-ROOM-012') :-
    player(Actor).

step(room(running, R0, R1, C1, Request, Grant, Starts, 0), Actor, game_action,
    room(running, R0, R1, C1, Request, Grant, Starts, 1), 'S-ROOM-010') :-
    player(Actor).
step(room(running, R0, R1, C1, Request, Grant, Starts, 1), Actor, game_action,
    room(post_game, R0, R1, C1, Request, Grant, Starts, 2), 'S-ROOM-016') :-
    player(Actor).

step(room(countdown, R0, _, yes, Request, Grant, Starts, Game), runtime,
    disconnect,
    room(lobby, R0, no, no, Request, Grant, Starts, Game), 'S-ROOM-019').
step(room(Phase, R0, _, yes, Request, Grant, Starts, Game), runtime, disconnect,
    room(Phase, R0, no, no, Request, Grant, Starts, Game), 'S-ROOM-019') :-
    open_phase(Phase),
    Phase \= countdown.
step(room(Phase, R0, R1, no, Request, Grant, Starts, Game), p1, reconnect,
    room(Phase, R0, R1, yes, Request, Grant, Starts, Game), 'S-ROOM-020') :-
    open_phase(Phase).

step(room(Phase, R0, R1, C1, none, Grant, Starts, Game), spec,
    request_hand(Owner),
    room(Phase, R0, R1, C1, request(Owner, spec), Grant, Starts, Game),
    'S-VIEW-003') :-
    open_phase(Phase),
    player(Owner).
step(room(Phase, R0, R1, C1, request(Owner, spec), _Grant, Starts, Game), Owner,
    grant_hand(spec),
    room(Phase, R0, R1, C1, none, grant(Owner, spec), Starts, Game),
    'S-VIEW-004') :-
    open_phase(Phase),
    player(Owner).
step(room(Phase, R0, R1, C1, Request, grant(Owner, spec), Starts, Game), Owner,
    revoke_hand(spec),
    room(Phase, R0, R1, C1, Request, none, Starts, Game), 'S-VIEW-005') :-
    open_phase(Phase),
    player(Owner).

step(room(Phase, R0, R1, C1, Request, Grant, Starts, Game), Actor, chat,
    room(Phase, R0, R1, C1, Request, Grant, Starts, Game), 'S-CHAT-001') :-
    open_phase(Phase),
    member(Actor).
step(room(Phase, R0, R1, C1, Request, Grant, Starts, Game), host, close_room,
    room(closed, R0, R1, C1, Request, Grant, Starts, Game), 'S-ROOM-022') :-
    open_phase(Phase).

predecessor(Next, Actor, Command, Previous, Rule) :-
    step(Previous, Actor, Command, Next, Rule).

rule_explanation('S-ROOM-004', seated_player_marks_ready).
rule_explanation('S-ROOM-005', unready_cancels_countdown).
rule_explanation('S-ROOM-006', host_arms_only_when_all_ready).
rule_explanation('S-ROOM-007', any_player_aborts_countdown).
rule_explanation('S-ROOM-008', authority_expiry_starts_once).
rule_explanation('S-ROOM-010', running_actor_may_advance_game).
rule_explanation('S-ROOM-011', any_player_may_pause).
rule_explanation('S-ROOM-012', any_player_may_resume).
rule_explanation('S-ROOM-016', abstract_game_reaches_post_game).
rule_explanation('S-ROOM-019', route_loss_preserves_membership).
rule_explanation('S-ROOM-020', stable_member_reconnects).
rule_explanation('S-ROOM-022', host_closes_room).
rule_explanation('S-VIEW-003', spectator_requests_exact_owner).
rule_explanation('S-VIEW-004', owner_grants_exact_recipient).
rule_explanation('S-VIEW-005', owner_revokes_future_delivery).
rule_explanation('S-VIEW-001', player_sees_own_hand).
rule_explanation('S-VIEW-002', member_sees_public_information).
rule_explanation('S-VIEW-006', exact_grant_reveals_exact_hand).
rule_explanation('S-CHAT-001', current_member_may_chat).
rule_explanation('S-AUTH-001', default_deny).

allowed(State, Actor, Command, Rule, Explanation) :-
    step(State, Actor, Command, _, Rule),
    rule_explanation(Rule, Explanation).

decision(State, Actor, Command, allow(Rule, Explanation)) :-
    allowed(State, Actor, Command, Rule, Explanation).
decision(State, Actor, Command, deny('S-AUTH-001', default_deny)) :-
    principal(Actor),
    bounded_command(Command),
    \+ allowed(State, Actor, Command, _, _).

bounded_command(ready).
bounded_command(unready).
bounded_command(arm_countdown).
bounded_command(abort_countdown).
bounded_command(game_action).
bounded_command(pause).
bounded_command(resume).
bounded_command(reconnect).
bounded_command(request_hand(Owner)) :- player(Owner).
bounded_command(grant_hand(spec)).
bounded_command(revoke_hand(spec)).
bounded_command(chat).
bounded_command(close_room).

can_see(_, Viewer, public, 'S-VIEW-002', Explanation) :-
    member(Viewer),
    rule_explanation('S-VIEW-002', Explanation).
can_see(_, Owner, hand(Owner), 'S-VIEW-001', Explanation) :-
    player(Owner),
    rule_explanation('S-VIEW-001', Explanation).
can_see(room(_, _, _, _, _, grant(Owner, Viewer), _, _), Viewer, hand(Owner),
    'S-VIEW-006', Explanation) :-
    spectator(Viewer),
    player(Owner),
    rule_explanation('S-VIEW-006', Explanation).

projection_chain(Initial, Owner, Viewer,
    [because('S-VIEW-003', spectator_requests_exact_owner),
     because('S-VIEW-004', owner_grants_exact_recipient)], Final) :-
    step(Initial, Viewer, request_hand(Owner), Requested, 'S-VIEW-003'),
    step(Requested, Owner, grant_hand(Viewer), Final, 'S-VIEW-004'),
    can_see(Final, Viewer, hand(Owner), 'S-VIEW-006', _).

revocation_chain(Initial, Owner, Viewer,
    [because('S-VIEW-003', spectator_requests_exact_owner),
     because('S-VIEW-004', owner_grants_exact_recipient),
     because('S-VIEW-005', owner_revokes_future_delivery)], Final) :-
    projection_chain(Initial, Owner, Viewer, _, Granted),
    step(Granted, Owner, revoke_hand(Viewer), Final, 'S-VIEW-005'),
    \+ can_see(Final, Viewer, hand(Owner), _, _).

replay(State, [], State).
replay(State, [event(Actor, Command, Rule)|Events], Final) :-
    step(State, Actor, Command, Next, Rule),
    replay(Next, Events, Final).

history_candidate([
    event(host, ready, 'S-ROOM-004'),
    event(p1, ready, 'S-ROOM-004'),
    event(host, arm_countdown, 'S-ROOM-006'),
    event(clock, expire, 'S-ROOM-008')
]).
history_candidate([
    event(host, ready, 'S-ROOM-004'),
    event(p1, ready, 'S-ROOM-004'),
    event(host, arm_countdown, 'S-ROOM-006'),
    event(clock, expire, 'S-ROOM-008'),
    event(p1, pause, 'S-ROOM-011')
]).
history_candidate([
    event(host, ready, 'S-ROOM-004'),
    event(p1, ready, 'S-ROOM-004'),
    event(host, arm_countdown, 'S-ROOM-006'),
    event(p1, abort_countdown, 'S-ROOM-007'),
    event(host, arm_countdown, 'S-ROOM-006'),
    event(clock, expire, 'S-ROOM-008')
]).

possible_history(Target, Events) :-
    initial_state(Initial),
    history_candidate(Events),
    replay(Initial, Events, Target).

% Controlled defects are separate relations and never feed decision/can_see.
defective_allowed(_, outsider, pause, 'D-POLICY-ROOM-MEMBERSHIP').
defective_can_see(_, spec, hand(p1), 'D-VIEW-ROOM-BROADCAST').

% Deterministic normalized fixture protocol ---------------------------------

run_conformance_fixture(Name) :-
    findall(Row, conformance_row(Name, Row), RawRows),
    sort(RawRows, Rows),
    format("POCHE_PROLOG_FIXTURE ~w BEGIN~n", [Name]),
    write_rows(Rows),
    length(Rows, Count),
    format("POCHE_PROLOG_FIXTURE ~w END count=~d~n", [Name, Count]).

write_rows([]).
write_rows([Row|Rows]) :-
    format("POCHE_PROLOG_ANSWER ~w~n", [Row]),
    write_rows(Rows).

policy_case(ready_initial, initial, host, ready).
policy_case(arm_too_early, initial, host, arm_countdown).
policy_case(arm_all_ready, all_ready, host, arm_countdown).
policy_case(player_pause, running, p1, pause).
policy_case(player_resume, paused, host, resume).
policy_case(action_while_paused, paused, host, game_action).
policy_case(request_exact_hand, running, spec, request_hand(host)).
policy_case(grant_without_request, running, p1, grant_hand(spec)).
policy_case(nonhost_close, running, p1, close_room).
policy_case(spectator_chat, paused, spec, chat).
policy_case(outsider_pause, running, outsider, pause).

conformance_row(policy_decisions, decision(Name, Actor, Command, Result)) :-
    policy_case(Name, StateName, Actor, Command),
    named_state(StateName, State),
    decision(State, Actor, Command, Result).

successor_case(initial, host, ready).
successor_case(host_ready, p1, ready).
successor_case(all_ready, host, arm_countdown).
successor_case(countdown, p1, abort_countdown).
successor_case(countdown, clock, expire).
successor_case(running, host, pause).
successor_case(paused, p1, resume).
successor_case(running, host, game_action).
successor_case(running_step1, p1, game_action).
successor_case(running, spec, request_hand(host)).
successor_case(requested_host, host, grant_hand(spec)).
successor_case(granted_host, host, revoke_hand(spec)).
successor_case(running, host, close_room).

conformance_row(successors, successor(From, Actor, Command, Rule, To)) :-
    successor_case(From, Actor, Command),
    named_state(From, State),
    step(State, Actor, Command, Next, Rule),
    named_state(To, Next).

predecessor_target(running).
predecessor_target(paused).

conformance_row(predecessors, predecessor(To, Actor, Command, Rule, From)) :-
    predecessor_target(To),
    named_state(To, Target),
    named_state(From, Previous),
    predecessor(Target, Actor, Command, Previous, Rule).

visibility_state(running).
visibility_state(requested_host).
visibility_state(granted_host).
visibility_viewer(host).
visibility_viewer(p1).
visibility_viewer(spec).
visibility_viewer(outsider).
visibility_item(public).
visibility_item(hand(host)).
visibility_item(hand(p1)).

conformance_row(visibility, visible(StateName, Viewer, Item, Rule, Explanation)) :-
    visibility_state(StateName),
    visibility_viewer(Viewer),
    visibility_item(Item),
    named_state(StateName, State),
    can_see(State, Viewer, Item, Rule, Explanation).

conformance_row(grant_chains, grant(Owner, spec, Chain)) :-
    named_state(running, Initial),
    player(Owner),
    projection_chain(Initial, Owner, spec, Chain, _).
conformance_row(grant_chains, revoke(Owner, spec, Chain)) :-
    named_state(running, Initial),
    player(Owner),
    revocation_chain(Initial, Owner, spec, Chain, _).

conformance_row(history_causes, history(TargetName, Events)) :-
    ( TargetName = running ; TargetName = paused ),
    named_state(TargetName, Target),
    possible_history(Target, Events).

conformance_row(controlled_defects,
    defect(policy, outsider, pause, Defect)) :-
    named_state(running, State),
    defective_allowed(State, outsider, pause, Defect).
conformance_row(controlled_defects,
    safe(policy, outsider, pause, Rule, Explanation)) :-
    named_state(running, State),
    decision(State, outsider, pause, deny(Rule, Explanation)).
conformance_row(controlled_defects,
    defect(visibility, spec, hand(p1), Defect)) :-
    named_state(running, State),
    defective_can_see(State, spec, hand(p1), Defect).
conformance_row(controlled_defects,
    safe(visibility, spec, hand(p1), 'S-VIEW-006', exact_grant_required)) :-
    named_state(running, State),
    \+ can_see(State, spec, hand(p1), _, _).
