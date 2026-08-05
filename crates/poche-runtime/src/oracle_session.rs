use poche_environment::{
    EnvironmentAction, GameEnvironment, OracleChanceAction, OracleEnvironment, OraclePlayerAction,
    TurnOwner,
};
use poche_oracle_rust::{Card, DeckOrder, Game, RuleViolation, Seat};
use poche_protocol::{ChanceWire, GameActionWire, PrincipalId};
use poche_session::{GameTransition, GameTurn, SessionGame};

/// Existing full-rule Poche game adapted to the pure multiplayer session gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleSessionGame<const PLAYERS: usize> {
    /// Complete hidden game state. Viewer projections remain separate.
    pub game: Game<PLAYERS>,
}

/// Failed conversion or full-rule game transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OracleSessionGameError {
    InvalidSeatComposition,
    InvalidCardCode,
    ChanceProvenanceMismatch,
    Rule(RuleViolation),
}

impl<const PLAYERS: usize> SessionGame for OracleSessionGame<PLAYERS> {
    type Error = OracleSessionGameError;

    fn start(seats: &[(u8, PrincipalId)]) -> Result<Self, Self::Error> {
        if seats.len() != PLAYERS
            || seats
                .iter()
                .enumerate()
                .any(|(index, (seat, _))| usize::from(*seat) != index)
        {
            return Err(OracleSessionGameError::InvalidSeatComposition);
        }
        let first_dealer = Seat::new(0).map_err(OracleSessionGameError::Rule)?;
        let game = OracleEnvironment::<PLAYERS>::initial(first_dealer)
            .map_err(OracleSessionGameError::Rule)?;
        Ok(Self { game })
    }

    fn turn(&self) -> GameTurn {
        match OracleEnvironment::<PLAYERS>::turn(&self.game) {
            TurnOwner::Chance => GameTurn::Chance,
            TurnOwner::Agent(seat) => GameTurn::Player(
                u8::try_from(seat.index()).expect("the oracle supports at most 51 seats"),
            ),
            TurnOwner::Environment => GameTurn::Environment,
            TurnOwner::Finished => GameTurn::Finished,
        }
    }

    fn player_transition(
        &self,
        seat: u8,
        action: &GameActionWire,
    ) -> Result<GameTransition<Self>, Self::Error> {
        let seat = Seat::new(usize::from(seat)).map_err(OracleSessionGameError::Rule)?;
        let action = match action {
            GameActionWire::Bid { tricks } => OraclePlayerAction::Bid {
                player: seat,
                tricks: *tricks,
            },
            GameActionWire::Play { card } => OraclePlayerAction::Play {
                player: seat,
                card: card_from_code(*card)?,
            },
        };
        transition(&self.game, EnvironmentAction::Player(action))
    }

    fn chance_transition(&self, chance: &ChanceWire) -> Result<GameTransition<Self>, Self::Error> {
        let codes: [u8; 52] = chance
            .cards
            .as_slice()
            .try_into()
            .map_err(|_| OracleSessionGameError::InvalidCardCode)?;
        let cards = codes
            .map(card_from_code)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let cards: [Card; 52] = cards
            .try_into()
            .map_err(|_| OracleSessionGameError::InvalidCardCode)?;
        let deck = DeckOrder::new(cards).map_err(OracleSessionGameError::Rule)?;
        let chance_action = match (chance.seed, chance.deal_ordinal) {
            (None, None) => OracleChanceAction::explicit(deck),
            (Some(seed), Some(deal_ordinal)) => {
                let seeded = OracleChanceAction::seeded(seed, deal_ordinal);
                if seeded.deck != deck {
                    return Err(OracleSessionGameError::ChanceProvenanceMismatch);
                }
                seeded
            }
            _ => return Err(OracleSessionGameError::ChanceProvenanceMismatch),
        };
        transition(&self.game, EnvironmentAction::Chance(chance_action))
    }

    fn settle(&self) -> Result<GameTransition<Self>, Self::Error> {
        transition(&self.game, EnvironmentAction::Settle)
    }
}

fn transition<const PLAYERS: usize>(
    game: &Game<PLAYERS>,
    action: EnvironmentAction<OraclePlayerAction<PLAYERS>, OracleChanceAction>,
) -> Result<GameTransition<OracleSessionGame<PLAYERS>>, OracleSessionGameError> {
    let outcome = OracleEnvironment::<PLAYERS>::transition(game, action)
        .map_err(OracleSessionGameError::Rule)?;
    let round_scores = outcome.round_scores.map(|scores| {
        scores
            .into_iter()
            .map(|event| i32::from(event.score.points))
            .collect()
    });
    Ok(GameTransition {
        terminal: outcome.game_outcome.is_some(),
        game: OracleSessionGame {
            game: outcome.state,
        },
        round_scores,
    })
}

fn card_from_code(code: u8) -> Result<Card, OracleSessionGameError> {
    Card::standard_deck()
        .get(usize::from(code))
        .copied()
        .ok_or(OracleSessionGameError::InvalidCardCode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poche_environment::ChanceProvenance;

    fn principal(value: &str) -> PrincipalId {
        PrincipalId::new(value).unwrap()
    }

    #[test]
    fn concrete_oracle_adapter_starts_and_replays_seeded_chance() {
        let game = OracleSessionGame::<2>::start(&[
            (0, principal("player-zero")),
            (1, principal("player-one")),
        ])
        .unwrap();
        assert_eq!(game.turn(), GameTurn::Chance);

        let seeded = OracleChanceAction::seeded(7, 0);
        let standard = Card::standard_deck();
        let cards = seeded
            .deck
            .cards()
            .iter()
            .map(|card| {
                u8::try_from(
                    standard
                        .iter()
                        .position(|candidate| candidate == card)
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let dealt = game
            .chance_transition(&ChanceWire {
                cards,
                seed: Some(7),
                deal_ordinal: Some(0),
            })
            .unwrap();
        assert!(matches!(dealt.game.turn(), GameTurn::Player(_)));
        assert!(!dealt.terminal);
    }

    #[test]
    fn chance_provenance_must_match_explicit_deck() {
        let game = OracleSessionGame::<2>::start(&[
            (0, principal("player-zero")),
            (1, principal("player-one")),
        ])
        .unwrap();
        assert_eq!(
            game.chance_transition(&ChanceWire {
                cards: (0_u8..52).collect(),
                seed: Some(7),
                deal_ordinal: Some(0),
            }),
            Err(OracleSessionGameError::ChanceProvenanceMismatch)
        );
    }

    #[test]
    fn chance_provenance_type_remains_explicit() {
        assert_ne!(
            ChanceProvenance::Explicit,
            ChanceProvenance::Seeded {
                seed: 0,
                deal_ordinal: 0,
            }
        );
    }
}
