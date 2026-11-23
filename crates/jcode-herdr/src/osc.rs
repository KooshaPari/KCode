//! OSC 7501 status reports for herdr-core (HC).
//!
//! herdr-core has no report verb. A program inside an HC terminal tells the
//! core what it is doing by printing an OSC 7501 record on its own output:
//!
//! ```text
//! ESC ] 7 5 0 1 ; f i e l d s ESC \
//! ```
//!
//! `fields` is a `:`-separated list. The first field is `state=`, one of
//! `idle`, `working`, `done`, `blocked`, `error`, or `clear`. Optional fields
//! are `id=`, `kind=`, `progress=`, `app=`, `title=` and `msg=`. `title` and
//! `msg` are base64 because a record may not contain raw control bytes; the
//! core decodes them. A record with no `id` replaces the terminal's root
//! status, and a record carrying an `id` is kept as a child record under it.
//!
//! The manual is explicit that the words are the program's own and nothing is
//! translated, and that a plain program reports none: printing nothing leaves
//! the terminal unreported rather than guessing.
//!
//! Wire format measured against herdr-core 0.1.22 on 2026-11-23: every field
//! below round-tripped through `hc get --json`, with both `ESC \` and `BEL` as
//! terminators.

use std::io::Write;

/// Environment variable HC sets in every terminal it runs. Its presence means
/// this process is talking to a core that reads OSC 7501.
pub const HC_TERMINAL_ID: &str = "HC_TERMINAL_ID";

/// `ESC ]` + the status code + `;` — everything before the first field.
const OSC_STATUS_PREFIX: &str = "\u{1b}]7501;";

/// String Terminator. `BEL` is also accepted by the core, but `ESC \` is what
/// the core itself emits when it replays a record to a new viewer.
const OSC_STRING_TERMINATOR: &str = "\u{1b}\\";

/// Field separator inside a record.
const FIELD_SEPARATOR: char = ':';

/// A lifecycle word, as the core's own vocabulary spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HcState {
    /// Ready for input.
    Idle,
    /// Actively working on a turn.
    Working,
    /// A turn ended successfully.
    Done,
    /// A person is needed: a permission prompt or a question.
    Blocked,
    /// The turn failed.
    Error,
}

impl HcState {
    /// The word as it goes on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Error => "error",
        }
    }
}

/// Why a program is blocked, when it knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HcKind {
    /// A permission prompt is on screen.
    Permission,
    /// A question is on screen.
    Question,
    /// A login or credential prompt is on screen.
    Auth,
}

impl HcKind {
    /// The word as it goes on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Question => "question",
            Self::Auth => "auth",
        }
    }
}

/// One status record, built up field by field. Only `state` is required.
#[derive(Debug, Clone, Default)]
pub struct Record {
    kind: Option<HcKind>,
    progress: Option<u8>,
    app: Option<String>,
    title: Option<String>,
    msg: Option<String>,
}

impl Record {
    /// A record carrying nothing but its state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Say why a blocked program is blocked.
    pub fn kind(mut self, kind: HcKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Report completion percentage, 0..100. Clamped.
    pub fn progress(mut self, percent: u8) -> Self {
        self.progress = Some(percent.min(100));
        self
    }

    /// Name the program behind the record.
    pub fn app(mut self, app: impl Into<String>) -> Self {
        self.app = Some(app.into());
        self
    }

    /// Set the terminal title the core shows for this record.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the free-text message the core shows for this record.
    pub fn msg(mut self, msg: impl Into<String>) -> Self {
        self.msg = Some(msg.into());
        self
    }
}

/// Encode a record as the escape sequence a terminal consumes.
///
/// `id` names a child record; `None` replaces the terminal's root status, which
/// is what an agent's own state should do.
pub fn encode(state: HcState, id: Option<&str>, record: &Record) -> String {
    let mut fields = vec![format!("state={}", state.as_str())];
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        fields.push(format!("id={}", sanitize(id)));
    }
    if let Some(kind) = record.kind {
        fields.push(format!("kind={}", kind.as_str()));
    }
    if let Some(progress) = record.progress {
        fields.push(format!("progress={progress}"));
    }
    if let Some(app) = record.app.as_deref().filter(|app| !app.is_empty()) {
        fields.push(format!("app={}", sanitize(app)));
    }
    // A record may not carry raw control bytes; base64 keeps any title or
    // message safe, including newlines and other people's escape sequences.
    if let Some(title) = record.title.as_deref().filter(|t| !t.is_empty()) {
        fields.push(format!("title={}", base64(title.as_bytes())));
    }
    if let Some(msg) = record.msg.as_deref().filter(|m| !m.is_empty()) {
        fields.push(format!("msg={}", base64(msg.as_bytes())));
    }
    format!(
        "{OSC_STATUS_PREFIX}{}{OSC_STRING_TERMINATOR}",
        fields.join(&FIELD_SEPARATOR.to_string())
    )
}

/// Encode the record that removes this terminal's status, or one child
/// record and everything under it.
pub fn clear(id: Option<&str>) -> String {
    match id.filter(|id| !id.is_empty()) {
        Some(id) => format!(
            "{OSC_STATUS_PREFIX}state=clear:id={}{OSC_STRING_TERMINATOR}",
            sanitize(id)
        ),
        None => format!("{OSC_STATUS_PREFIX}state=clear{OSC_STRING_TERMINATOR}"),
    }
}

/// `true` when this process runs inside a herdr-core terminal.
pub fn active() -> bool {
    std::env::var_os(HC_TERMINAL_ID).is_some_and(|value| !value.is_empty())
}

/// Write one record to standard output.
///
/// The whole record goes out in a single write so a concurrent writer cannot
/// land inside the escape sequence. Returns `false` when this process is not
/// inside herdr-core, so callers stay silent in a plain terminal.
pub fn emit(document: &str) -> bool {
    if !active() {
        return false;
    }
    write_record(document)
}

/// Write one record without consulting the environment. Used by callers that
/// already know the runtime, and by tests.
pub fn write_record(document: &str) -> bool {
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(document.as_bytes()).is_err() {
        return false;
    }
    stdout.flush().is_ok()
}

/// Drop anything that would end the record early or break the field syntax.
fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() && *c != FIELD_SEPARATOR && *c != '\u{1b}')
        .collect()
}

/// Standard base64 with padding: the alphabet the core decodes `title` and
/// `msg` with.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_state_record_has_no_optional_fields() {
        assert_eq!(
            encode(HcState::Working, None, &Record::new()),
            "\u{1b}]7501;state=working\u{1b}\\"
        );
    }

    #[test]
    fn every_word_is_the_cores_own() {
        let words: Vec<&str> = [
            HcState::Idle,
            HcState::Working,
            HcState::Done,
            HcState::Blocked,
            HcState::Error,
        ]
        .iter()
        .map(|state| state.as_str())
        .collect();
        assert_eq!(words, ["idle", "working", "done", "blocked", "error"]);
    }

    #[test]
    fn fields_are_colon_separated_after_state() {
        let record = Record::new()
            .kind(HcKind::Permission)
            .progress(42)
            .app("kcode");
        assert_eq!(
            encode(HcState::Blocked, None, &record),
            "\u{1b}]7501;state=blocked:kind=permission:progress=42:app=kcode\u{1b}\\"
        );
    }

    #[test]
    fn id_marks_a_child_record() {
        assert_eq!(
            encode(HcState::Working, Some("t7/worker"), &Record::new()),
            "\u{1b}]7501;state=working:id=t7/worker\u{1b}\\"
        );
    }

    #[test]
    fn empty_id_is_the_root_record() {
        assert_eq!(
            encode(HcState::Idle, Some(""), &Record::new()),
            "\u{1b}]7501;state=idle\u{1b}\\"
        );
    }

    #[test]
    fn title_and_msg_are_base64() {
        let record = Record::new().title("hello").msg("hi\n");
        assert_eq!(
            encode(HcState::Done, None, &record),
            "\u{1b}]7501;state=done:title=aGVsbG8=:msg=aGkK\u{1b}\\"
        );
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn control_bytes_cannot_end_the_record_early() {
        let record = Record::new().app("bad\u{1b}]7501;state=idle\u{7}app");
        let encoded = encode(HcState::Working, None, &record);
        assert_eq!(encoded.matches('\u{1b}').count(), 2, "{encoded:?}");
        assert!(!encoded.contains('\u{7}'));
    }

    #[test]
    fn field_separator_is_stripped_from_values() {
        let record = Record::new().app("a:b");
        assert_eq!(
            encode(HcState::Working, None, &record),
            "\u{1b}]7501;state=working:app=ab\u{1b}\\"
        );
    }

    #[test]
    fn progress_is_clamped_to_a_percentage() {
        let record = Record::new().progress(250);
        assert!(encode(HcState::Working, None, &record).contains("progress=100"));
    }

    #[test]
    fn clear_removes_the_root_or_one_child() {
        assert_eq!(clear(None), "\u{1b}]7501;state=clear\u{1b}\\");
        assert_eq!(
            clear(Some("t7/worker")),
            "\u{1b}]7501;state=clear:id=t7/worker\u{1b}\\"
        );
    }

    #[test]
    fn emit_is_silent_when_not_inside_herdr_core() {
        if std::env::var_os(HC_TERMINAL_ID).is_some() {
            // The test itself runs inside HC: nothing to assert about absence.
            return;
        }
        assert!(!active());
        assert!(!emit("\u{1b}]7501;state=idle\u{1b}\\"));
    }
}
