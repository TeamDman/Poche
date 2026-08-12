use core::fmt;

use facet::Facet;
use serde::{Deserialize, Serialize};

/// A rejected textual protocol identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentifierError;

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("identifier must be 1-64 ASCII alphanumeric, '-', '_', or '.' bytes")
    }
}

impl std::error::Error for IdentifierError {}

fn is_valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

macro_rules! text_identifier {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validate and construct an identifier.
            ///
            /// # Errors
            ///
            /// Returns [`IdentifierError`] for empty, oversized, or noncanonical text.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                if is_valid_identifier(&value) {
                    Ok(Self(value))
                } else {
                    Err(IdentifierError)
                }
            }

            /// Return the canonical textual representation.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Return whether this value satisfies the canonical identifier
            /// refinement after reflection or decoding.
            #[must_use]
            pub fn validate(&self) -> bool {
                is_valid_identifier(&self.0)
            }
        }
    };
}

text_identifier!(
    /// Stable room identifier.
    RoomId
);
text_identifier!(
    /// Stable application-principal identifier.
    PrincipalId
);
text_identifier!(
    /// Stable application device-key identifier, distinct from transport IDs.
    DeviceId
);
text_identifier!(
    /// Stable player-root-issued device certificate identifier.
    CertificateId
);
text_identifier!(
    /// Idempotency identifier for one command.
    CommandId
);
text_identifier!(
    /// Identifier for one authoritative event.
    EventId
);
text_identifier!(
    /// Identifier shared by one user-visible operation and its results.
    CorrelationId
);
text_identifier!(
    /// Identifier for one snapshot.
    SnapshotId
);
text_identifier!(
    /// Identifier for one projection.
    ProjectionId
);
text_identifier!(
    /// Logical countdown token.
    CountdownToken
);
text_identifier!(
    /// Stable policy identifier recorded with a decision.
    PolicyId
);
text_identifier!(
    /// Stable governance proposal identifier.
    ProposalId
);
text_identifier!(
    /// Stable retrospective rule-finding identifier.
    FindingId
);
text_identifier!(
    /// Stable manual-accusation attempt identifier.
    AccusationId
);
text_identifier!(
    /// Stable identifier for one exact-target graphical capture request.
    CaptureRequestId
);
text_identifier!(
    /// Content-addressed capture artifact identifier.
    CaptureArtifactId
);
text_identifier!(
    /// Identifier for one bounded private artifact transfer.
    CaptureTransferId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_ambiguous_text() {
        assert!(RoomId::new("room-01").is_ok());
        assert!(RoomId::new("").is_err());
        assert!(RoomId::new("room/other").is_err());
        assert!(RoomId::new("x".repeat(65)).is_err());
    }
}
