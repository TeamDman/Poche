use facet::Facet;
use figue as args;
use poche_player_client::AdvertisedActionPolicy;

use crate::cli::ParseError;

#[derive(Facet, PartialEq, Eq)]
pub struct AgentArgs {
    #[facet(args::subcommand)]
    pub command: AgentCommand,
}

/// Persistent policy devices. The policy chooses only from advertised actions.
#[derive(Facet, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum AgentCommand {
    Run {
        #[facet(args::positional)]
        profile: String,
        #[facet(args::positional)]
        room: String,
        #[facet(args::positional)]
        policy: String,
    },
}

impl AgentArgs {
    pub(crate) fn validate(&self) -> Result<(), ParseError> {
        self.policy().map(|_| ())
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        "run"
    }

    /// Parse the stable baseline-policy vocabulary. Learned/Burn policies are
    /// deliberately not accepted by this phase-five device command.
    ///
    /// # Errors
    ///
    /// Returns a value-free parse error for an unknown policy or invalid seed.
    pub fn policy(&self) -> Result<AdvertisedActionPolicy, ParseError> {
        let AgentCommand::Run { policy, .. } = &self.command;
        if policy == "first-legal" {
            return Ok(AdvertisedActionPolicy::FirstLegal);
        }
        let Some(seed) = policy.strip_prefix("seeded-random:") else {
            return Err(ParseError::new(
                "policy must be first-legal or seeded-random:<seed>",
            ));
        };
        let seed = seed
            .parse()
            .map_err(|_| ParseError::new("policy must be first-legal or seeded-random:<seed>"))?;
        Ok(AdvertisedActionPolicy::SeededRandom { seed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(policy: &str) -> AgentArgs {
        AgentArgs {
            command: AgentCommand::Run {
                profile: "alice-agent".to_owned(),
                room: "room-1".to_owned(),
                policy: policy.to_owned(),
            },
        }
    }

    #[test]
    fn baseline_policy_vocabulary_is_typed_and_bounded() {
        assert_eq!(
            args("first-legal").policy().unwrap(),
            AdvertisedActionPolicy::FirstLegal
        );
        assert_eq!(
            args("seeded-random:41").policy().unwrap(),
            AdvertisedActionPolicy::SeededRandom { seed: 41 }
        );
        assert!(args("burn:model.bin").policy().is_err());
        assert!(args("seeded-random:not-a-number").policy().is_err());
    }
}
