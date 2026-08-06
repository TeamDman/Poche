// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{error::Error, fmt};

use poche_oracle_rust::{
    Action, Card, DeckOrder, Game, GameState, Observation, PhaseTag, Seat, Turn,
};
use poche_spatial::{
    AabbMm, AbstractionError, CardFace, CardLocation, LayoutError, LayoutId,
    PlayerSpatialProjection, Point3Mm, RealizationError, RevealedWonCard, SceneError, SeatId,
    SpatialLayout, TableId, TextBinding, ViewerSpatialProjection, ZoneClassification, ZoneId,
    ZoneVolume, abstract_viewer_scene, realize_viewer_scene, reconstruct_animation_endpoint,
    registered_layout, resolve_card_play,
};

const PLAYERS: usize = 2;
const FIRST_SEED: u8 = 0;
const LAST_SEED: u8 = 15;
const MAX_TRACE_STEPS: usize = 4_000;

/// Stable bounded scope exercised by the Rust spatial-refinement checker.
pub const SPATIAL_CHECK_SCOPE: &str = "spatial-rust-v1-layouts-2-8-complete-2p-traces-seeds-0-15";

/// One deliberately injected fault and the deterministic minimal witness that
/// proves the checker rejects it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NegativeSpatialControl {
    /// Stable fault identifier.
    pub fault: &'static str,
    /// First deterministic witness retained by the check.
    pub witness: String,
}

/// Exact measurements for one completed spatial-refinement run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialCheckReport {
    /// Fully declared, non-exhaustive scope identifier.
    pub scope: &'static str,
    /// Inclusive deterministic seed range.
    pub seeds: (u8, u8),
    /// Registered layout cardinalities checked.
    pub registered_layouts: usize,
    /// Reachable oracle states realized for each exact viewer.
    pub viewer_projections: usize,
    /// Exact abstraction/realization equalities checked.
    pub round_trips: usize,
    /// Pairs of exact-viewer projections whose public portions agreed.
    pub privacy_noninterference_pairs: usize,
    /// Typed play transitions whose spatial endpoint remained in the trick.
    pub play_endpoint_commutations: usize,
    /// Typed plays whose second card atomically completed/captured a trick.
    pub atomic_trick_resolutions: usize,
    /// Complete deterministic traces that reached `Finished`.
    pub finished_traces: usize,
    /// Total semantic transitions traversed across all seeds.
    pub transitions: usize,
    /// Deliberate faults caught with stable minimal witnesses.
    pub negative_controls: Vec<NegativeSpatialControl>,
}

/// Stable failure while constructing a spatial-refinement receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpatialCheckError {
    /// A registered layout could not be constructed.
    Layout(LayoutError),
    /// A canonical viewer projection did not realize.
    Realization(RealizationError),
    /// A canonical realized scene did not abstract.
    Abstraction(AbstractionError),
    /// The oracle rejected a deterministic advertised action.
    Oracle(String),
    /// A named refinement equality failed.
    Law(&'static str),
    /// The deterministic trace exceeded its declared safety bound.
    TraceBound { seed: u8 },
}

impl fmt::Display for SpatialCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(error) => write!(formatter, "layout construction failed: {error:?}"),
            Self::Realization(error) => write!(formatter, "spatial realization failed: {error:?}"),
            Self::Abstraction(error) => write!(formatter, "spatial abstraction failed: {error:?}"),
            Self::Oracle(error) => write!(formatter, "oracle transition failed: {error}"),
            Self::Law(law) => write!(formatter, "spatial refinement law failed: {law}"),
            Self::TraceBound { seed } => write!(
                formatter,
                "seed {seed} exceeded the declared {MAX_TRACE_STEPS}-transition bound"
            ),
        }
    }
}

impl Error for SpatialCheckError {}

impl From<LayoutError> for SpatialCheckError {
    fn from(value: LayoutError) -> Self {
        Self::Layout(value)
    }
}

impl From<RealizationError> for SpatialCheckError {
    fn from(value: RealizationError) -> Self {
        Self::Realization(value)
    }
}

impl From<AbstractionError> for SpatialCheckError {
    fn from(value: AbstractionError) -> Self {
        Self::Abstraction(value)
    }
}

/// Exercise the v1 Rust spatial refinement over registered layouts and
/// deterministic complete oracle traces.
///
/// This is bounded evidence, not a proof over all shuffles or player policies.
/// Every bound and seed is carried by the returned report.
///
/// # Errors
///
/// Returns the first stable construction, oracle, or named-law failure.
#[allow(
    clippy::too_many_lines,
    reason = "the check keeps each measured refinement law adjacent to its exact counterexample boundary"
)]
pub fn check_spatial_refinement() -> Result<SpatialCheckReport, SpatialCheckError> {
    let registered_layouts = check_registered_layout_round_trips()?;
    let layout_id = LayoutId::new(2, 1).ok_or(SpatialCheckError::Law("two-player layout ID"))?;
    let layout = registered_layout(TableId::new(0x50_C4_E3), layout_id)?;
    let mut report = SpatialCheckReport {
        scope: SPATIAL_CHECK_SCOPE,
        seeds: (FIRST_SEED, LAST_SEED),
        registered_layouts,
        viewer_projections: 0,
        round_trips: registered_layouts,
        privacy_noninterference_pairs: 0,
        play_endpoint_commutations: 0,
        atomic_trick_resolutions: 0,
        finished_traces: 0,
        transitions: 0,
        negative_controls: negative_controls(&layout)?,
    };

    for seed in FIRST_SEED..=LAST_SEED {
        check_complete_trace(seed, &layout, &mut report)?;
    }
    if report.play_endpoint_commutations == 0 || report.atomic_trick_resolutions == 0 {
        return Err(SpatialCheckError::Law(
            "transition/realization scope was non-vacuous",
        ));
    }
    Ok(report)
}

fn check_registered_layout_round_trips() -> Result<usize, SpatialCheckError> {
    let mut checked = 0;
    for player_count in 2..=8 {
        let id =
            LayoutId::new(player_count, 1).ok_or(SpatialCheckError::Law("registered layout ID"))?;
        let layout = registered_layout(TableId::new(u64::from(player_count)), id)?;
        let projection = empty_projection(id, u64::from(player_count));
        let scene = realize_viewer_scene(&layout, &projection)?;
        if scene.cards.len() != 52 || scene.validate() != Ok(()) {
            return Err(SpatialCheckError::Law(
                "registered fixture conservation and attachment",
            ));
        }
        if abstract_viewer_scene(&layout, &scene)? != projection {
            return Err(SpatialCheckError::Law(
                "abstract(realize(registered fixture)) == fixture",
            ));
        }
        checked += 1;
    }
    Ok(checked)
}

fn check_complete_trace(
    seed: u8,
    layout: &SpatialLayout,
    report: &mut SpatialCheckReport,
) -> Result<(), SpatialCheckError> {
    let first = Seat::<PLAYERS>::new(0).map_err(oracle_error)?;
    let mut game = Game::<PLAYERS>::new(first).map_err(oracle_error)?;
    let mut deal_index = 0_usize;
    for step in 0..=MAX_TRACE_STEPS {
        check_state(seed, step, layout, &game, report)?;
        if game.phase() == PhaseTag::Finished {
            report.finished_traces += 1;
            return Ok(());
        }
        if step == MAX_TRACE_STEPS {
            return Err(SpatialCheckError::TraceBound { seed });
        }
        let action = deterministic_action(&game, seed, deal_index)?;
        if matches!(action, Action::Deal(_)) {
            deal_index += 1;
        }
        check_play_commutation(layout, &game, &action, report)?;
        game = game.transition(action).map_err(oracle_error)?.next;
        report.transitions += 1;
    }
    Err(SpatialCheckError::TraceBound { seed })
}

fn check_state(
    seed: u8,
    step: usize,
    layout: &SpatialLayout,
    game: &Game<PLAYERS>,
    report: &mut SpatialCheckReport,
) -> Result<(), SpatialCheckError> {
    let mut projections = Vec::with_capacity(PLAYERS);
    for viewer in 0..PLAYERS {
        let seat = Seat::<PLAYERS>::new(viewer).map_err(oracle_error)?;
        let projection = observation_projection(
            layout.id(),
            u64::from(seed) << 32 | u64::try_from(step).unwrap_or(u64::MAX),
            &game.observe(seat),
        )?;
        let scene = realize_viewer_scene(layout, &projection).map_err(|error| {
            SpatialCheckError::Oracle(format!(
                "seed {seed} step {step} viewer {viewer} phase {:?} projection {projection:?}: spatial realization {error:?}",
                game.phase()
            ))
        })?;
        if scene.cards.len() != 52 || scene.validate() != Ok(()) {
            return Err(SpatialCheckError::Law(
                "reachable scene uniqueness, conservation, and attachment",
            ));
        }
        let repeated = realize_viewer_scene(layout, &projection)?;
        if repeated != scene {
            return Err(SpatialCheckError::Law("endpoint determinism"));
        }
        if abstract_viewer_scene(layout, &scene)? != projection {
            return Err(SpatialCheckError::Law(
                "abstract(realize(reachable projection)) == projection",
            ));
        }
        report.viewer_projections += 1;
        report.round_trips += 1;
        projections.push(projection);
    }
    if public_projection(&projections[0]) != public_projection(&projections[1]) {
        return Err(SpatialCheckError::Law("privacy noninterference"));
    }
    report.privacy_noninterference_pairs += 1;
    Ok(())
}

fn check_play_commutation(
    layout: &SpatialLayout,
    game: &Game<PLAYERS>,
    action: &Action<PLAYERS>,
    report: &mut SpatialCheckReport,
) -> Result<(), SpatialCheckError> {
    let Action::Play { player, card } = action else {
        return Ok(());
    };
    let viewer = *player;
    let projection = observation_projection(layout.id(), 0xC0_4D_4D_55_7E, &game.observe(viewer))?;
    let scene = realize_viewer_scene(layout, &projection).map_err(|error| {
        SpatialCheckError::Oracle(format!(
            "play source phase {:?} actor {} projection {projection:?}: spatial realization {error:?}",
            game.phase(),
            player.index()
        ))
    })?;
    let seat = SeatId::new(
        u8::try_from(player.index()).map_err(|_| SpatialCheckError::Law("seat conversion"))?,
        layout.id(),
    )
    .ok_or(SpatialCheckError::Law("actor spatial seat"))?;
    let face = card_face(*card)?;
    let resolved = resolve_card_play(layout, &scene, seat, face)
        .map_err(|_| SpatialCheckError::Law("typed play resolves spatially"))?;
    let endpoint = reconstruct_animation_endpoint(layout, resolved.record)
        .map_err(|_| SpatialCheckError::Law("play endpoint reconstruction"))?;
    let before_len = match game.state() {
        GameState::Playing(state) => state.trick.len(),
        _ => return Err(SpatialCheckError::Law("play action occurs in Playing")),
    };
    let successor = game.transition(action.clone()).map_err(oracle_error)?.next;
    let successor_projection =
        observation_projection(layout.id(), 0xC0_4D_4D_55_7E, &successor.observe(viewer))?;
    let successor_scene = realize_viewer_scene(layout, &successor_projection).map_err(|error| {
        SpatialCheckError::Oracle(format!(
            "play successor phase {:?} actor {} projection {successor_projection:?}: spatial realization {error:?}",
            successor.phase(),
            player.index()
        ))
    })?;
    if before_len + 1 < PLAYERS {
        let played = successor_scene.cards.iter().find(|candidate| {
            candidate.location == CardLocation::Play { seat } && candidate.face == Some(face)
        });
        if played.is_none_or(|played| played.pose != endpoint.to) {
            return Err(SpatialCheckError::Law(
                "typed transition commutes with realized play endpoint",
            ));
        }
        report.play_endpoint_commutations += 1;
    } else {
        if successor_scene
            .cards
            .iter()
            .any(|candidate| matches!(candidate.location, CardLocation::Play { .. }))
        {
            return Err(SpatialCheckError::Law(
                "complete trick is atomically removed from play",
            ));
        }
        report.atomic_trick_resolutions += 1;
    }
    Ok(())
}

fn deterministic_action(
    game: &Game<PLAYERS>,
    seed: u8,
    deal_index: usize,
) -> Result<Action<PLAYERS>, SpatialCheckError> {
    match game.turn() {
        Turn::Chance => {
            let mut cards = Card::standard_deck();
            cards.rotate_left((usize::from(seed) + deal_index * 7) % 52);
            Ok(Action::Deal(DeckOrder::new(cards).map_err(oracle_error)?))
        }
        Turn::Player(_) => game
            .legal_player_actions()
            .into_iter()
            .next()
            .ok_or(SpatialCheckError::Law("player turn has a legal action")),
        Turn::Environment => Ok(Action::SettleRound),
        Turn::Finished => Err(SpatialCheckError::Law("finished state has no successor")),
    }
}

fn observation_projection(
    layout: LayoutId,
    projection_epoch: u64,
    observation: &Observation<PLAYERS>,
) -> Result<ViewerSpatialProjection, SpatialCheckError> {
    let mut players = Vec::with_capacity(PLAYERS);
    for ordinal in 0..PLAYERS {
        let seat = spatial_seat(layout, ordinal)?;
        let hand_count = observation.hand_counts[ordinal];
        let visible_hand = (ordinal == observation.viewer.index() && hand_count > 0)
            .then(|| {
                observation
                    .private_hand
                    .iter()
                    .map(card_face)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        players.push(PlayerSpatialProjection {
            seat,
            display_name: format!("player-{ordinal}"),
            score: observation.scores[ordinal],
            hand_count,
            visible_hand,
            tricks_won: observation.tricks_won[ordinal],
        });
    }
    let current_trick = observation
        .current_trick
        .iter()
        .map(|played| {
            Ok(poche_spatial::PlayedCardProjection {
                seat: spatial_seat(layout, played.player.index())?,
                face: card_face(played.card)?,
            })
        })
        .collect::<Result<Vec<_>, SpatialCheckError>>()?;
    Ok(ViewerSpatialProjection {
        projection_epoch,
        players,
        trump: observation.trump.map(card_face).transpose()?,
        current_trick,
        revealed_won_cards: Vec::<RevealedWonCard>::new(),
    })
}

fn public_projection(projection: &ViewerSpatialProjection) -> ViewerSpatialProjection {
    let mut public = projection.clone();
    for player in &mut public.players {
        player.visible_hand = None;
    }
    public
}

fn empty_projection(layout: LayoutId, projection_epoch: u64) -> ViewerSpatialProjection {
    ViewerSpatialProjection {
        projection_epoch,
        players: (0..layout.players())
            .map(|ordinal| PlayerSpatialProjection {
                seat: SeatId::new(ordinal, layout).expect("ordinal is in its layout"),
                display_name: format!("player-{ordinal}"),
                score: u16::from(ordinal),
                hand_count: 0,
                visible_hand: None,
                tricks_won: 0,
            })
            .collect(),
        trump: None,
        current_trick: Vec::new(),
        revealed_won_cards: Vec::new(),
    }
}

fn negative_controls(
    layout: &SpatialLayout,
) -> Result<Vec<NegativeSpatialControl>, SpatialCheckError> {
    let overlap = overlap_control(layout)?;
    let ambiguity = ambiguity_control(layout)?;
    let (face_text, hidden_text) = text_controls(layout)?;
    Ok(vec![overlap, ambiguity, face_text, hidden_text])
}

fn overlap_control(layout: &SpatialLayout) -> Result<NegativeSpatialControl, SpatialCheckError> {
    let mut zones = layout.zones().to_vec();
    let first = zones[0];
    let second = zones[1];
    zones[1] = ZoneVolume::new(second.id, first.inner, first.outer)
        .ok_or(SpatialCheckError::Law("overlap fault construction"))?;
    let result = SpatialLayout::try_new(
        layout.table_id(),
        layout.id(),
        layout.table(),
        layout.score_sheet(),
        layout.seats().to_vec(),
        zones,
    );
    if result != Err(LayoutError::OverlappingInnerZones) {
        return Err(SpatialCheckError::Law(
            "overlapping layout negative control",
        ));
    }
    Ok(NegativeSpatialControl {
        fault: "overlapping-inner-zones",
        witness: format!("{:?} and {:?} share {:?}", first.id, second.id, first.inner),
    })
}

fn ambiguity_control(layout: &SpatialLayout) -> Result<NegativeSpatialControl, SpatialCheckError> {
    let deck = layout
        .zones()
        .iter()
        .find(|zone| zone.id == ZoneId::Deck)
        .ok_or(SpatialCheckError::Law("deck zone exists"))?;
    let trump = layout
        .zones()
        .iter()
        .find(|zone| zone.id == ZoneId::Trump)
        .ok_or(SpatialCheckError::Law("trump zone exists"))?;
    let broad = AabbMm::new(
        Point3Mm::new(
            deck.outer.min.x.get(),
            deck.outer.min.y.get(),
            deck.outer.min.z.get(),
        ),
        Point3Mm::new(
            trump.outer.max.x.get(),
            trump.outer.max.y.get(),
            trump.outer.max.z.get(),
        ),
    )
    .ok_or(SpatialCheckError::Law("ambiguity witness bounds"))?;
    let ZoneClassification::Ambiguous(ids) = layout.classify_bounds(broad) else {
        return Err(SpatialCheckError::Law("ambiguity negative control"));
    };
    if ids.len() < 2 {
        return Err(SpatialCheckError::Law(
            "ambiguity names every implicated zone",
        ));
    }
    Ok(NegativeSpatialControl {
        fault: "first-match-classifier",
        witness: format!(
            "bounds {broad:?} implicate {ids:?}; selecting {:?} would be unsound",
            ids[0]
        ),
    })
}

fn text_controls(
    layout: &SpatialLayout,
) -> Result<(NegativeSpatialControl, NegativeSpatialControl), SpatialCheckError> {
    let id = layout.id();
    let seat_zero = spatial_seat(id, 0)?;
    let seat_one = spatial_seat(id, 1)?;
    let projection = ViewerSpatialProjection {
        projection_epoch: 77,
        players: vec![
            PlayerSpatialProjection {
                seat: seat_zero,
                display_name: "player-0".to_owned(),
                score: 0,
                hand_count: 1,
                visible_hand: Some(vec![CardFace::new(0).expect("card zero")]),
                tricks_won: 0,
            },
            PlayerSpatialProjection {
                seat: seat_one,
                display_name: "player-1".to_owned(),
                score: 0,
                hand_count: 1,
                visible_hand: None,
                tricks_won: 0,
            },
        ],
        trump: Some(CardFace::new(2).expect("distinct trump")),
        current_trick: Vec::new(),
        revealed_won_cards: Vec::new(),
    };
    let scene = realize_viewer_scene(layout, &projection)?;
    let mut wrong_face = scene.clone();
    let face_text = wrong_face
        .text
        .iter_mut()
        .find(|text| matches!(text.binding, TextBinding::CardFace(_)))
        .ok_or(SpatialCheckError::Law("known face text exists"))?;
    let face_witness = format!("{:?} text {:?}", face_text.binding, face_text.text);
    "A♠".clone_into(&mut face_text.text);
    if wrong_face.validate() != Err(SceneError::FaceTextMismatch) {
        return Err(SpatialCheckError::Law("face text negative control"));
    }

    let mut hidden_face = scene;
    let hidden = hidden_face
        .cards
        .iter()
        .find(|card| {
            card.location
                == CardLocation::Hand {
                    seat: seat_one,
                    index_from_left: 0,
                }
        })
        .copied()
        .ok_or(SpatialCheckError::Law("hidden hand card exists"))?;
    hidden_face
        .cards
        .iter_mut()
        .find(|card| card.id == hidden.id)
        .expect("card retained")
        .face = CardFace::new(1);
    if hidden_face.validate() != Err(SceneError::MissingFaceText) {
        return Err(SpatialCheckError::Law("hidden face negative control"));
    }
    Ok((
        NegativeSpatialControl {
            fault: "face-text-mismatch",
            witness: face_witness,
        },
        NegativeSpatialControl {
            fault: "unauthorized-hidden-face",
            witness: format!(
                "{:?} gained a face without an authorized text attachment",
                hidden.id
            ),
        },
    ))
}

fn spatial_seat(layout: LayoutId, ordinal: usize) -> Result<SeatId, SpatialCheckError> {
    let ordinal = u8::try_from(ordinal).map_err(|_| SpatialCheckError::Law("seat fits u8"))?;
    SeatId::new(ordinal, layout).ok_or(SpatialCheckError::Law("seat belongs to layout"))
}

fn card_face(card: Card) -> Result<CardFace, SpatialCheckError> {
    let code = Card::standard_deck()
        .iter()
        .position(|candidate| *candidate == card)
        .and_then(|index| u8::try_from(index).ok())
        .and_then(CardFace::new);
    code.ok_or(SpatialCheckError::Law(
        "oracle card has a standard dense code",
    ))
}

fn oracle_error(error: impl fmt::Debug) -> SpatialCheckError {
    SpatialCheckError::Oracle(format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::check_spatial_refinement;

    #[test]
    fn spatial_refinement_receipt_is_reproducible_and_non_vacuous() {
        let report = check_spatial_refinement().expect("declared spatial scope passes");
        let repeated = check_spatial_refinement().expect("same scope repeats");
        assert_eq!(report, repeated);
        assert_eq!(report.registered_layouts, 7);
        assert_eq!(report.finished_traces, 16);
        assert_eq!(report.negative_controls.len(), 4);
        assert!(report.viewer_projections > 0);
        assert!(report.play_endpoint_commutations > 0);
        assert!(report.atomic_trick_resolutions > 0);
        eprintln!("{report:#?}");
    }
}
