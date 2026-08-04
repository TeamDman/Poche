// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

module poche

open util/ordering[Step] as stepOrder

// This is an independent Alloy oracle derived from docs/main.typ.  It is not
// generated from the Rust oracle.  Every command below records its finite
// scope; an UNSAT check therefore establishes only a bounded result.

// R-GAME-001, R-GAME-004: supported tables and one fixed clockwise ring.
sig Player { left: one Player }

fact SupportedClockwiseTable {
  #Player >= 2
  #Player <= 51
  left in Player one -> one Player
  no iden & left
  all p: Player | Player in p.*left
}

// R-GAME-002, R-TRICK-010, R-TRICK-011: the standard deck.  Rank order is
// explicit; suits deliberately have no ordering relation.
abstract sig Suit {}
one sig Clubs, Diamonds, Hearts, Spades extends Suit {}

abstract sig Rank { below: set Rank }
one sig Two, Three, Four, Five, Six, Seven, Eight, Nine, Ten,
        Jack, Queen, King, Ace extends Rank {}

fact RankOrder {
  no Two.below
  Three.below = Two
  Four.below = Two + Three
  Five.below = Two + Three + Four
  Six.below = Two + Three + Four + Five
  Seven.below = Two + Three + Four + Five + Six
  Eight.below = Two + Three + Four + Five + Six + Seven
  Nine.below = Two + Three + Four + Five + Six + Seven + Eight
  Ten.below = Two + Three + Four + Five + Six + Seven + Eight + Nine
  Jack.below = Two + Three + Four + Five + Six + Seven + Eight + Nine + Ten
  Queen.below = Two + Three + Four + Five + Six + Seven + Eight + Nine + Ten + Jack
  King.below = Two + Three + Four + Five + Six + Seven + Eight + Nine + Ten + Jack + Queen
  Ace.below = Two + Three + Four + Five + Six + Seven + Eight + Nine + Ten + Jack + Queen + King
}

sig Card {
  suit: one Suit,
  rank: one Rank
}

fact CompleteStandardDeck {
  all s: Suit, r: Rank |
    one { c: Card | c.suit = s and c.rank = r }
}

pred higher[a, b: Card] { b.rank in a.rank.below }

// R-HAND-001..004: the rulebook formula is represented independently of any
// concrete Player scope.  Seven-bit integers cover every n in 2..51.
fun maximumHand[n: Int]: one Int {
  n <= 7 => 7 else integer/div[51, n]
}

fun scheduledRounds[n: Int]: one Int {
  integer/sub[integer/mul[2, maximumHand[n]], 1]
}

fun scheduledHand[n, roundNumber: Int]: one Int {
  roundNumber <= maximumHand[n] => roundNumber
  else integer/sub[integer/mul[2, maximumHand[n]], roundNumber]
}

one sig GamePlan {
  firstDealer: one Player,
  roundDealer: Int -> lone Player,
  roundHand: Int -> lone Int
}

fact RisingFallingPlan {
  let count = scheduledRounds[#Player] |
    all i: Int |
      ((i >= 1 and i <= count) iff
        (one GamePlan.roundDealer[i] and one GamePlan.roundHand[i]))

  all i: Int |
    one GamePlan.roundHand[i] implies
      GamePlan.roundHand[i] = scheduledHand[#Player, i]

  GamePlan.roundDealer[1] = GamePlan.firstDealer
  all i: Int |
    i >= 1 and i < scheduledRounds[#Player] implies
      GamePlan.roundDealer[integer/add[i, 1]] = GamePlan.roundDealer[i].left
}

// R-GAME-003 and R-SETUP-001: bounded lifecycle traces.  Advance may begin
// another deal or finish after the last scheduled round.
abstract sig Phase {}
one sig Prepare, Deal, Bid, Play, Score, Advance, Finished extends Phase {}

sig Step { phase: one Phase }

pred validPhaseSuccessor[from, to: Phase] {
  (from = Prepare and to = Deal) or
  (from = Deal and to = Bid) or
  (from = Bid and to = Play) or
  (from = Play and to = Score) or
  (from = Score and to = Advance) or
  (from = Advance and to in Deal + Finished) or
  (from = Finished and to = Finished)
}

fact PhaseTraceShape {
  lone { s: Step | s.phase = Prepare }
  stepOrder/first.phase = Prepare
  all s: Step - stepOrder/last |
    validPhaseSuccessor[s.phase, stepOrder/next[s].phase]
}

// A Round is a complete semantic snapshot.  Dealing is atomic because the
// intermediate one-card-at-a-time states reveal no order-sensitive choice.
// The turnIndex relation nevertheless records clockwise within-trick order.
sig Round {
  dealer: one Player,
  handSize: one Int,
  hand: Player -> set Card,
  trumpCard: one Card,
  bid: Player -> one Int,
  score: Player -> one Int,
  missedBid: set Player,
  antePaid: set Player,
  missPaid: set Player,
  captured: Player -> set Card,
  restored: set Card
}

sig Trick {
  round: one Round,
  index: one Int,
  leader: one Player,
  turnIndex: Player -> one Int,
  play: Player -> one Card,
  winner: one Player
}

fun tricksOf[r: Round]: set Trick { r.~round }
fun trickAt[r: Round, i: Int]: lone Trick { { t: tricksOf[r] | t.index = i } }
fun cardPlayed[t: Trick, p: Player]: one Card { p.(t.play) }
fun leadSuit[t: Trick]: one Suit { cardPlayed[t, t.leader].suit }

fun earlierCards[r: Round, t: Trick, p: Player]: set Card {
  { c: Card |
    some prior: tricksOf[r] |
      prior.index < t.index and c = cardPlayed[prior, p]
  }
}

fun availableCards[r: Round, t: Trick, p: Player]: set Card {
  p.(r.hand) - earlierCards[r, t, p]
}

fun trumpPlays[t: Trick]: set Card {
  { c: Player.(t.play) | c.suit = t.round.trumpCard.suit }
}

fun leadPlays[t: Trick]: set Card {
  { c: Player.(t.play) | c.suit = leadSuit[t] }
}

fun eligiblePlays[t: Trick]: set Card {
  some trumpPlays[t] => trumpPlays[t] else leadPlays[t]
}

fun cardsWon[r: Round, p: Player]: set Card {
  { c: Card |
    some t: tricksOf[r] |
      t.winner = p and c in Player.(t.play)
  }
}

fun tricksWon[r: Round, p: Player]: one Int {
  #{ t: tricksOf[r] | t.winner = p }
}

fun undealt[r: Round]: set Card {
  Card - r.trumpCard - Player.(r.hand)
}

// R-DEAL-001..004 and R-TRICK-001: the complete deal is a partition, the
// revealed trump remains outside hands, and every hand card is played once.
fact RoundCardPartition {
  all r: Round |
    r.handSize >= 1 and r.handSize <= maximumHand[#Player]

  all r: Round, p: Player | #(p.(r.hand)) = r.handSize
  all r: Round, disj p, q: Player | no p.(r.hand) & q.(r.hand)
  all r: Round | r.trumpCard not in Player.(r.hand)
  all r: Round | integer/add[integer/mul[#Player, r.handSize], 1] <= 52

  all r: Round | #tricksOf[r] = r.handSize
  all r: Round, t: tricksOf[r] | t.index >= 1 and t.index <= r.handSize
  all r: Round, disj t, u: tricksOf[r] | t.index != u.index
  all r: Round, p: Player | p.(tricksOf[r].play) = p.(r.hand)
}

// R-BID-001..005: bids are fixed whole numbers in range.  Bid order follows
// the same turnIndex ring and the dealer is therefore last.  No total-bid
// restriction appears in this fact.
fact BidDomain {
  all r: Round, p: Player |
    p.(r.bid) >= 0 and p.(r.bid) <= r.handSize
}

// R-TRICK-002..006: leaders, clockwise turns, and follow-suit legality.
fact LegalTrickPlay {
  all r: Round, t: tricksOf[r] |
    (t.index = 1 implies t.leader = r.dealer.left)

  all r: Round, t: tricksOf[r] |
    t.index > 1 implies
      t.leader = trickAt[r, integer/sub[t.index, 1]].winner

  all t: Trick | t.leader.(t.turnIndex) = 0
  all t: Trick, p: Player |
    p.(t.turnIndex) >= 0 and p.(t.turnIndex) < #Player
  all t: Trick, disj p, q: Player |
    p.(t.turnIndex) != q.(t.turnIndex)
  all t: Trick, p: Player |
    p.left.(t.turnIndex) =
      (p.(t.turnIndex) = integer/sub[#Player, 1]
        => 0 else integer/add[p.(t.turnIndex), 1])

  all r: Round, t: tricksOf[r], p: Player - t.leader |
    (some c: availableCards[r, t, p] | c.suit = leadSuit[t]) implies
      cardPlayed[t, p].suit = leadSuit[t]
}

// R-TRICK-007..012: trump first, otherwise lead suit, then explicit rank.
// captured retains the provenance of every face-down trick pile.
fact TrickWinnerAndCapture {
  all t: Trick |
    cardPlayed[t, t.winner] in eligiblePlays[t]

  all t: Trick |
    all c: eligiblePlays[t] - cardPlayed[t, t.winner] |
      c.rank in cardPlayed[t, t.winner].rank.below

  all r: Round, p: Player | p.(r.captured) = cardsWon[r, p]
  all r: Round, disj p, q: Player | no p.(r.captured) & q.(r.captured)
}

// R-SCORE-001..005 and R-MONEY-001..002: scoring and physical money are
// distinct fields.  A missed bid pays one ten-cent unit; every player paid one
// opening 25-cent ante unit.  Currency units are not added to score.
fact RoundScoringAndMoney {
  all r: Round | r.antePaid = Player
  all r: Round | r.missedBid = { p: Player | tricksWon[r, p] != p.(r.bid) }
  all r: Round | r.missPaid = r.missedBid

  all r: Round, p: Player |
    (tricksWon[r, p] != p.(r.bid) implies p.(r.score) = 0)

  all r: Round, p: Player |
    (tricksWon[r, p] = p.(r.bid) and tricksWon[r, p] < r.handSize implies
      p.(r.score) = integer/add[10, p.(r.bid)])

  all r: Round, p: Player |
    (tricksWon[r, p] = p.(r.bid) and tricksWon[r, p] = r.handSize implies
      p.(r.score) = integer/add[20, p.(r.bid)])
}

// R-ADVANCE-001: undealt, trump, and captured zones reconstruct the full deck.
fact RestoreFullDeck {
  all r: Round | r.restored = undealt[r] + r.trumpCard + Player.(r.captured)
  all r: Round | r.restored = Card
}

// R-GAME-005, R-MONEY-003, R-FINISH-002..004: final totals are score-only;
// every maximum scorer is a shared winner and receives the communal bowl.
sig GameResult {
  rounds: set Round,
  total: Player -> one Int,
  winners: set Player,
  bowlRecipients: set Player
}

fact FinalWinnerSemantics {
  all g: GameResult | g.rounds = Round
  all g: GameResult, p: Player |
    p.(g.total) = (sum r: g.rounds | p.(r.score))
  all g: GameResult, p: Player | p.(g.total) >= 0
  all g: GameResult |
    g.winners = { p: Player | no q: Player | q.(g.total) > p.(g.total) }
  all g: GameResult | g.bowlRecipients = g.winners
}

// R-SETUP-004 and R-RANDOM-002..003,006: First Jack is a finite without-
// replacement sequence dealt in seat order.  The support is captured, while
// probability is intentionally not invented: earlier seats have earlier draw
// positions and the method is not asserted uniform.
sig FirstJackSelection {
  firstRecipient: one Player,
  draw: Int -> lone Card,
  recipient: Int -> lone Player,
  selected: one Player,
  restoredCards: set Card
}

fact FirstJackProcedure {
  all s: FirstJackSelection |
    s.draw.Card = s.recipient.Player

  all s: FirstJackSelection |
    some s.draw.Card and integer/min[s.draw.Card] = 0

  all s: FirstJackSelection, i: s.draw.Card |
    i >= 0 and (i > 0 implies integer/sub[i, 1] in s.draw.Card)

  all s: FirstJackSelection, disj i, j: s.draw.Card |
    s.draw[i] != s.draw[j]

  all s: FirstJackSelection | s.recipient[0] = s.firstRecipient
  all s: FirstJackSelection, i: s.draw.Card - 0 |
    s.recipient[i] = s.recipient[integer/sub[i, 1]].left

  all s: FirstJackSelection |
    let firstJack = integer/min[{ i: s.draw.Card | s.draw[i].rank = Jack }] |
      one firstJack and firstJack = integer/max[s.draw.Card] and
      s.selected = s.recipient[firstJack] and
      all i: s.draw.Card | i < firstJack implies s.draw[i].rank != Jack

  all s: FirstJackSelection | s.restoredCards = Card
}

// R-RANDOM-004..006: High Card deals to every current contender before rank
// comparison, repeats only among tied highest ranks, and ends at one winner.
sig HighCardDraw {
  contenders: some Player,
  dealt: Player -> lone Card
}

sig HighCardProcess {
  draws: some HighCardDraw,
  first: one HighCardDraw,
  final: one HighCardDraw,
  next: HighCardDraw -> lone HighCardDraw,
  selected: one Player,
  restoredCards: set Card
}

fun highCardLeaders[d: HighCardDraw]: set Player {
  { p: d.contenders |
    no q: d.contenders | higher[q.(d.dealt), p.(d.dealt)]
  }
}

fact HighCardProcedure {
  all h: HighCardProcess |
    h.first in h.draws and h.final in h.draws and
    h.draws = h.first.*(h.next) and no h.final.(h.next)

  all h: HighCardProcess | h.next in h.draws -> lone h.draws
  all h: HighCardProcess, d: h.draws - h.first | one d.~(h.next)
  all h: HighCardProcess | no h.first.~(h.next)

  all d: HighCardDraw | d.dealt.Card = d.contenders
  all d: HighCardDraw, disj p, q: d.contenders |
    p.(d.dealt) != q.(d.dealt)

  all h: HighCardProcess, d: h.draws - h.final |
    #highCardLeaders[d] > 1 and d.(h.next).contenders = highCardLeaders[d]

  all h: HighCardProcess |
    one highCardLeaders[h.final] and h.selected = highCardLeaders[h.final]

  all h: HighCardProcess, disj d, e: h.draws |
    no Player.(d.dealt) & Player.(e.dealt)

  all h: HighCardProcess | h.restoredCards = Card
}

// ---- Satisfiability witnesses ------------------------------------------------

pred CompleteRoundWitness {
  one Round
  one GameResult
  Round.handSize = 2
  stepOrder/first.phase = Prepare
  stepOrder/next[stepOrder/first].phase = Deal
  stepOrder/next[stepOrder/next[stepOrder/first]].phase = Bid
  stepOrder/next[stepOrder/next[stepOrder/next[stepOrder/first]]].phase = Play
  stepOrder/next[stepOrder/next[stepOrder/next[stepOrder/next[stepOrder/first]]]].phase = Score
  stepOrder/last.phase = Advance
}

pred UnrestrictedBidWitness {
  one r: Round |
    r.handSize = 1 and all p: Player | p.(r.bid) = 1
}

pred ZeroBidSuccessWitness {
  some r: Round, p: Player |
    p.(r.bid) = 0 and tricksWon[r, p] = 0 and p.(r.score) = 10
}

pred SharedWinnerWitness {
  one g: GameResult | #g.winners > 1
}

pred FirstJackWitness {
  one FirstJackSelection
  FirstJackSelection.selected = GamePlan.firstDealer
}

pred RepeatedHighCardWitness {
  one HighCardProcess
  #HighCardProcess.draws = 2
  HighCardProcess.selected = GamePlan.firstDealer
}

pred ParameterBoundaryWitness {
  maximumHand[2] = 7
  scheduledRounds[2] = 13
  maximumHand[51] = 1
  scheduledRounds[51] = 1
  scheduledHand[2, 13] = 1
}

// ---- Bounded assertions ------------------------------------------------------

assert CompleteDeckIsExactly52 {
  #Card = 52
}

assert CardConservationAndPartition {
  all r: Round |
    Card = undealt[r] + r.trumpCard + Player.(r.captured) and
    no undealt[r] & r.trumpCard and
    no undealt[r] & Player.(r.captured) and
    no r.trumpCard & Player.(r.captured)
}

assert FollowSuitIsEnforced {
  all r: Round, t: tricksOf[r], p: Player - t.leader |
    (some c: availableCards[r, t, p] | c.suit = leadSuit[t]) implies
      cardPlayed[t, p].suit = leadSuit[t]
}

assert DealerBidsLastInClockwiseOrder {
  all r: Round |
    r.dealer.(trickAt[r, 1].turnIndex) = integer/sub[#Player, 1]
}

assert WinnerIsEligibleAndHighest {
  all t: Trick |
    cardPlayed[t, t.winner] in eligiblePlays[t] and
    all c: eligiblePlays[t] - cardPlayed[t, t.winner] |
      c.rank in cardPlayed[t, t.winner].rank.below
}

assert ScoreAndPaymentAgree {
  all r: Round, p: Player |
    (p in r.missedBid iff tricksWon[r, p] != p.(r.bid)) and
    (p in r.missPaid iff p in r.missedBid) and
    (p not in r.missedBid implies p.(r.score) > 0) and
    (p in r.missedBid implies p.(r.score) = 0)
}

assert ScheduleBoundariesAndFeasibility {
  all n: Int |
    n >= 2 and n <= 51 implies
      maximumHand[n] >= 1 and maximumHand[n] <= 7 and
      integer/add[integer/mul[n, maximumHand[n]], 1] <= 52 and
      scheduledRounds[n] = integer/sub[integer/mul[2, maximumHand[n]], 1] and
      scheduledHand[n, 1] = 1 and
      scheduledHand[n, scheduledRounds[n]] = 1
}

assert FinalWinnersAreExactlyTheMaxima {
  all g: GameResult |
    some g.winners and
    all p: g.winners, q: Player | p.(g.total) >= q.(g.total) and
    all p: Player - g.winners | some q: g.winners | q.(g.total) > p.(g.total)
}

// Commands use the real deck and deliberately small player/trace scopes.
run CompleteRoundWitness for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 2 Trick, exactly 6 Step,
  exactly 1 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run UnrestrictedBidWitness for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 1 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run ZeroBidSuccessWitness for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 1 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run SharedWinnerWitness for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 0 Round, exactly 0 Trick, exactly 6 Step,
  exactly 1 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run FirstJackWitness for 6 but 7 Int, exactly 3 Player, exactly 52 Card,
  exactly 0 Round, exactly 0 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 1 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run RepeatedHighCardWitness for 6 but 7 Int, exactly 3 Player, exactly 52 Card,
  exactly 0 Round, exactly 0 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 1 HighCardProcess, exactly 2 HighCardDraw expect 1

run ParameterBoundaryWitness for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 0 Round, exactly 0 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

check CompleteDeckIsExactly52 for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 0 Round, exactly 0 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check CardConservationAndPartition for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 2 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check FollowSuitIsEnforced for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 2 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check DealerBidsLastInClockwiseOrder for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 2 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check WinnerIsEligibleAndHighest for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 2 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check ScoreAndPaymentAgree for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 1 Round, exactly 2 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check ScheduleBoundariesAndFeasibility for 6 but 7 Int, exactly 2 Player, exactly 52 Card,
  exactly 0 Round, exactly 0 Trick, exactly 6 Step,
  exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check FinalWinnersAreExactlyTheMaxima for 6 but 7 Int, exactly 3 Player, exactly 52 Card,
  exactly 0 Round, exactly 0 Trick, exactly 6 Step,
  exactly 1 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0
