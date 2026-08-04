#import "@preview/charged-ieee:0.1.4": ieee

#show: ieee.with(
  title: [Poche: Formal Rules],
  abstract: [
    Poche is a round-based trick-taking game in which players predict how many tricks they will win, then score for meeting those predictions. This document formalizes the intended tabletop rules for the 52-card family game as a sequence of physical phases: prepare the table, deal, bid, play, score, and continue until the game ends.
  ],
  authors: (
    (
      name: "TeamDman",
      location: [Ontario, Canada],
    ),
  ),
)

#let blue = rgb("#1f4d78")
#let dark = rgb("#0b2545")
#let muted = rgb("#555f6d")
#let pale-blue = rgb("#e8eef5")
#let pale-gold = rgb("#fff8e8")
#let gold = rgb("#7a5a00")

#let callout(title, body, fill: pale-blue, accent: blue) = block(
  fill: fill,
  inset: 8pt,
  radius: 3pt,
  stroke: (left: 2pt + accent),
)[
  *#title.* #body
]

#show link: it => it

= Overview

*Formal rules for the 52-card trick-taking game*  \
Version 1.0 | August 3, 2026

#callout(
  [Rulebook scope],
  [This document formalizes the intended tabletop rules described in the Poche project. Where the prototype code and the family-rule notes differ, these tabletop rules are authoritative for play. Remaining assumptions are listed in @sec:clarifications.],
  fill: pale-gold,
  accent: gold,
)

== How to read the rules

Play follows the phases in order:

#align(center, [PREPARE THE TABLE -> DEAL A ROUND -> COLLECT BIDS -> PLAY THE TRICKS -> SCORE AND SETTLE -> ADVANCE TO THE NEXT ROUND])

The detailed rules introduce each object and decision at the point where the players need it. The table-side reference near the end repeats the sequence in compact form.

== At a glance

- *Players:* $n$ players, where $2 <= n <= 51$
- *Deck:* Standard 52-card deck, no jokers
- *Rounds:* A rising-and-falling schedule defined in @sec:hand-size-schedule
- *Scorekeeper:* One player chosen by volunteer consensus, or randomly if nobody volunteers
- *Opening ante:* 25 cents per player
- *Missed-bid payment:* 10 cents to the pot when a player's tricks taken differ from their bid
- *Objective:* Finish with the highest score

== Objective

Poche is a round-based trick-taking game in which players predict how many tricks they will win. A complete game rises from one-card hands to the largest hand size that fits the deck, capped at seven cards, and then returns to one card before the winner is declared.

The player with the highest total score after the final scheduled round wins.

= Prepare the table

This phase happens once, before the first round.

1. *Arrange the table.* Players sit around the table and agree on the clockwise seat order. The deck resides near the dealer. Keep the shared central area clear for the cards played to the current trick. A player's left-hand neighbor is next in the agreed seat order.
2. *Set out the coin jars.* Each player places their glass jar in front of their seat. The jar is adorned with a picture of its owner and the owner's name. Remove the lid and pour a convenient supply of the jar's coins into it; use the lid as a small coin tray during play, replenishing it from the jar when needed. Keep the jar and lid in front of their owner.
3. *Choose the scorekeeper.* Ask for volunteers. If there is one volunteer, that player serves. If there are several volunteers, the players reach consensus on one of them. If nobody volunteers, choose one of the methods in @sec:random-player-selection and use it to select a player. The scorekeeper is responsible for writing with the pen and paper to track the game's progress while also participating in play.
4. *Choose the first dealer.* Use the *First Jack* method in @sec:random-player-selection: the first player to receive a Jack becomes the dealer. Return all selection cards to the deck and shuffle. The first dealer deals the first round; after that, the deal rotates as described in @sec:advance-round.
5. *Pay the ante.* Each player pays 25 cents from their coin tray into the bowl, which becomes the communal pot for the game.
6. *Prepare the score sheet.* Use paper and a pen. Reserve the leftmost column or margin for the round label, and write each player's name across the top with one column per player. Leave room for every scheduled round and a final total.

== Score sheet layout

The scorekeeper keeps the sheet visible to the table. Each round gets one row. The first three columns identify the round: round number, dealer, and cards. Player names begin to their right. At the beginning of a round, the scorekeeper writes the round number, the dealer's name, and the number of cards dealt to each player.

For example, with Joe, Mark, and Diana seated in that order, the hand-size labels rise to seven cards and then descend:

#let game-seed = 20260803

#let random-step(state, upper) = {
  let next = calc.rem(state * 1103515245 + 12345, 2147483647)
  (next, calc.rem(next, upper))
}

#let round-tricks(seed, hand-size) = {
  let state = seed
  let joe = 0
  let mark = 0
  let diana = 0
  for _ in range(hand-size) {
    let (next, winner) = random-step(state, 3)
    state = next
    if winner == 0 {
      joe = joe + 1
    } else if winner == 1 {
      mark = mark + 1
    } else {
      diana = diana + 1
    }
  }
  (state, (joe, mark, diana))
}

#let score-value(bid, tricks, hand-size) = {
  if tricks != bid {
    0
  } else if tricks == hand-size {
    20 + bid
  } else {
    10 + bid
  }
}

#let score-cell(bid, tricks, hand-size) = {
  if tricks != bid {
    [●]
  } else if tricks == hand-size {
    [#(str(2) + str(bid))]
  } else {
    [#(str(1) + str(bid))]
  }
}

#let completed-game() = {
  let rows = ()
  let state = game-seed
  let total-joe = 0
  let total-mark = 0
  let total-diana = 0

  for round in range(1, 14) {
    let hand-size = if round <= 7 { round } else { 14 - round }
    let dealer = if calc.rem(round - 1, 3) == 0 {
      "Joe"
    } else if calc.rem(round - 1, 3) == 1 {
      "Mark"
    } else {
      "Diana"
    }

    let (next, tricks) = round-tricks(state + round * 997, hand-size)
    state = next

    let (next, flag-joe) = random-step(state, 2)
    state = next
    let (next, flag-mark) = random-step(state, 2)
    state = next
    let (next, flag-diana) = random-step(state, 2)
    state = next

    let bid-joe = if flag-joe == 0 {
      tricks.at(0)
    } else {
      calc.rem(tricks.at(0) + 1, hand-size + 1)
    }
    let bid-mark = if flag-mark == 0 {
      tricks.at(1)
    } else {
      calc.rem(tricks.at(1) + 1, hand-size + 1)
    }
    let bid-diana = if flag-diana == 0 {
      tricks.at(2)
    } else {
      calc.rem(tricks.at(2) + 1, hand-size + 1)
    }

    let joe-value = score-value(bid-joe, tricks.at(0), hand-size)
    let mark-value = score-value(bid-mark, tricks.at(1), hand-size)
    let diana-value = score-value(bid-diana, tricks.at(2), hand-size)
    total-joe = total-joe + joe-value
    total-mark = total-mark + mark-value
    total-diana = total-diana + diana-value

    let row = (
      text(str(round)),
      text(dealer),
      text(str(hand-size)),
      score-cell(bid-joe, tricks.at(0), hand-size),
      score-cell(bid-mark, tricks.at(1), hand-size),
      score-cell(bid-diana, tricks.at(2), hand-size),
    )
    rows = rows + row
  }

  (rows, (total-joe, total-mark, total-diana))
}

#let (completed-rows, completed-totals) = completed-game()

#figure(
  table(
    columns: (0.6fr, 1fr, 0.6fr, 1fr, 1fr, 1fr),
    align: (center, left, center, left, left, left),
    inset: 6pt,
    stroke: (x, y) => if y == 0 {
      (bottom: 0.8pt + blue)
    } else {
      0.5pt + luma(85%)
    },
    fill: (x, y) => if y == 0 {
      pale-blue
    } else if calc.rem(y, 2) == 0 {
      rgb("#f7f9fb")
    } else {
      white
    },
    table.header[Round][Dealer][Cards][Joe][Mark][Diana],
    [1], [Joe], [1], [], [], [],
    [2], [Mark], [2], [], [], [],
    [3], [Diana], [3], [], [], [],
    [4], [Joe], [4], [], [], [],
    [5], [Mark], [5], [], [], [],
    [6], [Diana], [6], [], [], [],
    [7], [Joe], [7], [], [], [],
    [8], [Mark], [6], [], [], [],
    [9], [Diana], [5], [], [], [],
    [10], [Joe], [4], [], [], [],
    [11], [Mark], [3], [], [], [],
    [12], [Diana], [2], [], [], [],
    [13], [Joe], [1], [], [], [],
    [Total], [], [], [], [], [],
  ),
  caption: [Initial paper score sheet],
)

The completed sheet below uses the fixed seed `#game-seed` to generate an illustrative three-player game. The generator assigns each trick to exactly one player, so the trick counts for every row sum to that row's hand size. It then generates bids and replaces each bid with the corresponding final score-cell notation.

#figure(
  table(
    columns: (0.6fr, 1fr, 0.6fr, 1fr, 1fr, 1fr),
    align: (center, left, center, left, left, left),
    inset: 6pt,
    stroke: (x, y) => if y == 0 {
      (bottom: 0.8pt + blue)
    } else {
      0.5pt + luma(85%)
    },
    fill: (x, y) => if y == 0 {
      pale-blue
    } else if calc.rem(y, 2) == 0 {
      rgb("#f7f9fb")
    } else {
      white
    },
    table.header[Round][Dealer][Cards][Joe][Mark][Diana],
    ..completed-rows,
    [Total], [], [], text(str(completed-totals.at(0))), text(str(completed-totals.at(1))), text(str(completed-totals.at(2))),
  ),
  caption: [Completed paper score sheet generated from seed #game-seed],
)

= Deal a round <sec:deal-round>

A *round* is one complete hand of cards, from the deal through scoring. The *dealer* is responsible for dealing that round. The player immediately to the dealer's left begins the dealing and play sequence.

Before dealing, use the hand-size sequence in @sec:hand-size-schedule to determine the scheduled number of cards for this round.

== Deal and reveal trump

1. Deal one card at a time, beginning with the player to the dealer's left and continuing clockwise until every player has the scheduled number of cards.
2. Turn the next card of the deck face-up and place it on top of the deck. Its suit is *trump* for the round: a trump card beats every non-trump card. Keep the revealed card face-up on top of the deck, near the dealer, and out of the players' hands until the round is scored.
3. Write the round number, dealer name, and current hand size in the first three score-sheet columns.

= Collect bids

A *bid* is a player's announced prediction for the number of tricks they will win in the current round.

1. Beginning with the player to the dealer's left and continuing clockwise, each player announces a whole-number bid from zero through the current hand size. The dealer bids last.
2. As each bid is announced, the scorekeeper writes the number in that player's cell on the current round row. The bid remains fixed for the round.
3. There is no restriction requiring the total of all bids to equal, or differ from, the number of tricks available.
4. A bid of zero is valid. Taking no tricks after bidding zero is a successful bid.

= Play the tricks

A *trick* is one cycle in which each player plays one card. A round contains one trick for each card in a player's hand.

== Start a trick

The first trick begins with the player to the dealer's left. The player who wins a trick becomes the *leader* and leads the next trick. The leader may play any card from their hand; the suit of that card becomes the *lead suit* for the trick.

Players play one at a time clockwise from the leader. Cards for the current trick go face-up into the shared center.

== Follow the lead suit

Each subsequent player must play a card of the lead suit if they have one. A player who has no card of the lead suit may play any card, including a trump card.

== Determine the winner

- If one or more trump cards were played, the highest-ranked trump card wins.
- If no trump was played, the highest-ranked card of the lead suit wins.
- Cards of other non-trump suits cannot win the trick.
- Rank order from low to high is: 2, 3, 4, 5, 6, 7, 8, 9, 10, Jack, Queen, King, Ace.
- Suits have no ranking relative to one another; only the lead suit and trump status matter.

The winner gathers the cards into a separate face-down trick pile in front of themselves and counts it as one trick toward their bid. Keep all piles separate until scoring so that anyone at the table can verify the trick count.

#callout(
  [Example],
  [Hearts is led and Clubs is trump. A player who has no Hearts may play the Two of Clubs and win over an Ace of Hearts, because any trump beats every non-trump card.],
)

= Score and settle

After the final trick, the scorekeeper counts the trick piles with the table watching and compares each player's tricks taken with their recorded bid. To *poche* is to fail to take exactly the number of tricks bid. A player who takes exactly their bid has made a successful bid; taking every trick is the all-tricks outcome.

== Update the score cells

The scorekeeper updates the same cell that held the bid:

- *Poche (bid failed):* If tricks taken does not match the bid, overwrite the number with `●`. The entry contributes zero points.
- *Bid succeeded:* If the player takes the bid but not all tricks, prefix the bid with `1`. For example, a bid of 3 becomes `13`, worth $10 + 3$ points.
- *All tricks taken:* If the player takes every trick, prefix the bid with `2`. For example, a bid of 3 becomes `23`, worth $20 + 3$ points.

The scorekeeper adds the numeric value represented by each successful entry to the running total at the bottom of that player's column. The black circle represents zero. The scorekeeper then records any required pot payment.

#figure(
  table(
    columns: (auto, 1fr, auto, auto),
    align: (left, left, right, right),
    inset: 6pt,
    stroke: (x, y) => if y == 0 {
      (bottom: 0.8pt + blue)
    } else {
      0.5pt + luma(85%)
    },
    fill: (x, y) => if y == 0 {
      pale-blue
    } else if calc.rem(y, 2) == 0 {
      rgb("#f7f9fb")
    } else {
      white
    },
    table.header[Outcome][Condition][Points][Pot payment],
    [Poche], [Tricks taken != bid], [0], [+10 cents],
    [Successful bid], [Tricks taken = bid, but not all tricks], [10 + bid], [None],
    [All tricks], [Bid = tricks taken = hand size], [20 + bid], [None],
  ),
  caption: [Round scoring],
)

== Pay into the bowl

- The opening ante was paid during table preparation: 25 cents per player into the bowl.
- After a missed bid, the player takes 10 cents from their coin tray and puts it into the bowl. A miss occurs whenever tricks taken differs from the bid; replenish the tray from the player's jar whenever necessary.
- A successful bid, including the all-tricks outcome, does not require a payment.
- The bowl is the communal pot. Unless the family has another custom, the winner takes it at the end of the game.

= Advance to the next round <sec:advance-round>

After scoring and settling the pot payment:

1. Return all played cards and the revealed trump card to the deck. Clear the round's cards and trick piles.
2. Move the deal one seat to the left. The player to the current dealer's left becomes the dealer for the next round.
3. Continue with @sec:deal-round. Use the next hand size in the schedule; the game ends after all $2m - 1$ scheduled rounds are complete.

= Finish the game

1. *Complete the final round.* The final scheduled round is the one-card round on the descending side of the hand-size sequence. Resolve its scoring as usual.
2. *Total the scores.* Add each player's successful score entries from all scheduled rounds. Do not count ante or pot payments as points.
3. *Declare the winner.* The player with the highest total score wins Poche.
4. *Resolve a tie.* If two or more players are tied for the highest score, the tied players share the win unless the table agrees before play to use a playoff round.
5. *Resolve the bowl.* By the default rule in this edition, the winner takes the communal bowl. A shared win means the tied winners divide its contents as evenly as practical.

= Table-side reference

== Round flow

#callout(
  [DEAL -> REVEAL TRUMP -> RECORD BIDS -> PLAY TRICKS -> UPDATE SCORE CELLS -> PAY MISSED BIDS -> RETURN CARDS -> PASS DEALER],
  [Use this sequence as the table's round checklist.],
)

== Before each trick

- Identify the leader.
- If you lead, play any card and establish the lead suit.
- If you follow, check whether you can follow suit. If yes, you must. If not, play any card.
- Compare trump first; otherwise compare cards in the lead suit.
- The winner takes the trick pile and leads next.

== Round checklist

- Correct hand size dealt to every player.
- Trump card revealed and visible.
- Hand-size/dealer label written in the score-sheet margin.
- Every player has announced a bid and the bid is recorded.
- Every trick contains one card from every player.
- Each player has been credited with the correct number of tricks.
- Score cells, running totals, and missed-bid payments recorded before the next deal.

= Clarifications and house conventions <sec:clarifications>

The Poche project contains both an executable rules prototype and an earlier visualization prototype. They do not fully agree on every tabletop convention. This edition makes the following choices so that a group can play without needing to interpret the code.

- *Dealer selection:* The first dealer is chosen by the "first Jack deals" method. If the table elects a different random selection method, use @sec:random-player-selection. A later dealer rotation always moves left, one seat at a time.
- *Scorecard notation:* The physical score-cell notation above is formal: `●` records a Poche (failed bid), `1` prefixed to the bid records an ordinary successful bid, and `2` prefixed to the bid records taking all tricks. These entries correspond directly to the numeric points in the scoring table.
- *Tie handling:* The project does not prescribe a final-score tiebreaker. This edition uses shared victory unless a playoff was agreed in advance.
- *Pot payout:* The project tracks a pot but does not define its final payout. This edition awards the bowl to the winner, with an even split for a shared win.
- *Prototype status:* The software prototypes are useful references for game state and flow, but this document is the authority for tabletop play where implementation details are incomplete or inconsistent.

#callout(
  [Version note],
  [This is a formalized first edition. After the family reviews it, update this section or the relevant rule text to record any house-specific conventions that differ from these defaults.],
  fill: pale-gold,
  accent: gold,
)

= Appendix: Parametric hand sizes and spreadsheet helper <sec:hand-size-schedule>

The main rules refer to the hand-size schedule without interrupting the physical flow of a round. Use this appendix when preparing the score sheet or determining how many cards to deal.

== Hand-size math

Let $n$ be the number of players. A hand of size $h$ requires $n dot h$ cards for the hands and one additional card for the revealed trump. Therefore, the largest hand size that fits the deck is

#align(center, $m = min(7, floor(51 / n))$)

The hand-size sequence is

#align(center, $1, 2, ..., m, m - 1, ..., 2, 1$)

The game therefore has $2m - 1$ rounds. If $n dot 7 + 1 <= 52$, then $m = 7$ and the familiar schedule has thirteen rounds. If a seven-card hand would not fit, the game begins descending after the largest hand size that does fit.

#figure(
  table(
    columns: (auto, auto, auto),
    align: (right, right, right),
    inset: 6pt,
    stroke: (x, y) => if y == 0 {
      (bottom: 0.8pt + blue)
    } else {
      0.5pt + luma(85%)
    },
    fill: (x, y) => if y == 0 {
      pale-blue
    } else if calc.rem(y, 2) == 0 {
      rgb("#f7f9fb")
    } else {
      white
    },
    table.header[Players $n$][Maximum hand $m$][Rounds $2m - 1$],
    [2-7], [7], [13],
    [8], [6], [11],
    [9-10], [5], [9],
    [11-12], [4], [7],
    [13-17], [3], [5],
    [18-25], [2], [3],
    [26-51], [1], [1],
  ),
  caption: [Complete parametric schedules for every supported player count],
)

== Spreadsheet helper

If player names begin in `D1` and continue in adjacent cells across row 1, enter the three formulas below in `A2`, `B2`, and `C2`, respectively, and copy each one down. These formulas use broadly supported Excel functions rather than `LET` or `HSTACK`. They discover the number of players automatically and return blanks after the final scheduled round. Keep the player names contiguous, with no other nonblank cells to their right.

#let excel-round-copy = "=IF(ROW()-1>2*MIN(7,INT(51/COUNTA($D$1:$IV$1)))-1,\"\",ROW()-1)"
#let excel-dealer-copy = "=IF($A2=\"\",\"\",INDEX($D$1:$IV$1,1,MOD($A2-1,COUNTA($D$1:$IV$1))+1))"
#let excel-cards-copy = "=IF($A2=\"\",\"\",IF($A2<=MIN(7,INT(51/COUNTA($D$1:$IV$1))),$A2,2*MIN(7,INT(51/COUNTA($D$1:$IV$1)))-$A2))"

#block(
  fill: rgb("#f7f7f7"),
  inset: (x: 10pt, y: 8pt),
  radius: 3pt,
  stroke: (left: 2pt + luma(60%)),
)[
  *Copy-paste version.* Select the formula under each label and place it in the named cell.

  *A2 — round:*
  #text(font: "DejaVu Sans Mono", size: 7pt)[#excel-round-copy]

  *B2 — dealer:*
  #text(font: "DejaVu Sans Mono", size: 7pt)[#excel-dealer-copy]

  *C2 — cards:*
  #text(font: "DejaVu Sans Mono", size: 7pt)[#excel-cards-copy]
]

== Random player selection <sec:random-player-selection>

Whenever a rule calls for a player to be selected randomly, the table chooses one of these physical methods. Announce the chosen method before dealing.

=== First Jack (order-biased)

1. Shuffle the deck.
2. Deal cards face-up one at a time, moving around the table in seat order.
3. Stop when the first Jack appears. The player who receives that Jack is selected.
4. Return all selection cards to the deck and shuffle before ordinary play resumes.

This method is random, but it is not an equal-probability selection across seats. Because dealing stops at the first Jack, players earlier in the deal order have more opportunities to receive that Jack before later players do. This is the traditional method used by “first Jack deals.”

=== High card (order-neutral)

1. Shuffle the deck.
2. Deal one card face-up to each player, moving around the table in seat order.
3. The player with the highest-ranked card is selected. If two or more players tie for highest rank, collect the dealt cards, shuffle again, and repeat the draw among only the tied players.
4. Return all selection cards to the deck and shuffle before ordinary play resumes.

Use the normal rank order from 2 through Ace, with Ace high. Because every player receives one card before the result is determined, this method does not favor earlier seats. A random selection chooses only the player named by the rule; it does not alter the clockwise dealer rotation.
