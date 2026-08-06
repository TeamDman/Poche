% This Source Code Form is subject to the terms of the Mozilla Public
% License, v. 2.0. If a copy of the MPL was not distributed with this
% file, You can obtain one at https://mozilla.org/MPL/2.0/.

:- module(poche_governance, [
    eligible/3,
    visible_vote/5,
    tally/5,
    decision/4,
    effect/4,
    predecessor/4,
    run_conformance_fixture/1
]).

:- use_module(library(format), [format/2]).
:- use_module(library(lists), [length/2]).

% Independent finite relational oracle for governance-micro.

member(alice).
member(bob).
member(john).

proposal(score_vote, adjust_score(alice, 100), 2, 10).
proposal(score_reject, adjust_score(alice, -40), 2, 10).
proposal(score_timeout, adjust_score(bob, 25), 2, 10).
proposal(kick_accused, kick(john), 2, 10).
proposal(redeal_afk, redeal, 1, 10).

eligible(Proposal, alice, included) :- proposal(Proposal, _, _, _).
eligible(Proposal, bob, included) :-
    proposal(Proposal, _, 2, _).
eligible(kick_accused, john, excluded(target_and_confirmed_accused)).

visible_vote(score_vote, alice, approve, yes, included).
visible_vote(score_vote, bob, approve, yes, included).
visible_vote(score_vote, john, reject, no, confirmed_accused_subject).
visible_vote(score_reject, alice, reject, yes, included).
visible_vote(score_reject, bob, reject, yes, included).
visible_vote(score_timeout, alice, approve, yes, included).
visible_vote(kick_accused, alice, approve, yes, included).
visible_vote(kick_accused, bob, approve, yes, included).
visible_vote(kick_accused, john, reject, no, target_and_confirmed_accused).
visible_vote(redeal_afk, alice, approve, yes, included).

counted_choice(Proposal, Voter, Choice) :-
    visible_vote(Proposal, Voter, Choice, yes, _).

tally(Proposal, Eligible, Approvals, Rejections, Abstentions) :-
    proposal(Proposal, _, Eligible, _),
    findall(Voter, counted_choice(Proposal, Voter, approve), ApproveRows),
    findall(Voter, counted_choice(Proposal, Voter, reject), RejectRows),
    findall(Voter, counted_choice(Proposal, Voter, abstain), AbstainRows),
    length(ApproveRows, Approvals),
    length(RejectRows, Rejections),
    length(AbstainRows, Abstentions).

strict_majority(Votes, Eligible) :- Votes * 2 > Eligible.

decision(Proposal, approved, before_deadline, because(strict_majority)) :-
    tally(Proposal, Eligible, Approvals, _, _),
    strict_majority(Approvals, Eligible).
decision(Proposal, rejected, before_deadline, because(rejection_majority)) :-
    tally(Proposal, Eligible, _, Rejections, _),
    strict_majority(Rejections, Eligible).
decision(score_timeout, rejected, logical_tick_10,
    because(deadline_without_majority)).

capability(alice, change_rights).
capability(alice, adjust_score).

effect(Proposal, Action, approved_vote, because(strict_majority)) :-
    proposal(Proposal, Action, _, _),
    decision(Proposal, approved, _, because(strict_majority)).
effect(direct_score, adjust_score(alice, 100), capability(alice, adjust_score),
    because(exact_unilateral_grant)).
effect(direct_rights, change_rights(bob, grant(adjust_score)),
    capability(alice, change_rights), because(exact_unilateral_grant)).

predecessor(effect(Proposal, Action), vote(Proposal, Voter, approve),
    pending(Proposal), because(counted_strict_majority)) :-
    effect(Proposal, Action, approved_vote, _),
    counted_choice(Proposal, Voter, approve).

structural_denial(create_card, deny(structural_invariant),
    because(finite_card_universe_not_governable)).
structural_denial(change_card_identity, deny(structural_invariant),
    because(card_identity_not_governable)).

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

conformance_row(eligibility, eligibility(Proposal, Member, Status)) :-
    eligible(Proposal, Member, Status).
conformance_row(visible_votes,
    vote(Proposal, Voter, Choice, counted(Counted), exclusion(Exclusion))) :-
    visible_vote(Proposal, Voter, Choice, Counted, Exclusion).
conformance_row(tallies,
    tally(Proposal, Eligible, Approvals, Rejections, Abstentions)) :-
    tally(Proposal, Eligible, Approvals, Rejections, Abstentions).
conformance_row(decisions, decision(Proposal, Outcome, When, Why)) :-
    decision(Proposal, Outcome, When, Why).
conformance_row(effects, effect(Source, Action, Authority, Why)) :-
    effect(Source, Action, Authority, Why).
conformance_row(predecessors,
    predecessor(Effect, Vote, Previous, Why)) :-
    predecessor(Effect, Vote, Previous, Why).
conformance_row(structural_denials, structural(Command, Outcome, Why)) :-
    structural_denial(Command, Outcome, Why).
