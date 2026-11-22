use super::*;
use crate::tui::backend::ssh_gate::LocalSshGateError;

fn show_pause(app: &mut App, reason: &LocalSshGateError) {
    app.push_display_message(DisplayMessage::error(format!(
        "SSH reconnect paused: {reason}. No further requests will be sent on this connection. An earlier in-flight action may have happened; reconcile it before retrying. Uncertain requests are retained in memory only and are not resent automatically. Ctrl+R restores the latest ordinary request and images to an empty draft; hidden/system requests cannot be restored this way. Verify the target, then exit and reconnect explicitly."
    )));
}

fn block_submit(
    app: &mut App,
    event: &Option<std::result::Result<crossterm::event::Event, std::io::Error>>,
) -> bool {
    if matches!(event, Some(Ok(crossterm::event::Event::Key(key))) if key.code == crossterm::event::KeyCode::Enter)
    {
        app.set_status_notice(
            "SSH target requires explicit reconnection; draft was not sent or queued",
        );
        true
    } else {
        false
    }
}

pub(in crate::tui::app) async fn pause(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    event_stream: &mut EventStream,
    state: &mut RemoteRunState,
    reason: &LocalSshGateError,
) -> Result<ConnectOutcome> {
    show_pause(app, reason);
    let mut redraw = disconnected_redraw_interval(false);
    loop {
        tokio::select! {
            _ = redraw.tick() => { terminal.draw(|frame| crate::tui::ui::draw(frame, app))?; }
            event = event_stream.next() => {
                if let Some(Ok(crossterm::event::Event::Key(key))) = &event {
                    if super::uncertain::handle_recovery_key(app, state, key) { continue; }
                }
                if block_submit(app, &event) { continue; }
                if handle_terminal_event_while_disconnected(app, terminal, event)? {
                    return Ok(ConnectOutcome::Quit);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn paused_submit_keeps_draft_attachments_and_queue() {
        let mut app = App::new_for_remote(Some("session_one".into()));
        app.input = "draft".into();
        app.pending_images = vec![("image/png".into(), "synthetic-image".into())];
        app.queued_messages = vec!["queued".into()];
        show_pause(&mut app, &LocalSshGateError::TargetChanged);
        let enter = Some(Ok(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert!(block_submit(&mut app, &enter));
        assert_eq!(app.input, "draft");
        assert_eq!(
            app.pending_images,
            vec![("image/png".into(), "synthetic-image".into())]
        );
        assert_eq!(app.queued_messages, vec!["queued"]);
    }
}
