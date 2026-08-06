% This Source Code Form is subject to the terms of the Mozilla Public
% License, v. 2.0. If a copy of the MPL was not distributed with this
% file, You can obtain one at https://mozilla.org/MPL/2.0/.

:- module(poche_spatial, [
    card_location/3,
    attached_text/5,
    resolve_named/5,
    resolve_drag/6,
    drop_explanation/6,
    layout_finding/3,
    spatial_step/5,
    spatial_predecessor/5,
    run_conformance_fixture/1
]).

:- use_module(library(format), [format/2]).
:- use_module(library(lists), [length/2]).

:- discontiguous(conformance_row/2).

% Independent finite relational oracle for poche-spatial-v1 query-micro.
% It uses exact atoms and finite named regions. No arbitrary real, nonlinear,
% mesh, collision, renderer, or physics solving is claimed.

viewer(p0).
viewer(p1).
viewer(spec).

player(p0).
player(p1).

card(c0, two_clubs).
card(c1, jack_spades).
card(c2, ace_hearts).
card(c3, seven_diamonds).
card(c4, king_clubs).
card(c5, four_spades).
card(c6, nine_hearts).
card(c7, queen_diamonds).

card_location(c0, hand(p0, 0), owner(p0)).
card_location(c1, hand(p0, 1), owner(p0)).
card_location(c2, hand(p1, 0), owner(p1)).
card_location(c3, trump, public).
card_location(c4, play(p0), public).
card_location(c5, deck(0), concealed).
card_location(c6, won(p1, 0, 0), public).
card_location(c7, deck(1), concealed).

public_card(Card) :- card_location(Card, _, public).
owned_hand_card(Player, Card) :- card_location(Card, hand(Player, _), owner(Player)).

can_see(Player, Card, own_hand) :- player(Player), owned_hand_card(Player, Card).
can_see(Viewer, Card, public_zone) :- viewer(Viewer), public_card(Card).

attached_text(Viewer, face_text(Card, Face), surface(Card, face), card_face,
    because('P3-SPATIAL-TEXT-001', exact_viewer_face_attachment)) :-
    can_see(Viewer, Card, _),
    card(Card, Face).
attached_text(Viewer, name_text(Player), surface(score_sheet, face), player_name,
    because('P3-SPATIAL-TEXT-002', public_score_sheet_attachment)) :-
    viewer(Viewer),
    player(Player).
attached_text(Viewer, score_text(Player, 0), surface(score_sheet, face), player_score,
    because('P3-SPATIAL-TEXT-003', typed_score_cell_attachment)) :-
    viewer(Viewer),
    player(Player).

% Named-state command resolution. Typed and drag paths intentionally return
% the same intent and rule explanation for c0.
resolve_named(initial, p0, c0, allow(play(c0)),
    because('P3-SPATIAL-INTENT-001', visible_owned_hand_card)).
resolve_named(initial, p0, c2, deny(unauthorized_card),
    because('P3-SPATIAL-INTENT-002', card_owned_by_other_player)).
resolve_named(initial, p0, c7, deny(card_not_in_hand),
    because('P3-SPATIAL-INTENT-003', card_is_in_deck)).
resolve_named(paused, p0, c0, deny(paused),
    because('P3-SPATIAL-INTENT-004', committed_game_is_paused)).

resolve_drag(initial, p0, c0, play_inner, allow(play(c0)),
    because('P3-SPATIAL-INTENT-001', visible_owned_hand_card)).
resolve_drag(initial, p0, c0, deck_inner, deny(wrong_zone(deck)),
    because('P3-SPATIAL-DROP-002', snapped_to_non_play_zone)).
resolve_drag(initial, p0, c0, play_dead_band, deny(dead_band(play)),
    because('P3-SPATIAL-DROP-003', not_fully_inside_inner_volume)).
resolve_drag(initial, p0, c0, broad_deck_trump,
    deny(ambiguous([deck, trump])),
    because('P3-SPATIAL-DROP-004', multiple_outer_zones_implicated)).
resolve_drag(initial, p0, c0, nowhere, deny(free),
    because('P3-SPATIAL-DROP-005', no_outer_zone_implicated)).
resolve_drag(initial, p0, c0, outside_table, deny(out_of_bounds),
    because('P3-SPATIAL-DROP-006', outside_checked_table_frame)).
resolve_drag(initial, p0, c2, play_inner, deny(unauthorized_card),
    because('P3-SPATIAL-DROP-007', cannot_move_other_players_card)).
resolve_drag(initial, p0, unknown, play_inner, deny(unknown_object),
    because('P3-SPATIAL-DROP-008', object_handle_not_in_projection)).

drop_explanation(State, Actor, Card, Bound, Outcome, Explanation) :-
    resolve_drag(State, Actor, Card, Bound, Outcome, Explanation).

layout_finding(layout2, valid,
    because('P3-SPATIAL-LAYOUT-001', complete_separated_two_player_layout)).
layout_finding(overlap_layout, violation(overlap(deck, trump, deck_cell)),
    because('P3-SPATIAL-LAYOUT-002', outer_zone_intersection)).
layout_finding(duplicate_seat_layout, violation(duplicate_seat(seat0, p0, p1)),
    because('P3-SPATIAL-LAYOUT-003', seat_ownership_not_injective)).
layout_finding(missing_won_layout, violation(missing_zone(won(p1))),
    because('P3-SPATIAL-LAYOUT-004', required_semantic_zone_absent)).

% Complete bounded successor relation. State names denote canonical committed
% endpoint snapshots; presentation frames are not predecessors/successors.
spatial_step(initial, p0, play(c0), after_p0,
    because('P3-SPATIAL-STEP-001', hand_to_play_endpoint)).
spatial_step(initial, p1, pause, paused,
    because('P3-SPATIAL-STEP-002', pause_preserves_endpoints)).
spatial_step(paused, p0, resume, initial,
    because('P3-SPATIAL-STEP-003', resume_preserves_endpoints)).
spatial_step(after_p0, p1, play(c2), both_played,
    because('P3-SPATIAL-STEP-001', hand_to_play_endpoint)).
spatial_step(after_p0, runtime, disconnect, recovery,
    because('P3-SPATIAL-STEP-004', recovery_preserves_committed_state)).
spatial_step(recovery, p1, reconnect, after_p0,
    because('P3-SPATIAL-STEP-005', reconnect_restores_interaction)).
spatial_step(both_played, p1, capture, captured,
    because('P3-SPATIAL-STEP-006', complete_trick_to_won_endpoint)).

spatial_predecessor(Next, Actor, Command, Previous, Explanation) :-
    spatial_step(Previous, Actor, Command, Next, Explanation).

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

conformance_row(card_locations, location(Card, Zone, Authority)) :-
    card_location(Card, Zone, Authority).

conformance_row(attached_text, attached(Viewer, Text, Surface, Kind, Why)) :-
    attached_text(Viewer, Text, Surface, Kind, Why).

conformance_row(command_resolution,
    named(State, Actor, Card, Outcome, Why)) :-
    resolve_named(State, Actor, Card, Outcome, Why).
conformance_row(command_resolution,
    drag(State, Actor, Card, Bound, Outcome, Why)) :-
    State = initial,
    Actor = p0,
    Card = c0,
    Bound = play_inner,
    resolve_drag(State, Actor, Card, Bound, Outcome, Why).

conformance_row(drop_explanations,
    drop(State, Actor, Card, Bound, Outcome, Why)) :-
    drop_explanation(State, Actor, Card, Bound, Outcome, Why).

conformance_row(layout_findings, layout(Layout, Finding, Why)) :-
    layout_finding(Layout, Finding, Why).

conformance_row(successors, successor(From, Actor, Command, To, Why)) :-
    spatial_step(From, Actor, Command, To, Why).

conformance_row(predecessors, predecessor(To, Actor, Command, From, Why)) :-
    spatial_predecessor(To, Actor, Command, From, Why).
