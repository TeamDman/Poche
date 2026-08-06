// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

module spatial

// Independent bounded oracle for poche-spatial-v1. Coordinates are finite
// integer-labelled cells, not continuous geometry or renderer meshes. Every
// command below retains its exact atom scope and 5-bit integer width.

abstract sig Viewer {}
abstract sig Player extends Viewer {}
one sig Player0, Player1 extends Player {}
one sig Spectator extends Viewer {}

abstract sig Seat {}
one sig Seat0, Seat1 extends Seat {}

abstract sig Zone {}
one sig DeckZone, TrumpZone, PlayZone extends Zone {}
abstract sig HandZone extends Zone {}
one sig Hand0Zone, Hand1Zone extends HandZone {}
abstract sig WonZone extends Zone {}
one sig Won0Zone, Won1Zone extends WonZone {}

sig Cell { x, y, z: one Int }
sig Slot { index: one Int }
sig Face {}

abstract sig Object {}
sig Card extends Object { truth: one Face }
one sig Table, ScoreSheet extends Object {}

sig Layout {
  seated: Seat -> lone Player,
  handOwner: HandZone -> lone Player,
  wonOwner: WonZone -> lone Player,
  inner: Zone -> set Cell,
  outer: Zone -> set Cell
}

sig TypedProjection {
  layout: one Layout,
  cardZone: Card -> one Zone,
  cardSlot: Card -> one Slot,
  known: Viewer -> Card,
  score: Player -> one Int
}

abstract sig TextRun { attached: one Object }
sig FaceRun extends TextRun {
  viewer: one Viewer,
  card: one Card,
  meaning: one Face
}
sig ScoreRun extends TextRun {
  player: one Player,
  value: one Int
}

sig SpatialScene {
  source: one TypedProjection,
  layout: one Layout,
  location: Card -> one Zone,
  slot: Card -> one Slot,
  shown: Viewer -> Card,
  scoreCell: Player -> one Int,
  text: set TextRun
}

// A drop is a finite conservative bound: every covered coordinate cell may
// touch a zone. A faulty classifier records one arbitrary selected zone.
sig Drop {
  covers: some Cell,
  selected: lone Zone
}

fact FiniteIdentityDomains {
  all disj a, b: Cell | a.x != b.x or a.y != b.y or a.z != b.z
  all s: Slot | s.index >= 0
  all disj a, b: Slot | a.index != b.index
  all disj a, b: Card | a.truth != b.truth
}

pred layoutShape[l: Layout] {
  all s: Seat | one s.(l.seated)
  all p: Player | one p.~(l.seated)
  all h: HandZone | one h.(l.handOwner)
  all p: Player | one p.~(l.handOwner)
  all w: WonZone | one w.(l.wonOwner)
  all p: Player | one p.~(l.wonOwner)
  all z: Zone | some z.(l.inner)
  all z: Zone | z.(l.inner) in z.(l.outer)
}

pred zonesSeparated[l: Layout] {
  all disj a, b: Zone | no a.(l.outer) & b.(l.outer)
}

pred validLayout[l: Layout] {
  layoutShape[l]
  zonesSeparated[l]
}

fun ownerOf[l: Layout, z: Zone]: lone Player {
  z.(l.handOwner + l.wonOwner)
}

pred authorized[t: TypedProjection, v: Viewer, c: Card] {
  c.(t.cardZone) in TrumpZone + PlayZone + WonZone or
  (v in Player and c.(t.cardZone) in HandZone and
    v = ownerOf[t.layout, c.(t.cardZone)])
}

pred locationAndSlotShape[t: TypedProjection] {
  all c: Card | one c.(t.cardZone) and one c.(t.cardSlot)
  all disj a, b: Card | a.(t.cardSlot) != b.(t.cardSlot)
  one { c: Card | c.(t.cardZone) = TrumpZone }
  no { c: Card | c.(t.cardZone) = DeckZone } & Viewer.(t.known)
}

pred validTyped[t: TypedProjection] {
  validLayout[t.layout]
  locationAndSlotShape[t]
  all v: Viewer, c: Card |
    (v->c in t.known iff authorized[t, v, c])
  all p: Player | p.(t.score) >= 0
}

pred validScene[s: SpatialScene] {
  validLayout[s.layout]
  all c: Card | one c.(s.location) and one c.(s.slot)
  all disj a, b: Card | a.(s.slot) != b.(s.slot)

  all v: Viewer, c: Card |
    (v->c in s.shown iff
      one r: FaceRun & s.text | r.viewer = v and r.card = c)

  all r: FaceRun & s.text |
    r.attached = r.card and r.meaning = r.card.truth and
    r.viewer->r.card in s.shown

  all v: Viewer, c: Card | lone {
    r: FaceRun & s.text | r.viewer = v and r.card = c
  }

  all p: Player | one {
    r: ScoreRun & s.text |
      r.player = p and r.attached = ScoreSheet and
      r.value = p.(s.scoreCell)
  }

  s.text = TextRun
}

pred realizes[t: TypedProjection, s: SpatialScene] {
  validTyped[t]
  s.source = t
  s.layout = t.layout
  s.location = t.cardZone
  s.slot = t.cardSlot
  s.shown = t.known
  s.scoreCell = t.score
  validScene[s]
}

pred abstracts[s: SpatialScene, t: TypedProjection] {
  validScene[s]
  validTyped[t]
  s.layout = t.layout
  s.location = t.cardZone
  s.slot = t.cardSlot
  s.shown = t.known
  s.scoreCell = t.score
}

pred canonicalCardCounts[t: TypedProjection] {
  #{ c: Card | c.(t.cardZone) = DeckZone } = 2
  #{ c: Card | c.(t.cardZone) = TrumpZone } = 1
  #{ c: Card | c.(t.cardZone) = PlayZone } = 1
  #{ c: Card | c.(t.cardZone) = Hand0Zone } = 2
  #{ c: Card | c.(t.cardZone) = Hand1Zone } = 2
  no { c: Card | c.(t.cardZone) in WonZone }
}

pred CanonicalSpatialWitness {
  one t: TypedProjection, s: SpatialScene |
    realizes[t, s] and canonicalCardCounts[t]
}

assert SeatAndOwnedZoneInjection {
  all l: Layout | validLayout[l] implies
    (all disj s, u: Seat | s.(l.seated) != u.(l.seated)) and
    (all disj h, k: HandZone | h.(l.handOwner) != k.(l.handOwner)) and
    (all disj w, q: WonZone | w.(l.wonOwner) != q.(l.wonOwner))
}

assert ZoneSeparation {
  all l: Layout | validLayout[l] implies
    all disj a, b: Zone | no a.(l.outer) & b.(l.outer)
}

assert CardLocationAndSlotUniqueness {
  all t: TypedProjection | validTyped[t] implies
    all disj a, b: Card | a.(t.cardSlot) != b.(t.cardSlot)
}

assert FaceAttachmentTotality {
  all s: SpatialScene | validScene[s] implies
    all v: Viewer, c: Card |
      (v->c in s.shown iff one r: FaceRun & s.text |
        r.viewer = v and r.card = c and
        r.attached = c and r.meaning = c.truth)
}

assert VisibilityRelations {
  all t: TypedProjection | validTyped[t] implies
    (all v: Viewer, c: Card |
      c.(t.cardZone) = DeckZone implies v->c not in t.known) and
    (all v: Viewer, c: Card |
      c.(t.cardZone) in TrumpZone + PlayZone + WonZone implies
        v->c in t.known) and
    (all disj owner, viewer: Player, c: Card |
      c.(t.cardZone) in HandZone and
      owner = ownerOf[t.layout, c.(t.cardZone)] implies
        viewer->c not in t.known)
}

assert ScoreAttachmentTotality {
  all s: SpatialScene | validScene[s] implies
    all p: Player | one r: ScoreRun & s.text |
      r.player = p and r.attached = ScoreSheet and
      r.value = p.(s.scoreCell)
}

assert RealizationAbstractionRoundTrip {
  all t: TypedProjection, s: SpatialScene |
    realizes[t, s] implies abstracts[s, t]
}

// Negative-control assertions intentionally have bounded counterexamples.
assert OverlapNegativeControl {
  all l: Layout | layoutShape[l] implies zonesSeparated[l]
}

fun touched[l: Layout, d: Drop]: set Zone {
  { z: Zone | some d.covers & z.(l.outer) }
}

assert AmbiguousFirstMatchNegativeControl {
  all l: Layout, d: Drop |
    validLayout[l] and #touched[l, d] > 1 and d.selected in touched[l, d]
      implies no d.selected
}

pred sceneShapeExceptFaceAttachment[s: SpatialScene] {
  validLayout[s.layout]
  all c: Card | one c.(s.location) and one c.(s.slot)
  all disj a, b: Card | a.(s.slot) != b.(s.slot)
  all v: Viewer, c: Card |
    (v->c in s.shown iff
      one r: FaceRun & s.text | r.viewer = v and r.card = c)
  all r: FaceRun & s.text |
    r.meaning = r.card.truth and r.viewer->r.card in s.shown
  all v: Viewer, c: Card | lone {
    r: FaceRun & s.text | r.viewer = v and r.card = c
  }
  all p: Player | one {
    r: ScoreRun & s.text |
      r.player = p and r.attached = ScoreSheet and
      r.value = p.(s.scoreCell)
  }
  s.text = TextRun
}

assert DetachedFaceTextNegativeControl {
  all s: SpatialScene | sceneShapeExceptFaceAttachment[s] implies
    all r: FaceRun & s.text | r.attached = r.card
}

run CanonicalSpatialWitness for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 1

check SeatAndOwnedZoneInjection for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 0

check ZoneSeparation for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 0

check CardLocationAndSlotUniqueness for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 0

check FaceAttachmentTotality for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 0

check VisibilityRelations for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 0

check ScoreAttachmentTotality for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 0

check RealizationAbstractionRoundTrip for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 0

check OverlapNegativeControl for 12 but 5 Int,
  exactly 1 Layout, exactly 0 TypedProjection, exactly 0 SpatialScene,
  exactly 0 Card, exactly 0 Face, exactly 0 Slot, exactly 7 Cell,
  exactly 0 FaceRun, exactly 0 ScoreRun, exactly 0 Drop expect 1

check AmbiguousFirstMatchNegativeControl for 12 but 5 Int,
  exactly 1 Layout, exactly 0 TypedProjection, exactly 0 SpatialScene,
  exactly 0 Card, exactly 0 Face, exactly 0 Slot, exactly 7 Cell,
  exactly 0 FaceRun, exactly 0 ScoreRun, exactly 1 Drop expect 1

check DetachedFaceTextNegativeControl for 12 but 5 Int,
  exactly 1 Layout, exactly 1 TypedProjection, exactly 1 SpatialScene,
  exactly 8 Card, exactly 8 Face, exactly 8 Slot, exactly 7 Cell,
  exactly 10 FaceRun, exactly 2 ScoreRun, exactly 0 Drop expect 1
