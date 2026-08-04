% This Source Code Form is subject to the terms of the Mozilla Public
% License, v. 2.0. If a copy of the MPL was not distributed with this
% file, You can obtain one at https://mozilla.org/MPL/2.0/.

:- module(poche, [
    standard_deck/1,
    max_hand/2,
    hand_schedule/2,
    dealer_for_round/4,
    score_sheet_row/5,
    deal_round/7,
    legal_play/3,
    trick_winner/3,
    round_score/6,
    game_winners/2,
    pot_division/4,
    first_jack/5,
    high_card_selection/3,
    initial_round_state/2,
    valid_round_state/1,
    legal_action/2,
    step/3,
    predecessor/3,
    replay/3,
    restore_round/2,
    run_oracle_tests/0
]).

:- use_module(library(between), [between/3]).
:- use_module(library(format), [format/2]).
:- use_module(library(lists), [
    append/2,
    append/3,
    length/2,
    member/2,
    memberchk/2,
    reverse/2,
    select/3
]).

:- discontiguous(oracle_test/1).

% This is an independent relational oracle derived from docs/main.typ.  It is
% not generated from Rust.  Predicates document their productive modes in the
% comments below; finite generators precede recursive relations so backtracking
% remains bounded for the query corpus.

% R-GAME-001, R-HAND-001..004 -------------------------------------------------

player_count(N) :- between(2, 51, N).

% Productive modes: max_hand(+N, -M), max_hand(-N, +M), max_hand(-N, -M).
max_hand(N, M) :-
    player_count(N),
    Quotient is 51 // N,
    ( Quotient < 7 -> M = Quotient ; M = 7 ).

hand_schedule(N, Schedule) :-
    max_hand(N, M),
    range_up(1, M, Ascending),
    DownStart is M - 1,
    range_down(DownStart, 1, Descending),
    append(Ascending, Descending, Schedule).

range_up(Current, End, []) :- Current > End.
range_up(Current, End, [Current|Rest]) :-
    Current =< End,
    Next is Current + 1,
    range_up(Next, End, Rest).

range_down(Current, End, []) :- Current < End.
range_down(Current, End, [Current|Rest]) :-
    Current >= End,
    Next is Current - 1,
    range_down(Next, End, Rest).

% R-GAME-004, R-ADVANCE-002..003: seats are integers 0..N-1 and left/4 is
% modular successor. Later dealer choice never depends on setup randomness.
seat(N, Seat) :-
    player_count(N),
    Last is N - 1,
    between(0, Last, Seat).

left(N, Seat, Left) :-
    seat(N, Seat),
    Left is (Seat + 1) mod N.

dealer_for_round(N, FirstDealer, RoundNumber, Dealer) :-
    hand_schedule(N, Schedule),
    nth1_rel(RoundNumber, Schedule, _),
    seat(N, FirstDealer),
    Offset is RoundNumber - 1,
    Dealer is (FirstDealer + Offset) mod N.

score_sheet_row(N, FirstDealer, RoundNumber, Dealer, HandSize) :-
    hand_schedule(N, Schedule),
    nth1_rel(RoundNumber, Schedule, HandSize),
    dealer_for_round(N, FirstDealer, RoundNumber, Dealer).

nth1_rel(1, [Value|_], Value).
nth1_rel(Index, [_|Rest], Value) :-
    nth1_rel(Previous, Rest, Value),
    Index is Previous + 1.

% R-GAME-002, R-TRICK-010..011 ------------------------------------------------

suit(clubs).
suit(diamonds).
suit(hearts).
suit(spades).

rank(Rank) :- between(2, 14, Rank).

card(card(Suit, Rank)) :- suit(Suit), rank(Rank).

standard_deck(Deck) :-
    suits(Suits),
    deck_for_suits(Suits, Deck).

suits([clubs, diamonds, hearts, spades]).

deck_for_suits([], []).
deck_for_suits([Suit|Suits], Deck) :-
    cards_for_suit(Suit, 2, SuitCards),
    deck_for_suits(Suits, OtherCards),
    append(SuitCards, OtherCards, Deck).

cards_for_suit(_, 15, []).
cards_for_suit(Suit, Rank, [card(Suit, Rank)|Cards]) :-
    Rank =< 14,
    Next is Rank + 1,
    cards_for_suit(Suit, Next, Cards).

% R-DEAL-001..004: whole-deal relation equivalent to dealing one layer at a
% time clockwise from the dealer's left. Productive forward mode requires the
% finite N/H/Dealer and ordered input Deck.
deal_round(N, HandSize, Dealer, Deck, Hands, Trump, Undealt) :-
    player_count(N),
    max_hand(N, Maximum),
    between(1, Maximum, HandSize),
    seat(N, Dealer),
    empty_hands(N, EmptyHands),
    left(N, Dealer, First),
    deal_layers(HandSize, N, First, Deck, EmptyHands, AfterHands, Hands),
    AfterHands = [Trump|Undealt],
    card(Trump).

empty_hands(0, []).
empty_hands(N, [[]|Hands]) :-
    N > 0,
    Previous is N - 1,
    empty_hands(Previous, Hands).

deal_layers(0, _, _, Deck, Hands, Deck, Hands).
deal_layers(Layers, N, First, Deck0, Hands0, Deck, Hands) :-
    Layers > 0,
    deal_one_layer(N, N, First, Deck0, Hands0, Deck1, Hands1),
    Remaining is Layers - 1,
    deal_layers(Remaining, N, First, Deck1, Hands1, Deck, Hands).

deal_one_layer(0, _, _, Deck, Hands, Deck, Hands).
deal_one_layer(Remaining, N, Seat, [Card|Deck0], Hands0, Deck, Hands) :-
    Remaining > 0,
    append_card_at(Seat, Card, Hands0, Hands1),
    NextSeat is (Seat + 1) mod N,
    More is Remaining - 1,
    deal_one_layer(More, N, NextSeat, Deck0, Hands1, Deck, Hands).

append_card_at(0, Card, [Hand|Hands], [Updated|Hands]) :-
    append(Hand, [Card], Updated).
append_card_at(Index, Card, [Hand|Hands], [Hand|UpdatedHands]) :-
    Index > 0,
    Previous is Index - 1,
    append_card_at(Previous, Card, Hands, UpdatedHands).

% R-BID-002..005 ---------------------------------------------------------------

legal_bid(HandSize, Bid) :- between(0, HandSize, Bid).

% R-TRICK-003..011: legal_play is productive for +Hand,+LeadSuit,-Card and
% +Hand,+LeadSuit,+Card. `none` denotes the unconstrained leader action.
legal_play(Hand, none, Card) :- member(Card, Hand).
legal_play(Hand, LeadSuit, Card) :-
    LeadSuit \= none,
    member(Card, Hand),
    ( has_suit(Hand, LeadSuit) -> Card = card(LeadSuit, _) ; true ).

has_suit([card(Suit, _)|_], Suit).
has_suit([card(Other, _)|Cards], Suit) :-
    Other \= Suit,
    has_suit(Cards, Suit).

% Plays are play(Player, card(Suit,Rank)) in clockwise order. Productive mode:
% trick_winner(+Trump,+Plays,-Winner).
trick_winner(TrumpSuit, [play(Player, Card)|Plays], Winner) :-
    Card = card(LeadSuit, _),
    best_play(TrumpSuit, LeadSuit, Plays, play(Player, Card), play(Winner, _)).

best_play(_, _, [], Best, Best).
best_play(Trump, Lead, [Candidate|Plays], Current, Best) :-
    ( play_beats(Trump, Lead, Candidate, Current) -> Next = Candidate ; Next = Current ),
    best_play(Trump, Lead, Plays, Next, Best).

play_beats(Trump, Lead, play(_, Candidate), play(_, Current)) :-
    card_strength(Trump, Lead, Candidate, CandidateClass, CandidateRank),
    card_strength(Trump, Lead, Current, CurrentClass, CurrentRank),
    ( CandidateClass > CurrentClass
    ; CandidateClass =:= CurrentClass, CandidateRank > CurrentRank
    ).

card_strength(Trump, _, card(Trump, Rank), 2, Rank).
card_strength(Trump, Lead, card(Lead, Rank), 1, Rank) :- Lead \= Trump.
card_strength(Trump, Lead, card(Suit, Rank), 0, Rank) :-
    Suit \= Trump,
    Suit \= Lead.

% R-SCORE-001..005, R-MONEY-001..002. This finite generator is relational in
% Bid and Tricks: reverse score queries enumerate all rule-consistent causes.
round_score(HandSize, Bid, Tricks, Points, Cell, MissDimes) :-
    between(0, HandSize, Bid),
    between(0, HandSize, Tricks),
    score_case(HandSize, Bid, Tricks, Points, Cell, MissDimes).

score_case(_, Bid, Tricks, 0, poche, 1) :- Tricks \= Bid.
score_case(HandSize, Bid, Bid, Points, all_tricks(Bid), 0) :-
    Bid =:= HandSize,
    Points is 20 + Bid.
score_case(HandSize, Bid, Bid, Points, exact(Bid), 0) :-
    Bid < HandSize,
    Points is 10 + Bid.

% R-GAME-005, R-FINISH-002..004, R-MONEY-003 -------------------------------

game_winners(Scores, Winners) :-
    maximum(Scores, Maximum),
    winner_indices(Scores, Maximum, 0, Winners).

maximum([Value|Values], Maximum) :- maximum_(Values, Value, Maximum).
maximum_([], Maximum, Maximum).
maximum_([Value|Values], Current, Maximum) :-
    ( Value > Current -> Next = Value ; Next = Current ),
    maximum_(Values, Next, Maximum).

winner_indices([], _, _, []).
winner_indices([Score|Scores], Maximum, Index, Winners) :-
    NextIndex is Index + 1,
    ( Score =:= Maximum -> Winners = [Index|Rest] ; Winners = Rest ),
    winner_indices(Scores, Maximum, NextIndex, Rest).

opening_ante_cents(N, Cents) :- player_count(N), Cents is 25 * N.

pot_division(TotalCents, Winners, ShareCents, RemainderCents) :-
    length(Winners, WinnerCount),
    WinnerCount > 0,
    ShareCents is TotalCents // WinnerCount,
    RemainderCents is TotalCents mod WinnerCount.

% R-RANDOM-002..003,006: First Jack keeps the actual seat-ordered draw support.
% RestoredDeck equals the original multiset before the required reshuffle;
% probability is deliberately not claimed uniform.
first_jack(Players, Deck, Selected, SelectionCards, RestoredDeck) :-
    Players = [_|_],
    first_jack_(Deck, Players, Players, [], Selected, SelectionCards),
    RestoredDeck = Deck.

first_jack_([card(Suit, 11)|_], [Player|_], _, Drawn, Player, SelectionCards) :-
    reverse([card(Suit, 11)|Drawn], SelectionCards).
first_jack_([card(Suit, Rank)|Deck], [_|Seats], Players, Drawn, Selected, Cards) :-
    Rank \= 11,
    next_seats(Seats, Players, NextSeats),
    first_jack_(Deck, NextSeats, Players, [card(Suit, Rank)|Drawn], Selected, Cards).

next_seats([], Players, Players).
next_seats([Seat|Seats], _, [Seat|Seats]).

% R-RANDOM-004..006: each High Card draw has exactly one card per current
% contender; tied maximum ranks alone recur. List order is semantically inert.
high_card_selection(Contenders, Draws, Selected) :-
    Contenders = [_|_],
    all_unique(Contenders),
    high_card_selection_(Contenders, Draws, Selected, []).

high_card_selection_(Contenders, [Draw|Draws], Selected, Used0) :-
    length(Contenders, DrawCount),
    length(Draw, DrawCount),
    draw_cards(Contenders, Draw, Cards),
    all_unique(Cards),
    no_members(Cards, Used0),
    append(Cards, Used0, Used),
    high_card_leaders(Draw, Leaders),
    ( Leaders = [Selected], Draws = []
    ; Leaders = [_,_|_], high_card_selection_(Leaders, Draws, Selected, Used)
    ).

draw_cards([], _, []).
draw_cards([Player|Players], Draw, [Card|Cards]) :-
    member(play(Player, Card), Draw),
    card(Card),
    draw_cards(Players, Draw, Cards).

high_card_leaders(Draw, Leaders) :-
    draw_ranks(Draw, Ranks),
    maximum(Ranks, Maximum),
    players_with_rank(Draw, Maximum, Leaders).

draw_ranks([], []).
draw_ranks([play(_, card(_, Rank))|Draw], [Rank|Ranks]) :- draw_ranks(Draw, Ranks).

players_with_rank([], _, []).
players_with_rank([play(Player, card(_, Rank))|Draw], Maximum, Players) :-
    ( Rank =:= Maximum -> Players = [Player|Rest] ; Players = Rest ),
    players_with_rank(Draw, Maximum, Rest).

no_members([], _).
no_members([Card|Cards], Used) :-
    \+ memberchk(Card, Used),
    no_members(Cards, Used).

% Executable one-round transition corpus --------------------------------------
%
% round_state(Phase,Dealer,HandSize,Undealt,Hands,Trump,Bids,Leader,Center,
%             Captured,Won,Scores,PotCents)
%
% Scope: two players, one-card complete round, full card identity. This scope
% supplies productive forward and reverse step/3 queries; generic schedules,
% deals, legal plays, winners, and scoring remain separate relations above.

initial_round_state(Deck,
    round_state(deal, 1, 1, Deck, [[],[]], none, [none,none], 0, [],
                [[],[]], [0,0], [0,0], 50)) :-
    standard_deck(Deck).

legal_action(round_state(deal, _, _, _, _, _, _, _, _, _, _, _, _), deal).
legal_action(round_state(bid(Player), _, HandSize, _, _, _, Bids, _, _, _, _, _, _),
             bid(Player, Bid)) :-
    nth0_rel(Player, Bids, none),
    legal_bid(HandSize, Bid).
legal_action(round_state(play(Player), _, _, _, Hands, _, _, Leader, Center, _, _, _, _),
             play(Player, Card)) :-
    nth0_rel(Player, Hands, Hand),
    ( Center = [] -> LeadSuit = none
    ; Center = [play(Leader, card(LeadSuit, _))]
    ),
    legal_play(Hand, LeadSuit, Card).
legal_action(round_state(collect(_), _, _, _, _, _, _, _, _, _, _, _, _), collect).
legal_action(round_state(settle, _, _, _, _, _, _, _, _, _, _, _, _), settle).

step(round_state(deal, Dealer, 1, Deck, [[],[]], none, [none,none], Leader, [],
                 [[],[]], [0,0], Scores, Pot),
     deal,
     round_state(bid(0), Dealer, 1, Undealt, Hands, Trump, [none,none], Leader, [],
                 [[],[]], [0,0], Scores, Pot)) :-
    legal_action(round_state(deal, Dealer, 1, Deck, [[],[]], none, [none,none],
                             Leader, [], [[],[]], [0,0], Scores, Pot), deal),
    deal_round(2, 1, Dealer, Deck, Hands, Trump, Undealt).

step(round_state(bid(0), Dealer, HandSize, Deck, Hands, Trump, [none,none], Leader,
                 Center, Captured, Won, Scores, Pot),
     bid(0, Bid),
     round_state(bid(1), Dealer, HandSize, Deck, Hands, Trump, [Bid,none], Leader,
                 Center, Captured, Won, Scores, Pot)) :-
    legal_bid(HandSize, Bid).

step(round_state(bid(1), Dealer, HandSize, Deck, Hands, Trump, [Bid0,none], Leader,
                 Center, Captured, Won, Scores, Pot),
     bid(1, Bid),
     round_state(play(Leader), Dealer, HandSize, Deck, Hands, Trump, [Bid0,Bid], Leader,
                 Center, Captured, Won, Scores, Pot)) :-
    legal_bid(HandSize, Bid).

step(round_state(play(Player), Dealer, HandSize, Deck, Hands0, Trump, Bids, Leader,
                 [], Captured, Won, Scores, Pot),
     play(Player, Card),
     round_state(play(Other), Dealer, HandSize, Deck, Hands, Trump, Bids, Leader,
                 [play(Player,Card)], Captured, Won, Scores, Pot)) :-
    Player = Leader,
    other_player(Player, Other),
    remove_player_card(Player, Card, Hands0, Hands),
    legal_play_from_hands(Hands0, Player, none, Card).

step(round_state(play(Player), Dealer, HandSize, Deck, Hands0, TrumpCard, Bids, Leader,
                 [play(Leader,LeadCard)], Captured, Won, Scores, Pot),
     play(Player, Card),
     round_state(collect(Winner), Dealer, HandSize, Deck, Hands, TrumpCard, Bids, Leader,
                 [play(Leader,LeadCard),play(Player,Card)], Captured, Won, Scores, Pot)) :-
    other_player(Leader, Player),
    LeadCard = card(LeadSuit, _),
    legal_play_from_hands(Hands0, Player, LeadSuit, Card),
    remove_player_card(Player, Card, Hands0, Hands),
    TrumpCard = card(TrumpSuit, _),
    trick_winner(TrumpSuit, [play(Leader,LeadCard),play(Player,Card)], Winner).

step(round_state(collect(Winner), Dealer, HandSize, Deck, Hands, Trump, Bids, _,
                 Center, Captured0, Won0, Scores, Pot),
     collect,
     round_state(settle, Dealer, HandSize, Deck, Hands, Trump, Bids, Winner,
                 [], Captured, Won, Scores, Pot)) :-
    collect_cards(Winner, Center, Captured0, Captured),
    increment_at(Winner, Won0, Won).

step(round_state(settle, Dealer, HandSize, Deck, Hands, Trump, [Bid0,Bid1], Leader,
                 [], Captured, [Won0,Won1], [Total0,Total1], Pot0),
     settle,
     round_state(complete, Dealer, HandSize, Deck, Hands, Trump, [Bid0,Bid1], Leader,
                 [], Captured, [Won0,Won1], [Next0,Next1], Pot)) :-
    round_score(HandSize, Bid0, Won0, Score0, _, Miss0),
    round_score(HandSize, Bid1, Won1, Score1, _, Miss1),
    Next0 is Total0 + Score0,
    Next1 is Total1 + Score1,
    Pot is Pot0 + (10 * Miss0) + (10 * Miss1).

predecessor(State, Action, Previous) :- step(Previous, Action, State).

replay(State, [], State).
replay(State, [Action|Actions], Final) :-
    step(State, Action, Next),
    replay(Next, Actions, Final).

other_player(0, 1).
other_player(1, 0).

nth0_rel(0, [Value|_], Value).
nth0_rel(Index, [_|Values], Value) :-
    Index > 0,
    Previous is Index - 1,
    nth0_rel(Previous, Values, Value).

replace_nth0(0, Value, [_|Values], [Value|Values]).
replace_nth0(Index, Value, [Head|Values], [Head|Updated]) :-
    Index > 0,
    Previous is Index - 1,
    replace_nth0(Previous, Value, Values, Updated).

remove_player_card(Player, Card, Hands0, Hands) :-
    nth0_rel(Player, Hands0, Hand0),
    select(Card, Hand0, Hand),
    replace_nth0(Player, Hand, Hands0, Hands).

legal_play_from_hands(Hands, Player, LeadSuit, Card) :-
    nth0_rel(Player, Hands, Hand),
    legal_play(Hand, LeadSuit, Card).

collect_cards(Winner, Center, Captured0, Captured) :-
    center_cards(Center, Cards),
    nth0_rel(Winner, Captured0, Existing),
    append(Existing, Cards, Updated),
    replace_nth0(Winner, Updated, Captured0, Captured).

center_cards([], []).
center_cards([play(_,Card)|Plays], [Card|Cards]) :- center_cards(Plays, Cards).

increment_at(Index, Values0, Values) :-
    nth0_rel(Index, Values0, Value),
    Next is Value + 1,
    replace_nth0(Index, Next, Values0, Values).

% Card conservation makes malformed round-state terms rejectable. Because the
% concatenated zones contain 52 distinct valid cards, they are the full deck.
valid_round_state(round_state(_, _, _, Deck, Hands, Trump, _, _, Center,
                              Captured, _, _, _)) :-
    round_state_cards(Deck, Hands, Trump, Center, Captured, AllCards),
    length(AllCards, 52),
    all_cards_valid(AllCards),
    all_unique(AllCards).

round_state_cards(Deck, Hands, Trump, Center, Captured, AllCards) :-
    append(Hands, HandCards),
    center_cards(Center, CenterCards),
    append(Captured, CapturedCards),
    trump_cards(Trump, TrumpCards),
    append([Deck,HandCards,TrumpCards,CenterCards,CapturedCards], AllCards).

% R-ADVANCE-001: this semantic restoration relation returns every card from
% every zone; a physical reshuffle may choose any permutation afterward.
restore_round(round_state(_, _, _, Deck, Hands, Trump, _, _, Center,
                          Captured, _, _, _), RestoredDeck) :-
    round_state_cards(Deck, Hands, Trump, Center, Captured, RestoredDeck),
    length(RestoredDeck, 52),
    all_cards_valid(RestoredDeck),
    all_unique(RestoredDeck).

trump_cards(none, []).
trump_cards(card(Suit, Rank), [card(Suit, Rank)]).

all_cards_valid([]).
all_cards_valid([Card|Cards]) :- card(Card), all_cards_valid(Cards).

all_unique([]).
all_unique([Card|Cards]) :-
    \+ memberchk(Card, Cards),
    all_unique(Cards).

% Native query corpus ----------------------------------------------------------

run_oracle_tests :-
    test_names(Names),
    run_named_tests(Names, 0, Count),
    format("POCHE_PROLOG_OK tests=~d~n", [Count]).

test_names([
    standard_deck,
    all_player_schedules,
    score_sheet_rotation,
    generic_deal,
    bid_domain,
    follow_suit,
    void_play,
    trump_winner,
    lead_winner,
    scoring_and_reverse_scoring,
    money_and_shared_winners,
    first_jack_selection,
    repeated_high_card,
    forward_round_trace,
    reverse_bid_predecessor,
    reverse_play_predecessor
]).

run_named_tests([], Count, Count).
run_named_tests([Name|Names], Count0, Count) :-
    ( once(oracle_test(Name)) ->
        Count1 is Count0 + 1,
        run_named_tests(Names, Count1, Count)
    ; throw(error(poche_oracle_test_failed(Name), run_oracle_tests/0))
    ).

oracle_test(standard_deck) :-
    standard_deck(Deck),
    length(Deck, 52),
    all_unique(Deck),
    memberchk(card(clubs,2), Deck),
    memberchk(card(spades,14), Deck).

oracle_test(all_player_schedules) :-
    all_schedules_valid(2),
    max_hand(2, 7),
    hand_schedule(2, [1,2,3,4,5,6,7,6,5,4,3,2,1]),
    max_hand(51, 1),
    hand_schedule(51, [1]).

all_schedules_valid(52).
all_schedules_valid(N) :-
    N < 52,
    max_hand(N, Maximum),
    N * Maximum + 1 =< 52,
    length_schedule(N, Length),
    Length =:= (2 * Maximum) - 1,
    Next is N + 1,
    all_schedules_valid(Next).

length_schedule(N, Length) :- hand_schedule(N, Schedule), length(Schedule, Length).

oracle_test(score_sheet_rotation) :-
    score_sheet_row(3, 1, 1, 1, 1),
    score_sheet_row(3, 1, 2, 2, 2),
    score_sheet_row(3, 1, 3, 0, 3).

oracle_test(generic_deal) :-
    standard_deck(Deck),
    deal_round(3, 2, 2, Deck, Hands, Trump, Undealt),
    Hands = [[card(clubs,2),card(clubs,5)],
             [card(clubs,3),card(clubs,6)],
             [card(clubs,4),card(clubs,7)]],
    Trump = card(clubs,8),
    length(Undealt, 45).

oracle_test(bid_domain) :-
    findall(Bid, legal_bid(2, Bid), [0,1,2]),
    legal_bid(1, 0),
    legal_bid(1, 1). % totals may therefore be 2 for one available trick

oracle_test(follow_suit) :-
    Hand = [card(hearts,2),card(spades,14)],
    findall(Card, legal_play(Hand, hearts, Card), [card(hearts,2)]).

oracle_test(void_play) :-
    Hand = [card(clubs,2),card(spades,14)],
    findall(Card, legal_play(Hand, hearts, Card),
            [card(clubs,2),card(spades,14)]).

oracle_test(trump_winner) :-
    Plays = [play(0,card(hearts,14)), play(1,card(spades,2))],
    trick_winner(spades, Plays, 1).

oracle_test(lead_winner) :-
    Plays = [play(0,card(hearts,10)), play(1,card(clubs,14))],
    trick_winner(spades, Plays, 0).

oracle_test(scoring_and_reverse_scoring) :-
    round_score(3, 2, 1, 0, poche, 1),
    round_score(3, 0, 0, 10, exact(0), 0),
    round_score(3, 3, 3, 23, all_tricks(3), 0),
    findall(Bid-Tricks, round_score(4, Bid, Tricks, 13, exact(3), 0), [3-3]).

oracle_test(money_and_shared_winners) :-
    opening_ante_cents(3, 75),
    game_winners([20,30,30], [1,2]),
    pot_division(80, [1,2,3], 26, 2).

oracle_test(first_jack_selection) :-
    Deck = [card(clubs,2),card(diamonds,5),card(hearts,7),card(spades,11),card(clubs,14)],
    first_jack([a,b,c], Deck, a, SelectionCards, Deck),
    SelectionCards = [card(clubs,2),card(diamonds,5),card(hearts,7),card(spades,11)].

oracle_test(repeated_high_card) :-
    Draws = [
        [play(a,card(clubs,14)),play(b,card(diamonds,14)),play(c,card(hearts,10))],
        [play(a,card(spades,8)),play(b,card(clubs,9))]
    ],
    high_card_selection([a,b,c], Draws, b),
    high_card_selection([c,b,a],
        [[play(c,card(hearts,10)),play(b,card(diamonds,14)),play(a,card(clubs,14))],
         [play(b,card(clubs,9)),play(a,card(spades,8))]], b).

oracle_test(forward_round_trace) :-
    canonical_round(Initial, Actions, Final),
    replay(Initial, Actions, Final),
    Final = round_state(complete,1,1,_,[[],[]],card(clubs,4),[0,1],1,[],_,[0,1],
                        [10,21],50),
    valid_trace_states(Initial, Actions),
    restore_round(Final, Restored),
    length(Restored, 52),
    all_unique(Restored).

canonical_round(Initial,
    [deal,bid(0,0),bid(1,1),play(0,card(clubs,2)),play(1,card(clubs,3)),collect,settle],
    Final) :-
    standard_deck(Deck),
    initial_round_state(Deck, Initial),
    replay(Initial,
        [deal,bid(0,0),bid(1,1),play(0,card(clubs,2)),play(1,card(clubs,3)),collect,settle],
        Final).

valid_trace_states(State, []) :- valid_round_state(State).
valid_trace_states(State, [Action|Actions]) :-
    valid_round_state(State),
    step(State, Action, Next),
    valid_trace_states(Next, Actions).

oracle_test(reverse_bid_predecessor) :-
    standard_deck(Deck),
    initial_round_state(Deck, Initial),
    step(Initial, deal, Bid0),
    step(Bid0, bid(0,0), Bid1),
    predecessor(Bid1, bid(0,0), Recovered),
    Recovered = Bid0.

oracle_test(reverse_play_predecessor) :-
    standard_deck(Deck),
    initial_round_state(Deck, Initial),
    replay(Initial, [deal,bid(0,0),bid(1,1)], BeforePlay),
    step(BeforePlay, play(0,card(clubs,2)), AfterPlay),
    predecessor(AfterPlay, play(0,card(clubs,2)), Recovered),
    Recovered = BeforePlay.
