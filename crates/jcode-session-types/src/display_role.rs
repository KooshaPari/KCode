//! Display-role tags stored alongside session messages.
//!
//! Kept in its own module so `lib.rs` stays inside the code-size budget while
//! the enum and its serde impls live next to each other. Tests for these
//! round-trips live in `lib.rs` beside the other session-serialisation tests.

use serde::{Deserialize, Serialize};

/// How a stored message should be displayed. Unknown values are preserved
/// verbatim so a newer JCode session can be loaded and saved by an older
/// KCode without losing its original display marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredDisplayRole {
    System,
    BackgroundTask,
    /// Informational file-change marker authored by JCode. It is not a user
    /// turn or a background task; render it like a system note.
    FileActivity,
    /// A display role from a newer client, retained byte-for-byte.
    Other(String),
}

impl<'de> Deserialize<'de> for StoredDisplayRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "system" => Self::System,
            "background_task" => Self::BackgroundTask,
            "file_activity" => Self::FileActivity,
            _ => Self::Other(raw),
        })
    }
}

impl Serialize for StoredDisplayRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::System => serializer.serialize_str("system"),
            Self::BackgroundTask => serializer.serialize_str("background_task"),
            Self::FileActivity => serializer.serialize_str("file_activity"),
            Self::Other(raw) => serializer.serialize_str(raw),
        }
    }
}
