//! State-space nodes for the UI.
//!
//! `build_scene` is a pure function of `Model`, so the app's visual states
//! form an enumerable graph. Each named node here is a deterministic `Model`
//! that can be rendered offscreen (`--capture <node> <out.png>`) for visual
//! verification without a window, compositor, or screenshots.

use crate::Model;

type NodeBuilder = fn() -> Model;

/// All named state-space nodes. Keep deterministic: no clocks, no randomness.
pub const NODES: &[(&str, NodeBuilder)] = &[
    ("connecting", connecting),
    ("attached_empty", attached_empty),
    ("mid_input", mid_input),
    ("mid_input_caret_inside", mid_input_caret_inside),
    ("caret_hidden", caret_hidden),
    ("selection", selection),
    ("multiline", multiline),
    ("wrapped_long_line", wrapped_long_line),
    ("multiline_selection", multiline_selection),
    ("selection_all", selection_all),
    ("streaming", streaming),
    ("streaming_reveal", streaming_reveal),
    ("turn_done", turn_done),
    ("transcript_selection", transcript_selection),
    ("scrolled_back", scrolled_back),
    ("markdown", markdown),
    ("latex", latex),
    ("code_block", code_block),
    ("notice", notice),
    ("error", error),
    ("long_paragraph", long_paragraph),
    // Heavy nodes. Every node above is a small, pretty screen, which is what a
    // capture wants and exactly the wrong thing to profile: a sweep over them
    // would have reported the whole app as fast while a real session lagged.
    // These sit at the slow end of the space on purpose, so `--profile-states`
    // measures the frames that actually hurt.
    ("heavy_long_session", heavy_long_session),
    ("heavy_code_wall", heavy_code_wall),
    ("heavy_wide_table", heavy_wide_table),
    ("heavy_math", heavy_math),
];

pub fn by_name(name: &str) -> Option<Model> {
    NODES
        .iter()
        .find(|(node, _)| *node == name)
        .map(|(_, build)| build())
}

pub fn names() -> Vec<&'static str> {
    NODES.iter().map(|(name, _)| *name).collect()
}

/// Captures must be deterministic, so nodes pin the build identity instead of
/// reading the real version, update channels, and auth store.
fn fixed_meta() -> crate::meta::Meta {
    crate::meta::Meta {
        version: "v0.0.0-demo (0000000)".into(),
        update: crate::meta::UpdateState::Current,
        account: Some("demo@jcode.dev (anthropic)".into()),
    }
}

fn connecting() -> Model {
    Model {
        theme: crate::theme::Theme::from_env(),
        meta: fixed_meta(),
        status: "connecting to ~/.jcode/jcode-api.sock...".into(),
        session_id: None,
        transcript: crate::transcript::Transcript::default(),
        editor: crate::editor::Editor::default(),
        caret: fixed_caret(),
        busy: false,
        scroll: 0.0,
        selection: None,
        notice: None,
        reveal: crate::stream::Reveal::default(),
    }
}

/// Captures must be a pure function of the model, so nodes pin the caret
/// instead of letting it blink on wall-clock time.
fn fixed_caret() -> crate::caret::Caret {
    crate::caret::Caret::pinned(true)
}

fn attached_empty() -> Model {
    Model {
        theme: crate::theme::Theme::from_env(),
        meta: fixed_meta(),
        status: "attached: session_demo_0000".into(),
        session_id: Some("session_demo_0000".into()),
        transcript: crate::transcript::Transcript::default(),
        editor: crate::editor::Editor::default(),
        caret: fixed_caret(),
        busy: false,
        scroll: 0.0,
        selection: None,
        notice: None,
        reveal: crate::stream::Reveal::default(),
    }
}

/// Build a transcript from (user, assistant) turns. Fixtures speak in turns
/// rather than in a formatted blob, so a capture exercises the real role
/// structure the renderer draws.
fn conversation(turns: Vec<(String, String)>) -> crate::transcript::Transcript {
    use crate::transcript::{Message, Transcript};
    let mut transcript = Transcript::default();
    for (user, assistant) in turns {
        transcript.push(Message::user(user));
        transcript.push(Message::assistant(assistant));
    }
    transcript
}

fn editor_with(text: &str, cursor: Option<usize>) -> crate::editor::Editor {
    let mut editor = crate::editor::Editor::default();
    editor.insert_str(text);
    if let Some(cursor) = cursor {
        editor.set_cursor_public(cursor);
    }
    editor
}

fn mid_input() -> Model {
    Model {
        editor: editor_with("explain the harness API handshake", None),
        ..attached_empty()
    }
}

/// Caret parked mid-text: proves the input box is a real buffer with a cursor
/// rather than an append-only string.
fn mid_input_caret_inside() -> Model {
    Model {
        editor: editor_with("explain the harness API handshake", Some(7)),
        ..attached_empty()
    }
}

/// The off phase of the blink, so the caret's absence is also a tested state.
fn caret_hidden() -> Model {
    Model {
        editor: editor_with("blink off phase", None),
        caret: crate::caret::Caret::pinned(false),
        ..attached_empty()
    }
}

/// A mouse or shift-arrow selection: proves the band renders and that text on
/// top of it stays readable.
fn selection() -> Model {
    let mut editor = editor_with("select this middle part", None);
    editor.place_cursor(7);
    editor.extend_to(11);
    Model {
        editor,
        ..attached_empty()
    }
}

fn selection_all() -> Model {
    let mut editor = editor_with("everything is selected", None);
    editor.select_all();
    Model {
        editor,
        ..attached_empty()
    }
}

/// A multi-line message: the composer grows and the caret sits on the last
/// line, not the first.
fn multiline() -> Model {
    let mut editor = crate::editor::Editor::default();
    editor.insert_str("first line\nsecond line\nthird line");
    Model {
        editor,
        ..attached_empty()
    }
}

/// One very long logical line: must wrap inside the well rather than running
/// past its right edge.
fn wrapped_long_line() -> Model {
    let mut editor = crate::editor::Editor::default();
    editor.insert_str(
        "this is a single very long line with no newlines at all that has to wrap \
         inside the composer well instead of spilling past its right edge",
    );
    Model {
        editor,
        ..attached_empty()
    }
}

/// A selection spanning a line break.
fn multiline_selection() -> Model {
    let mut editor = crate::editor::Editor::default();
    editor.insert_str("alpha beta\ngamma delta");
    editor.place_cursor(6);
    editor.extend_to(16);
    Model {
        editor,
        ..attached_empty()
    }
}

fn scrolled_back() -> Model {
    Model {
        transcript: conversation(
            (1..=20)
                .map(|n| {
                    (
                        format!("question {n}"),
                        format!("answer {n}. transcript line {n}"),
                    )
                })
                .collect(),
        ),
        scroll: 200.0,
        ..attached_empty()
    }
}

fn notice() -> Model {
    Model {
        editor: editor_with("undo me", None),
        notice: Some("nothing to undo".into()),
        ..attached_empty()
    }
}

fn streaming() -> Model {
    Model {
        transcript: conversation(vec![(
            "explain the harness API handshake".into(),
            "The client opens the socket and sends a `hello` frame carrying \
             its supported version range. The server replies with `hello_ok` \
             and the negotiated version, after which"
                .into(),
        )]),
        busy: true,
        ..attached_empty()
    }
}

/// Mid-animation streaming frame: the newest text is still fading and rising
/// into place. Pinned to a fixed point in the fade so the capture is
/// reproducible.
fn streaming_reveal() -> Model {
    let mut model = streaming();
    let start = std::time::Instant::now();
    let mut reveal = crate::stream::Reveal::default();
    // The last sentence is treated as the freshly arrived chunk.
    let tail = 90.min(model.transcript.len());
    reveal.push_at(model.transcript.len() - tail, start);
    reveal.push_at(tail, start + crate::stream::FADE);
    reveal.freeze_at(start + crate::stream::FADE + crate::stream::FADE / 3);
    model.reveal = reveal;
    model
}

fn turn_done() -> Model {
    Model {
        transcript: conversation(vec![(
            "explain the harness API handshake".into(),
            "The client opens the socket and sends a `hello` frame carrying \
             its supported version range. The server replies with `hello_ok` \
             and the negotiated version, after which normal requests flow."
                .into(),
        )]),
        busy: false,
        ..attached_empty()
    }
}

/// A transcript selection spanning both turns: the highlight has to band the
/// tail of the question, all of the gap between, and the head of the reply.
/// Rendered as a node so the bands can be reviewed and pixel-tested without a
/// window, which is the only way to see that they line up with the glyphs.
fn transcript_selection() -> Model {
    let done = turn_done();
    Model {
        selection: Some(crate::select::Selection::new(
            crate::select::Position {
                message: 0,
                block: 0,
                offset: 8,
            },
            crate::select::Position {
                message: 1,
                block: 0,
                offset: 40,
            },
        )),
        ..done
    }
}

/// Markdown a model actually emits: headings, emphasis, inline code, lists,
/// a quote, and a table. Proves the transcript renders structure rather than
/// echoing punctuation.
fn markdown() -> Model {
    Model {
        transcript: conversation(vec![(
            "summarise the transport".into(),
            "## Transport\n\nThe protocol is **line-delimited JSON** over a \
             *Unix socket*, framed by `\\n`.\n\n\
             - `hello` negotiates the version\n\
             - `subscribe` attaches to a session\n\n\
             > Framing is unchanged across transports.\n\n\
             | frame | direction |\n|---|---|\n| hello | client |\n| hello_ok | server |\n"
                .into(),
        )]),
        ..attached_empty()
    }
}

/// Inline and display math. The transcript must render these as math, not
/// print the LaTeX source at the user.
fn latex() -> Model {
    Model {
        transcript: conversation(vec![(
            "what is the cost".into(),
            "The march is $O(n^2)$ per frame, with $n$ the grid side.\n\n\
             $$\\frac{a + b}{c}$$\n\n\
             So halving $n$ quarters the work."
                .into(),
        )]),
        ..attached_empty()
    }
}

/// A fenced code block: it must read as a quoted artefact on its own wash,
/// not as more prose.
fn code_block() -> Model {
    Model {
        transcript: conversation(vec![(
            "show me the handler".into(),
            "Here is the entry point:\n\n```rust\nfn main() -> Result<()> {\n    \
             App::default().run()\n}\n```\n\nIt returns on the first error."
                .into(),
        )]),
        ..attached_empty()
    }
}

fn error() -> Model {
    Model {
        status: "disconnected: daemon connection closed".into(),
        ..turn_done()
    }
}

/// One very long unwrapped paragraph: the transcript must stay inside its own
/// region instead of running down over the composer.
fn long_paragraph() -> Model {
    Model {
        transcript: conversation(vec![(
            "explain everything".into(),
            "the client opens the socket and sends a hello frame carrying its supported version range. "
                .repeat(24),
        )]),
        ..attached_empty()
    }
}

/// A realistic long session: the shape that made the window feel laggy, and
/// the shape no other node covers. Sixty turns is an afternoon of work, not a
/// pathological input.
fn heavy_long_session() -> Model {
    let turns = (0..60)
        .map(|n| {
            (
                format!("question {n} about the transport layer"),
                format!(
                    "answer {n}. {}",
                    "the client opens the socket and sends a hello frame carrying its \
                     supported version range. "
                        .repeat(3)
                ),
            )
        })
        .collect();
    Model {
        transcript: conversation(turns),
        ..attached_empty()
    }
}

/// A reply that is mostly code. Code blocks carry their own wash, inset, and
/// padding, so they cost more per line than prose and are worth measuring
/// separately.
fn heavy_code_wall() -> Model {
    let code = (0..120)
        .map(|n| format!("    let value_{n} = compute(input[{n}], &config, depth + {n});"))
        .collect::<Vec<_>>()
        .join("\n");
    Model {
        transcript: conversation(vec![(
            "show me the whole function".into(),
            format!("Here it is:\n\n```rust\nfn main() {{\n{code}\n}}\n```\n"),
        )]),
        ..attached_empty()
    }
}

/// A wide table. Column widths are measured per cell by the desktop's own
/// table adapter, so this exercises a path prose never touches.
fn heavy_wide_table() -> Model {
    let header = "| frame | direction | payload | notes | since |";
    let rule = "|---|---|---|---|---|";
    let rows = (0..40)
        .map(|n| format!("| frame_{n} | client | {{\"id\": {n}}} | row {n} notes | v0.{n} |"))
        .collect::<Vec<_>>()
        .join("\n");
    Model {
        transcript: conversation(vec![(
            "list every frame".into(),
            format!("{header}\n{rule}\n{rows}\n"),
        )]),
        ..attached_empty()
    }
}

/// Math-heavy output. LaTeX goes through render-core's math translation before
/// it is ever laid out, so a reply full of it is a different cost profile
/// again.
fn heavy_math() -> Model {
    let body = (0..30)
        .map(|n| format!("The bound $x_{{{n}}}^2 + y_{{{n}}}^2 \\leq z_{{{n}}}$ holds.\n\n$$\\frac{{a_{{{n}}}}}{{b_{{{n}}}}} = \\sum_{{i=0}}^{{{n}}} c_i$$"))
        .collect::<Vec<_>>()
        .join("\n\n");
    Model {
        transcript: conversation(vec![("derive the bounds".into(), body)]),
        ..attached_empty()
    }
}
