//! The one clipboard write.
//!
//! Both the window and the tray reach the clipboard through
//! [`crate::commands::clips::copy`], and that function writes through this
//! trait. The trait exists for one reason: the copy path is the hottest and most
//! ordering-sensitive path in the application — clipboard first, then the
//! committed increment (spec §4.1, contract `copy_clip`) — and a test cannot
//! assert that ordering against a real system clipboard without either a desktop
//! session or leaving the user's clipboard holding a test fixture.

use tauri::{AppHandle, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::error::ClipError;

/// Writes text to the system clipboard.
pub trait ClipboardWriter {
    /// A failure is `clipboard`, and `use_count` is not incremented.
    ///
    /// The text is never logged, at any level: it is a clip `value`
    /// (ADR-0002).
    fn write_text(&self, text: &str) -> Result<(), ClipError>;
}

/// The real clipboard, reached through the Tauri plugin.
///
/// Holds the handle rather than the plugin's state, so that constructing one
/// touches nothing. `ClipboardExt::clipboard` panics if the plugin is not
/// registered, and a command that failed validation must not be the call that
/// discovers it.
pub struct AppClipboard<'a, R: Runtime>(pub &'a AppHandle<R>);

impl<R: Runtime> ClipboardWriter for AppClipboard<'_, R> {
    fn write_text(&self, text: &str) -> Result<(), ClipError> {
        match self.0.clipboard().write_text(text) {
            Ok(()) => Ok(()),
            Err(error) => {
                // The plugin's error carries a description of the platform
                // failure, never the text it was asked to write.
                log::error!("the clipboard could not be written: {error}");
                Err(ClipError::Clipboard)
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::sync::Mutex;

    use super::*;
    use crate::storage::lock_recovering;

    /// A clipboard that records what it was asked to write, and can be told to
    /// fail.
    pub(crate) struct FakeClipboard {
        written: Mutex<Vec<String>>,
        fails: bool,
    }

    impl FakeClipboard {
        pub(crate) fn working() -> Self {
            Self {
                written: Mutex::new(Vec::new()),
                fails: false,
            }
        }

        pub(crate) fn failing() -> Self {
            Self {
                written: Mutex::new(Vec::new()),
                fails: true,
            }
        }

        pub(crate) fn written(&self) -> Vec<String> {
            lock_recovering(&self.written).clone()
        }
    }

    impl ClipboardWriter for FakeClipboard {
        fn write_text(&self, text: &str) -> Result<(), ClipError> {
            if self.fails {
                return Err(ClipError::Clipboard);
            }
            lock_recovering(&self.written).push(text.to_owned());
            Ok(())
        }
    }
}
