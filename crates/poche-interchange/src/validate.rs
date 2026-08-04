use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use crate::{
    ChanceActionWire, EnvironmentActionWire, EvidenceBundleWire, EvidenceContextWire, FixtureWire,
    GameOutcomeWire, ObservationWire, PhaseWire, PlayerActionWire, ScopeWire, SemanticHashWire,
    SolverResultWire, StateWire, TraceWire, TransitionWire,
};

/// Semantic validation failure after a structural Phon decode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    path: String,
    message: String,
}

impl ValidationError {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Return the stable field/index path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Return the validation explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl Error for ValidationError {}

/// An evidence bundle whose cross-model identities and semantic refinements
/// have been checked after decoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedEvidence(EvidenceBundleWire);

impl ValidatedEvidence {
    /// Borrow the validated wire payload.
    #[must_use]
    pub const fn wire(&self) -> &EvidenceBundleWire {
        &self.0
    }

    /// Recover the validated wire payload.
    #[must_use]
    pub fn into_wire(self) -> EvidenceBundleWire {
        self.0
    }
}

impl TryFrom<EvidenceBundleWire> for ValidatedEvidence {
    type Error = ValidationError;

    fn try_from(bundle: EvidenceBundleWire) -> Result<Self, Self::Error> {
        validate_fixture(&bundle.fixture, "fixture")?;
        validate_result(&bundle.result, "result")?;
        validate_compatible_contexts(
            &bundle.fixture.context,
            &bundle.result.context,
            "fixture.context",
            "result.context",
        )?;
        if let Some(trace) = &bundle.result.trace
            && trace.fixture_id != bundle.fixture.fixture_id
        {
            return Err(ValidationError::new(
                "result.trace.fixture_id",
                "does not identify the bundled fixture",
            ));
        }
        Ok(Self(bundle))
    }
}

fn validate_fixture(fixture: &FixtureWire, path: &str) -> Result<(), ValidationError> {
    validate_context(&fixture.context, &format!("{path}.context"))?;
    require_text(&fixture.fixture_id, &format!("{path}.fixture_id"))?;
    require_text(&fixture.description, &format!("{path}.description"))?;
    validate_state(
        &fixture.state,
        &fixture.context.scope,
        &format!("{path}.state"),
    )?;
    for (index, observation) in fixture.observations.iter().enumerate() {
        validate_observation(
            observation,
            &fixture.state,
            &fixture.context.scope,
            &format!("{path}.observations[{index}]"),
        )?;
    }
    for (index, action) in fixture.legal_actions.iter().enumerate() {
        validate_player_action(
            *action,
            &fixture.state,
            &fixture.context.scope,
            &format!("{path}.legal_actions[{index}]"),
        )?;
    }
    for (index, action) in fixture.chance_actions.iter().enumerate() {
        validate_chance_action(
            action,
            &fixture.context.scope,
            &format!("{path}.chance_actions[{index}]"),
        )?;
    }
    if let Some(transition) = &fixture.expected_transition {
        validate_transition(
            transition,
            &fixture.state,
            &fixture.context.scope,
            &format!("{path}.expected_transition"),
        )?;
    }
    Ok(())
}

fn validate_result(result: &SolverResultWire, path: &str) -> Result<(), ValidationError> {
    validate_context(&result.context, &format!("{path}.context"))?;
    require_text(&result.summary, &format!("{path}.summary"))?;
    for (index, binding) in result.bindings.iter().enumerate() {
        require_text(
            &binding.variable,
            &format!("{path}.bindings[{index}].variable"),
        )?;
        require_text(&binding.term, &format!("{path}.bindings[{index}].term"))?;
    }
    let mut metric_names = BTreeSet::new();
    for (index, statistic) in result.statistics.backend.iter().enumerate() {
        require_text(
            &statistic.name,
            &format!("{path}.statistics.backend[{index}].name"),
        )?;
        if !metric_names.insert(&statistic.name) {
            return Err(ValidationError::new(
                format!("{path}.statistics.backend[{index}].name"),
                "duplicates a backend statistic name",
            ));
        }
    }
    for (index, diagnostic) in result.raw_diagnostics.iter().enumerate() {
        require_text(
            &diagnostic.stream,
            &format!("{path}.raw_diagnostics[{index}].stream"),
        )?;
        require_text(
            &diagnostic.severity,
            &format!("{path}.raw_diagnostics[{index}].severity"),
        )?;
    }
    if let Some(trace) = &result.trace {
        validate_trace(trace, path)?;
        if trace.context != result.context {
            return Err(ValidationError::new(
                format!("{path}.trace.context"),
                "must exactly match the enclosing solver-result context",
            ));
        }
    }
    Ok(())
}

fn validate_context(context: &EvidenceContextWire, path: &str) -> Result<(), ValidationError> {
    require_text(&context.model.model_id, &format!("{path}.model.model_id"))?;
    require_text(
        &context.model.model_revision,
        &format!("{path}.model.model_revision"),
    )?;
    require_text(&context.model.schema_id, &format!("{path}.model.schema_id"))?;
    require_hash(
        context.model.schema_semantic_hash,
        &format!("{path}.model.schema_semantic_hash"),
    )?;
    require_text(
        &context.model.rules_revision,
        &format!("{path}.model.rules_revision"),
    )?;
    require_hash(
        context.model.rules_semantic_hash,
        &format!("{path}.model.rules_semantic_hash"),
    )?;
    require_hash(
        context.model.observation_semantic_hash,
        &format!("{path}.model.observation_semantic_hash"),
    )?;
    require_hash(
        context.model.scoring_semantic_hash,
        &format!("{path}.model.scoring_semantic_hash"),
    )?;
    validate_scope(&context.scope, &format!("{path}.scope"))?;
    require_text(&context.backend.version, &format!("{path}.backend.version"))?;
    require_text(&context.subject.id, &format!("{path}.subject.id"))?;
    require_text(
        &context.confidence.qualification,
        &format!("{path}.confidence.qualification"),
    )?;
    if context.rules.is_empty() {
        return Err(ValidationError::new(
            format!("{path}.rules"),
            "must identify at least one governing rule",
        ));
    }
    let mut rules = BTreeSet::new();
    for (index, rule) in context.rules.iter().enumerate() {
        require_text(&rule.rule_id, &format!("{path}.rules[{index}].rule_id"))?;
        require_text(&rule.source, &format!("{path}.rules[{index}].source"))?;
        if !rules.insert(&rule.rule_id) {
            return Err(ValidationError::new(
                format!("{path}.rules[{index}].rule_id"),
                "duplicates a rule ID",
            ));
        }
    }
    Ok(())
}

fn validate_scope(scope: &ScopeWire, path: &str) -> Result<(), ValidationError> {
    require_text(&scope.scope_id, &format!("{path}.scope_id"))?;
    if !(2..=51).contains(&scope.player_count) {
        return Err(ValidationError::new(
            format!("{path}.player_count"),
            "must be within the supported 2..=51 range",
        ));
    }
    if scope.suit_count == 0 || scope.ranks_per_suit == 0 {
        return Err(ValidationError::new(
            path,
            "suit and rank counts must both be nonzero",
        ));
    }
    let computed_deck = u16::from(scope.suit_count) * u16::from(scope.ranks_per_suit);
    if computed_deck != u16::from(scope.deck_size) {
        return Err(ValidationError::new(
            format!("{path}.deck_size"),
            "must equal suit_count * ranks_per_suit",
        ));
    }
    if scope.trump_card_count > 1 {
        return Err(ValidationError::new(
            format!("{path}.trump_card_count"),
            "the current Poche state schema supports zero or one trump card",
        ));
    }
    let dealt = u16::from(scope.player_count) * u16::from(scope.cards_per_player)
        + u16::from(scope.trump_card_count)
        + u16::from(scope.undealt_card_count);
    if dealt != u16::from(scope.deck_size) {
        return Err(ValidationError::new(
            path,
            "player hands, trump, and undealt counts must partition the deck after a deal",
        ));
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "keeping all state refinements together makes the wire trust boundary auditable"
)]
fn validate_state(state: &StateWire, scope: &ScopeWire, path: &str) -> Result<(), ValidationError> {
    let players = usize::from(scope.player_count);
    require_len(&state.hands, players, &format!("{path}.hands"))?;
    require_len(&state.bids, players, &format!("{path}.bids"))?;
    require_len(&state.tricks_won, players, &format!("{path}.tricks_won"))?;
    require_len(
        &state.cumulative_scores,
        players,
        &format!("{path}.cumulative_scores"),
    )?;
    require_player(state.dealer, scope, &format!("{path}.dealer"))?;
    if let Some(actor) = state.actor {
        require_player(actor, scope, &format!("{path}.actor"))?;
    }
    if state.hand_size != scope.cards_per_player {
        return Err(ValidationError::new(
            format!("{path}.hand_size"),
            "must match the declared scope hand size",
        ));
    }
    match state.phase {
        PhaseWire::Bidding | PhaseWire::Playing if state.actor.is_none() => {
            return Err(ValidationError::new(
                format!("{path}.actor"),
                "player-owned phase requires an actor",
            ));
        }
        PhaseWire::AwaitingDeal | PhaseWire::Scoring | PhaseWire::Finished
            if state.actor.is_some() =>
        {
            return Err(ValidationError::new(
                format!("{path}.actor"),
                "chance, environment, and finished phases cannot have a player actor",
            ));
        }
        _ => {}
    }
    for (player, hand) in state.hands.iter().enumerate() {
        if hand.len() > usize::from(scope.cards_per_player) {
            return Err(ValidationError::new(
                format!("{path}.hands[{player}]"),
                "contains more cards than the declared hand size",
            ));
        }
    }
    for (player, bid) in state.bids.iter().enumerate() {
        if bid.is_some_and(|bid| bid > scope.cards_per_player) {
            return Err(ValidationError::new(
                format!("{path}.bids[{player}]"),
                "bid exceeds the round hand size",
            ));
        }
    }

    let mut cards = Vec::with_capacity(usize::from(scope.deck_size));
    for hand in &state.hands {
        cards.extend(hand.iter().copied());
    }
    if let Some(trump) = state.trump {
        cards.push(trump);
    }
    cards.extend(state.undealt.iter().copied());
    validate_trick(
        &state.current_trick,
        scope,
        &format!("{path}.current_trick"),
        false,
    )?;
    cards.extend(state.current_trick.iter().map(|play| play.card));
    for (index, trick) in state.completed_tricks.iter().enumerate() {
        validate_trick(
            trick,
            scope,
            &format!("{path}.completed_tricks[{index}]"),
            true,
        )?;
        cards.extend(trick.iter().map(|play| play.card));
    }
    if state.phase == PhaseWire::AwaitingDeal {
        if !cards.is_empty() {
            return Err(ValidationError::new(
                path,
                "awaiting-deal state must not retain a prior deck partition",
            ));
        }
    } else {
        require_card_partition(&cards, scope, path)?;
        if state.trump.is_some() != (scope.trump_card_count == 1) {
            return Err(ValidationError::new(
                format!("{path}.trump"),
                "presence does not match the scope trump-card count",
            ));
        }
        if state.undealt.len() != usize::from(scope.undealt_card_count) {
            return Err(ValidationError::new(
                format!("{path}.undealt"),
                "length does not match the scope undealt-card count",
            ));
        }
    }
    let completed = u16::try_from(state.completed_tricks.len()).map_err(|_| {
        ValidationError::new(
            format!("{path}.completed_tricks"),
            "trick count does not fit the finite score domain",
        )
    })?;
    let claimed: u16 = state.tricks_won.iter().map(|count| u16::from(*count)).sum();
    if claimed != completed {
        return Err(ValidationError::new(
            format!("{path}.tricks_won"),
            "sum must equal the number of completed tricks",
        ));
    }
    Ok(())
}

fn validate_observation(
    observation: &ObservationWire,
    state: &StateWire,
    scope: &ScopeWire,
    path: &str,
) -> Result<(), ValidationError> {
    require_player(observation.viewer, scope, &format!("{path}.viewer"))?;
    let players = usize::from(scope.player_count);
    require_len(
        &observation.hand_counts,
        players,
        &format!("{path}.hand_counts"),
    )?;
    require_len(&observation.bids, players, &format!("{path}.bids"))?;
    require_len(
        &observation.tricks_won,
        players,
        &format!("{path}.tricks_won"),
    )?;
    require_len(
        &observation.cumulative_scores,
        players,
        &format!("{path}.cumulative_scores"),
    )?;
    let viewer = usize::from(observation.viewer);
    let expected_counts: Vec<u8> = state
        .hands
        .iter()
        .map(|hand| u8::try_from(hand.len()).unwrap_or(u8::MAX))
        .collect();
    if observation.state_id != state.state_id
        || observation.phase != state.phase
        || observation.dealer != state.dealer
        || observation.actor != state.actor
        || observation.own_hand != state.hands[viewer]
        || observation.hand_counts != expected_counts
        || observation.trump != state.trump
        || observation.current_trick != state.current_trick
        || observation.bids != state.bids
        || observation.tricks_won != state.tricks_won
        || observation.cumulative_scores != state.cumulative_scores
        || observation.pot_cents != state.pot_cents
    {
        return Err(ValidationError::new(
            path,
            "does not equal the viewer-specific projection of the complete state",
        ));
    }
    Ok(())
}

fn validate_player_action(
    action: PlayerActionWire,
    state: &StateWire,
    scope: &ScopeWire,
    path: &str,
) -> Result<(), ValidationError> {
    let (player, card) = match action {
        PlayerActionWire::Bid { player, tricks } => {
            if tricks > state.hand_size {
                return Err(ValidationError::new(
                    path,
                    "bid action exceeds the current hand size",
                ));
            }
            (player, None)
        }
        PlayerActionWire::Play { player, card } => (player, Some(card)),
    };
    require_player(player, scope, path)?;
    if state.actor != Some(player) {
        return Err(ValidationError::new(
            path,
            "action is not owned by the acting player",
        ));
    }
    if let Some(card) = card {
        require_card(card, scope, path)?;
        if !state.hands[usize::from(player)].contains(&card) {
            return Err(ValidationError::new(
                path,
                "play action card is absent from the acting player's hand",
            ));
        }
    }
    Ok(())
}

fn validate_chance_action(
    action: &ChanceActionWire,
    scope: &ScopeWire,
    path: &str,
) -> Result<(), ValidationError> {
    require_card_partition(&action.deck, scope, &format!("{path}.deck"))
}

fn validate_transition(
    transition: &TransitionWire,
    before: &StateWire,
    scope: &ScopeWire,
    path: &str,
) -> Result<(), ValidationError> {
    if transition.before_state_id != before.state_id {
        return Err(ValidationError::new(
            format!("{path}.before_state_id"),
            "does not match the supplied predecessor state",
        ));
    }
    match &transition.action {
        EnvironmentActionWire::Player(action) => {
            validate_player_action(*action, before, scope, &format!("{path}.action"))?;
        }
        EnvironmentActionWire::Chance(action) => {
            if before.phase != PhaseWire::AwaitingDeal {
                return Err(ValidationError::new(
                    format!("{path}.action"),
                    "chance deal is only valid while awaiting a deal",
                ));
            }
            validate_chance_action(action, scope, &format!("{path}.action"))?;
        }
        EnvironmentActionWire::Settle if before.phase != PhaseWire::Scoring => {
            return Err(ValidationError::new(
                format!("{path}.action"),
                "settlement is only valid in the scoring phase",
            ));
        }
        EnvironmentActionWire::Settle => {}
    }
    validate_state(&transition.after, scope, &format!("{path}.after"))?;
    if transition.after.state_id == before.state_id {
        return Err(ValidationError::new(
            format!("{path}.after.state_id"),
            "a transition must produce a distinct trace-state identity",
        ));
    }
    if !transition.round_scores.is_empty() {
        require_len(
            &transition.round_scores,
            usize::from(scope.player_count),
            &format!("{path}.round_scores"),
        )?;
        let mut players = BTreeSet::new();
        for (index, score) in transition.round_scores.iter().enumerate() {
            require_player(
                score.player,
                scope,
                &format!("{path}.round_scores[{index}].player"),
            )?;
            if !players.insert(score.player) {
                return Err(ValidationError::new(
                    format!("{path}.round_scores[{index}].player"),
                    "duplicates a scored player",
                ));
            }
            if score.bid > before.hand_size || score.tricks_won > before.hand_size {
                return Err(ValidationError::new(
                    format!("{path}.round_scores[{index}]"),
                    "bid or tricks won exceeds the round hand size",
                ));
            }
            require_text(
                &score.score_rule_id,
                &format!("{path}.round_scores[{index}].score_rule_id"),
            )?;
            require_text(
                &score.money_rule_id,
                &format!("{path}.round_scores[{index}].money_rule_id"),
            )?;
        }
    }
    if let Some(outcome) = &transition.game_outcome {
        validate_game_outcome(outcome, scope, &format!("{path}.game_outcome"))?;
        if transition.after.phase != PhaseWire::Finished {
            return Err(ValidationError::new(
                format!("{path}.game_outcome"),
                "a final outcome requires a finished successor",
            ));
        }
    }
    if transition.rule_ids.is_empty() {
        return Err(ValidationError::new(
            format!("{path}.rule_ids"),
            "transition must retain at least one rule origin",
        ));
    }
    for (index, diff) in transition.diffs.iter().enumerate() {
        require_text(&diff.path, &format!("{path}.diffs[{index}].path"))?;
        if diff.before == diff.after {
            return Err(ValidationError::new(
                format!("{path}.diffs[{index}]"),
                "state diff must change a value",
            ));
        }
        if diff.rule_ids.is_empty() {
            return Err(ValidationError::new(
                format!("{path}.diffs[{index}].rule_ids"),
                "state diff must retain at least one rule origin",
            ));
        }
    }
    Ok(())
}

fn validate_trace(trace: &TraceWire, result_path: &str) -> Result<(), ValidationError> {
    let path = format!("{result_path}.trace");
    validate_context(&trace.context, &format!("{path}.context"))?;
    require_text(&trace.fixture_id, &format!("{path}.fixture_id"))?;
    validate_state(
        &trace.initial,
        &trace.context.scope,
        &format!("{path}.initial"),
    )?;
    let mut before = &trace.initial;
    for (index, step) in trace.steps.iter().enumerate() {
        if usize::try_from(step.index).ok() != Some(index) {
            return Err(ValidationError::new(
                format!("{path}.steps[{index}].index"),
                "trace indices must be contiguous and zero-based",
            ));
        }
        if let Some(observation) = &step.observation {
            validate_observation(
                observation,
                before,
                &trace.context.scope,
                &format!("{path}.steps[{index}].observation"),
            )?;
        }
        for (action_index, action) in step.legal_actions.iter().enumerate() {
            validate_player_action(
                *action,
                before,
                &trace.context.scope,
                &format!("{path}.steps[{index}].legal_actions[{action_index}]"),
            )?;
        }
        validate_transition(
            &step.transition,
            before,
            &trace.context.scope,
            &format!("{path}.steps[{index}].transition"),
        )?;
        before = &step.transition.after;
    }
    if trace.cycle_start.is_some_and(|start| {
        usize::try_from(start).map_or(true, |start| start >= trace.steps.len())
    }) {
        return Err(ValidationError::new(
            format!("{path}.cycle_start"),
            "must identify an existing trace step",
        ));
    }
    Ok(())
}

fn validate_game_outcome(
    outcome: &GameOutcomeWire,
    scope: &ScopeWire,
    path: &str,
) -> Result<(), ValidationError> {
    require_len(
        &outcome.scores,
        usize::from(scope.player_count),
        &format!("{path}.scores"),
    )?;
    if outcome.winners.is_empty() {
        return Err(ValidationError::new(
            format!("{path}.winners"),
            "must retain every seat tied for highest score",
        ));
    }
    let mut winners = BTreeSet::new();
    for (index, winner) in outcome.winners.iter().enumerate() {
        require_player(*winner, scope, &format!("{path}.winners[{index}]"))?;
        if !winners.insert(*winner) {
            return Err(ValidationError::new(
                format!("{path}.winners[{index}]"),
                "duplicates a winner",
            ));
        }
    }
    let Some(max_score) = outcome.scores.iter().max() else {
        return Err(ValidationError::new(
            format!("{path}.scores"),
            "cannot determine winners without scores",
        ));
    };
    let expected: BTreeSet<u8> = outcome
        .scores
        .iter()
        .enumerate()
        .filter(|(_, score)| *score == max_score)
        .map(|(player, _)| u8::try_from(player).unwrap_or(u8::MAX))
        .collect();
    if winners != expected {
        return Err(ValidationError::new(
            format!("{path}.winners"),
            "does not equal the complete maximum-score tie set",
        ));
    }
    let mut share_players = BTreeSet::new();
    let mut assigned = u64::from(outcome.remainder_cents);
    for (index, share) in outcome.pot_shares.iter().enumerate() {
        if !winners.contains(&share.player) || !share_players.insert(share.player) {
            return Err(ValidationError::new(
                format!("{path}.pot_shares[{index}].player"),
                "must identify one unique winner",
            ));
        }
        assigned += u64::from(share.cents);
    }
    if share_players != winners || assigned != u64::from(outcome.pot_cents) {
        return Err(ValidationError::new(
            format!("{path}.pot_shares"),
            "shares plus remainder must divide the full pot across every winner",
        ));
    }
    Ok(())
}

fn validate_trick(
    trick: &[crate::PlayedCardWire],
    scope: &ScopeWire,
    path: &str,
    complete: bool,
) -> Result<(), ValidationError> {
    let players = usize::from(scope.player_count);
    if (complete && trick.len() != players) || (!complete && trick.len() >= players) {
        return Err(ValidationError::new(
            path,
            if complete {
                "completed trick must contain exactly one play per player"
            } else {
                "current trick must contain fewer than one play per player"
            },
        ));
    }
    let mut seats = BTreeSet::new();
    for (index, play) in trick.iter().enumerate() {
        require_player(play.player, scope, &format!("{path}[{index}].player"))?;
        require_card(play.card, scope, &format!("{path}[{index}].card"))?;
        if !seats.insert(play.player) {
            return Err(ValidationError::new(
                format!("{path}[{index}].player"),
                "one player appears twice in a trick",
            ));
        }
    }
    Ok(())
}

fn validate_compatible_contexts(
    fixture: &EvidenceContextWire,
    result: &EvidenceContextWire,
    fixture_path: &str,
    result_path: &str,
) -> Result<(), ValidationError> {
    let compatible = fixture.model.schema_id == result.model.schema_id
        && fixture.model.schema_semantic_hash == result.model.schema_semantic_hash
        && fixture.model.rules_revision == result.model.rules_revision
        && fixture.model.rules_semantic_hash == result.model.rules_semantic_hash
        && fixture.model.observation_semantic_hash == result.model.observation_semantic_hash
        && fixture.model.scoring_semantic_hash == result.model.scoring_semantic_hash
        && fixture.scope == result.scope
        && fixture.subject == result.subject
        && fixture.rules == result.rules;
    if compatible {
        Ok(())
    } else {
        Err(ValidationError::new(
            result_path,
            format!(
                "is not semantically compatible with {fixture_path}; schema, rules, scope, subject, observation, scoring, and rule origins must match"
            ),
        ))
    }
}

fn require_card_partition(
    cards: &[u8],
    scope: &ScopeWire,
    path: &str,
) -> Result<(), ValidationError> {
    if cards.len() != usize::from(scope.deck_size) {
        return Err(ValidationError::new(
            path,
            format!(
                "must contain all {} cards exactly once, found {}",
                scope.deck_size,
                cards.len()
            ),
        ));
    }
    let mut unique = BTreeSet::new();
    for (index, card) in cards.iter().enumerate() {
        require_card(*card, scope, &format!("{path}[{index}]"))?;
        if !unique.insert(*card) {
            return Err(ValidationError::new(
                format!("{path}[{index}]"),
                "duplicates a card identity",
            ));
        }
    }
    Ok(())
}

fn require_player(player: u8, scope: &ScopeWire, path: &str) -> Result<(), ValidationError> {
    if player < scope.player_count {
        Ok(())
    } else {
        Err(ValidationError::new(
            path,
            format!("player {player} is outside 0..{}", scope.player_count),
        ))
    }
}

fn require_card(card: u8, scope: &ScopeWire, path: &str) -> Result<(), ValidationError> {
    if card < scope.deck_size {
        Ok(())
    } else {
        Err(ValidationError::new(
            path,
            format!("card {card} is outside 0..{}", scope.deck_size),
        ))
    }
}

fn require_hash(hash: SemanticHashWire, path: &str) -> Result<(), ValidationError> {
    if hash.is_zero() {
        Err(ValidationError::new(
            path,
            "zero is reserved for a missing semantic hash",
        ))
    } else {
        Ok(())
    }
}

fn require_text(value: &str, path: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::new(path, "must not be empty"))
    } else {
        Ok(())
    }
}

fn require_len<T>(values: &[T], expected: usize, path: &str) -> Result<(), ValidationError> {
    if values.len() == expected {
        Ok(())
    } else {
        Err(ValidationError::new(
            path,
            format!("expected length {expected}, found {}", values.len()),
        ))
    }
}
