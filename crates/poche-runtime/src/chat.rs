use std::collections::VecDeque;

use poche_protocol::PrincipalId;
use serde::{Deserialize, Serialize};

/// Default number of public chat entries retained only in process memory.
pub const DEFAULT_CHAT_TAIL_CAPACITY: usize = 64;

/// Public attributed chat item produced only by an accepted reducer event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatEntry {
    pub revision: u64,
    pub principal_id: PrincipalId,
    pub text: String,
}

/// Fixed-capacity in-memory tail. It has no persistence or authority behavior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatTail {
    capacity: usize,
    entries: VecDeque<ChatEntry>,
}

impl ChatTail {
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, entry: ChatEntry) {
        if self.capacity == 0 {
            return;
        }
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    #[must_use]
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &ChatEntry> {
        self.entries.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Export exactly the retained public tail as canonical NDJSON.
    ///
    /// # Errors
    ///
    /// Returns a serialization failure.
    pub fn export_ndjson(&self) -> Result<String, serde_json::Error> {
        let mut output = String::new();
        for entry in &self.entries {
            output.push_str(&serde_json::to_string(entry)?);
            output.push('\n');
        }
        Ok(output)
    }
}

impl Default for ChatTail {
    fn default() -> Self {
        Self::new(DEFAULT_CHAT_TAIL_CAPACITY)
    }
}
