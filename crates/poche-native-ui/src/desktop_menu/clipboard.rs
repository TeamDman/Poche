//! Production invitation buttons use the OS clipboard. Input acceptance may
//! explicitly replace only those buttons' backend with a shared private buffer;
//! it must not read, persist or overwrite arbitrary user clipboard contents.
use bevy::{
    clipboard::{Clipboard, ClipboardError, ClipboardRead},
    prelude::Resource,
};

#[derive(Resource, Default)]
pub(super) enum InvitationClipboard {
    #[default]
    System,
    #[cfg(feature = "input-probe")]
    Isolated(std::sync::Arc<std::sync::Mutex<String>>),
}

impl InvitationClipboard {
    pub(super) fn fetch_text(&mut self, system: &mut Clipboard) -> ClipboardRead {
        match self {
            Self::System => system.fetch_text(),
            #[cfg(feature = "input-probe")]
            Self::Isolated(text) => {
                ClipboardRead::Ready(Ok(text.lock().expect("isolated clipboard").clone()))
            }
        }
    }

    pub(super) fn set_text(
        &mut self,
        system: &mut Clipboard,
        value: String,
    ) -> Result<(), ClipboardError> {
        match self {
            Self::System => system.set_text(value),
            #[cfg(feature = "input-probe")]
            Self::Isolated(text) => {
                *text.lock().expect("isolated clipboard") = value;
                Ok(())
            }
        }
    }
}
