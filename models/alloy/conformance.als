// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

module poche_conformance

open poche

// Stable role labels make the canonical receipt projection independent of
// Alloy's generated atom names. The imported poche model remains handwritten
// and authoritative; this module only selects bounded comparison fixtures.
one sig CanonicalRoles {
  dealerRole: one Player,
  firstRole: one Player,
  lowLead: one Card,
  highFollow: one Card,
  trumpRole: one Card,
  roundRole: one Round,
  trickRole: one Trick
}

pred CanonicalOneCardRustFixture {
  CanonicalRoles.firstRole = CanonicalRoles.dealerRole.left
  CanonicalRoles.lowLead.suit = Clubs
  CanonicalRoles.lowLead.rank = Two
  CanonicalRoles.highFollow.suit = Clubs
  CanonicalRoles.highFollow.rank = Three
  CanonicalRoles.trumpRole.suit = Clubs
  CanonicalRoles.trumpRole.rank = Four

  let r = CanonicalRoles.roundRole,
      t = CanonicalRoles.trickRole,
      dealerPlayer = CanonicalRoles.dealerRole,
      firstPlayer = CanonicalRoles.firstRole |
    r.dealer = dealerPlayer and
    r.handSize = 1 and
    firstPlayer.(r.hand) = CanonicalRoles.lowLead and
    dealerPlayer.(r.hand) = CanonicalRoles.highFollow and
    r.trumpCard = CanonicalRoles.trumpRole and
    firstPlayer.(r.bid) = 0 and
    dealerPlayer.(r.bid) = 1 and
    t.round = r and
    t.index = 1 and
    t.leader = firstPlayer and
    firstPlayer.(t.play) = CanonicalRoles.lowLead and
    dealerPlayer.(t.play) = CanonicalRoles.highFollow and
    t.winner = dealerPlayer and
    no firstPlayer.(r.captured) and
    dealerPlayer.(r.captured) = CanonicalRoles.lowLead + CanonicalRoles.highFollow and
    firstPlayer.(r.score) = 10 and
    dealerPlayer.(r.score) = 21 and
    no r.missedBid and
    r.restored = Card
}

// Individually named invalid fixtures must be UNSAT under the imported facts.
pred InvalidDuplicateHandCard {
  some r: Round, disj p, q: Player | some p.(r.hand) & q.(r.hand)
}

pred InvalidTrumpInHand {
  some r: Round | r.trumpCard in Player.(r.hand)
}

pred InvalidFollowSuitPlay {
  some r: Round, t: tricksOf[r], p: Player - t.leader |
    (some c: availableCards[r, t, p] | c.suit = leadSuit[t]) and
    cardPlayed[t, p].suit != leadSuit[t]
}

pred InvalidWinner {
  some t: Trick | cardPlayed[t, t.winner] not in eligiblePlays[t]
}

pred InvalidRoundScore {
  some r: Round, p: Player |
    tricksWon[r, p] = p.(r.bid) and
    tricksWon[r, p] < r.handSize and
    p.(r.score) != integer/add[10, p.(r.bid)]
}

assert CoreRejectsKnownInvalidStructures {
  not InvalidDuplicateHandCard
  not InvalidTrumpInHand
  not InvalidFollowSuitPlay
  not InvalidWinner
  not InvalidRoundScore
}

// Controlled weakened rules must have witnesses. These predicates do not
// mutate the imported oracle; they describe exactly what each bad replacement
// would newly admit.
pred UnrestrictedFollowSuitDefect {
  some r: Round, t: tricksOf[r], p: Player - t.leader, c: Card |
    c in availableCards[r, t, p] and
    c.suit != leadSuit[t] and
    some heldLead: availableCards[r, t, p] | heldLead.suit = leadSuit[t]
}

pred RankOnlyWinnerDefect {
  some t: Trick, p: Player - t.winner |
    cardPlayed[t, t.winner].rank in cardPlayed[t, p].rank.below
}

pred PartialAllTricksBonusDefect {
  some r: Round, p: Player |
    tricksWon[r, p] = p.(r.bid) and
    tricksWon[r, p] < r.handSize and
    p.(r.score) != integer/add[20, p.(r.bid)]
}

run CanonicalOneCardRustFixture for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 1 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run InvalidDuplicateHandCard for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 1 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

run InvalidTrumpInHand for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 1 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

run InvalidFollowSuitPlay for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 2 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

run InvalidWinner for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 1 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

run InvalidRoundScore for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 2 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

check CoreRejectsKnownInvalidStructures for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 2 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 0

run UnrestrictedFollowSuitDefect for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 2 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run RankOnlyWinnerDefect for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 1 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1

run PartialAllTricksBonusDefect for 6 but 7 Int,
  exactly 2 Player, exactly 52 Card, exactly 1 Round, exactly 2 Trick,
  exactly 6 Step, exactly 0 GameResult, exactly 0 FirstJackSelection,
  exactly 0 HighCardProcess, exactly 0 HighCardDraw expect 1
