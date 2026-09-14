//! Shared types for the inline swarm gallery.

/// A renderer-agnostic view of one swarm member, ready for layout.
///
/// Callers are responsible for building the `body` lines (e.g. choosing live
/// output tail vs. status detail); everything else about how the tile looks is
/// handled here.
#[derive(Clone, Debug)]
pub struct GalleryMember {
    /// Display title (friendly name or short id).
    pub label: String,
    /// Optional session icon (emoji) shown in place of the name on the
    /// vertical strip, e.g. "🦊" for a session named "fox".
    pub icon: Option<String>,
    /// Lifecycle status string (drives the badge text and accent color).
    pub status: String,
    /// Short label of the task this member was spawned/assigned for.
    pub task: Option<String>,
    /// Swarm role, if any (drives the title glyph and sort order).
    pub role: Option<String>,
    /// Pre-rendered body lines shown inside the tile.
    pub body: Vec<String>,
    /// Stable tiebreaker for sorting members with equal role rank.
    pub sort_key: String,
    /// Optional todo progress as (completed, total).
    pub todo: Option<(u32, u32)>,
    /// Compact todo entries for the focused detail view.
    pub todo_items: Vec<GalleryTodo>,
    /// Provider model currently running this member, when known.
    pub model: Option<String>,
    /// Provider display name and credential route used by this member.
    pub provider: Option<String>,
    pub auth_method: Option<String>,
    /// Reasoning effort selected for this member.
    pub effort: Option<String>,
    /// Seconds since this member was spawned.
    pub elapsed_secs: Option<u64>,
}

/// One compact todo entry shown in the focused swarm detail view.
#[derive(Clone, Debug)]
pub struct GalleryTodo {
    pub content: String,
    /// "pending", "in_progress", or "completed".
    pub status: String,
    /// Up to three recent tool calls made while this item was active.
    pub tool_intents: Vec<GalleryToolIntent>,
}

#[derive(Clone, Debug)]
pub struct GalleryToolIntent {
    pub tool_name: String,
    pub intent: String,
    /// "running", "completed", or "error".
    pub status: String,
    /// Optional live progress as (current, total, unit).
    pub progress: Option<(u64, u64, Option<String>)>,
}

/// A key/label pair for the swarm strip hint line.
pub struct SwarmStripHint {
    /// The key chord to show, e.g. "alt+n" or "j/k".
    pub key: String,
    /// What it does, e.g. "select".
    pub label: String,
}
