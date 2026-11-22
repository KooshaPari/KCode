/// Forward the reasoning-effort variant staged by a model-picker selection
/// (e.g. "gpt-5.5 (high)") to the server right after the model-switch request.
/// In remote mode the picker cannot apply effort to `app.provider` (a local
/// stand-in), so skipping this leaves the server on its configured default
/// effort - typically low - silently downgrading the request (issue #427).
async fn forward_pending_reasoning_effort(app: &mut App, remote: &mut RemoteConnection) {
    let Some(effort) = app.pending_reasoning_effort.take() else {
        return;
    };
    match remote.set_reasoning_effort(&effort).await {
        Ok(()) => {
            // Optimistically track the requested effort so the widget/header
            // reflect the picker choice; ReasoningEffortChanged confirms it.
            app.remote_reasoning_effort = Some(effort);
        }
        Err(error) => {
            app.push_display_message(DisplayMessage::error(format!(
                "Failed to request reasoning effort '{}': {}",
                effort, error
            )));
            app.set_status_notice("Effort switch failed");
        }
    }
}

pub(super) async fn handle_terminal_event(
    app: &mut App,
    remote: &mut RemoteConnection,
    state: &mut RemoteRunState,
    event: Option<std::result::Result<Event, std::io::Error>>,
) -> Result<bool> {
    let mut needs_redraw = apply_terminal_event(app, remote, state, event).await?;
    // Coalesce bursts of already-buffered input (fast typing, key repeat,
    // scroll wheels) into a single frame instead of paying one full render per
    // event. Without this, typing faster than the frame rate queues events and
    // each one costs handle + full draw serially, which reads as input-line
    // lag. Mirrors the identical drain in `local::handle_terminal_event`.
    const MAX_DRAINED_EVENTS_PER_WAKE: usize = 32;
    for _ in 0..MAX_DRAINED_EVENTS_PER_WAKE {
        if !crossterm::event::poll(std::time::Duration::ZERO).unwrap_or(false) {
            break;
        }
        if let Ok(event) = crossterm::event::read() {
            needs_redraw |= apply_terminal_event(app, remote, state, Some(Ok(event))).await?;
        }
    }
    Ok(needs_redraw)
}

async fn apply_terminal_event(
    app: &mut App,
    remote: &mut RemoteConnection,
    state: &mut RemoteRunState,
    event: Option<std::result::Result<Event, std::io::Error>>,
) -> Result<bool> {
    if let Some(Ok(Event::Key(key))) = &event {
        if reconnect::handle_recovery_key(app, state, key) {
            return Ok(true);
        }
    }
    if reconnect::has_unresolved_for_session(app, state)
        && matches!(&event, Some(Ok(Event::Key(key))) if app.toggle_keys.auto_poke.matches(key.code, key.modifiers))
    {
        app.set_status_notice("Accept the unknown outcome with Ctrl+E before sending");
        return Ok(true);
    }
    let mut needs_redraw = false;
    let mut input_attribution = crate::tui::ui::FrameInputAttribution {
        event: None,
        scroll_delta: None,
        model_picker_open: app
            .inline_interactive_state
            .as_ref()
            .is_some_and(|state| state.kind == crate::tui::PickerKind::Model),
    };
    match event {
        Some(Ok(Event::FocusGained)) => {
            crate::tui::reapply_configured_terminal_modes_after_focus();
            input_attribution.event = Some("focus_gained".to_string());
            needs_redraw |= app.set_client_focused(true);
            app.note_client_focus(true);
        }
        Some(Ok(Event::FocusLost)) => {
            input_attribution.event = Some("focus_lost".to_string());
            app.set_client_focused(false);
        }
        Some(Ok(Event::Key(key))) => {
            // Start the key-to-paint clock at the moment the key is read, which is
            // the only point that corresponds to the user's press.
            crate::tui::ui::note_key_event_read();
            input_attribution.event = Some(if app.remote_login.is_some() {
                "ssh_login_key".to_string()
            } else {
                format!("key:{:?}:{:?}", key.code, key.kind)
            });
            input_attribution.scroll_delta = key_scroll_delta(&key);
            app.note_client_interaction();
            app.update_copy_badge_key_event(key);
            app.observe_voice_key_release(&key);
            if app.handle_voice_key_event(&key) {
                // Voice keys work from every screen and never type.
            } else if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                handle_remote_key_event(app, key, remote).await?;
                if let Some(selection) = app.pending_route_selection.take() {
                    app.pending_model_switch = None;
                    match remote.set_route_selection(selection).await {
                        Ok(_) => {
                            app.remote_model_switch_in_flight = true;
                            forward_pending_reasoning_effort(app, remote).await;
                        }
                        Err(error) => {
                            app.pending_reasoning_effort = None;
                            // A fallback-offer resend must not fire without its
                            // route switch; drop it with the failed request.
                            app.pending_fallback_resend = None;
                            app.push_display_message(DisplayMessage::error(format!(
                                "Failed to request model switch: {}",
                                error
                            )));
                            app.set_status_notice("Model switch failed");
                        }
                    }
                } else if let Some(spec) = app.pending_model_switch.take() {
                    match remote.set_model(&spec).await {
                        Ok(_) => {
                            app.remote_model_switch_in_flight = true;
                            forward_pending_reasoning_effort(app, remote).await;
                        }
                        Err(error) => {
                            app.pending_reasoning_effort = None;
                            app.push_display_message(DisplayMessage::error(format!(
                                "Failed to request model switch: {}",
                                error
                            )));
                            app.set_status_notice("Model switch failed");
                        }
                    }
                }
                if let Some(selection) = app.pending_account_picker_action.take() {
                    match selection {
                        crate::tui::AccountPickerAction::Switch { provider_id, label } => {
                            match provider_id.as_str() {
                                "claude" => {
                                    if let Err(e) = crate::auth::claude::set_active_account(&label)
                                    {
                                        app.push_display_message(DisplayMessage::error(format!(
                                            "Failed to switch account: {}",
                                            e
                                        )));
                                    } else {
                                        crate::auth::AuthStatus::invalidate_cache();
                                        app.set_context_limit_and_sync_budget(
                                            app.provider.context_window(),
                                        );
                                        let _ = remote.switch_anthropic_account(&label).await;
                                        app.push_display_message(DisplayMessage::system(format!(
                                            "Switched to Anthropic account `{}`.",
                                            label
                                        )));
                                        app.set_status_notice(format!(
                                            "Account: switched to {}",
                                            label
                                        ));
                                    }
                                }
                                "openai" => {
                                    if let Err(e) = crate::auth::codex::set_active_account(&label) {
                                        app.push_display_message(DisplayMessage::error(format!(
                                            "Failed to switch OpenAI account: {}",
                                            e
                                        )));
                                    } else {
                                        crate::auth::AuthStatus::invalidate_cache();
                                        app.set_context_limit_and_sync_budget(
                                            app.provider.context_window(),
                                        );
                                        let _ = remote.switch_openai_account(&label).await;
                                        app.push_display_message(DisplayMessage::system(format!(
                                            "Switched to OpenAI account `{}`.",
                                            label
                                        )));
                                        app.set_status_notice(format!(
                                            "OpenAI account: switched to {}",
                                            label
                                        ));
                                    }
                                }
                                _ => app.push_display_message(DisplayMessage::error(format!(
                                    "Provider `{}` does not support account switching.",
                                    provider_id
                                ))),
                            }
                        }
                        crate::tui::AccountPickerAction::Add { .. }
                        | crate::tui::AccountPickerAction::Replace { .. }
                        | crate::tui::AccountPickerAction::OpenCenter { .. } => {}
                    }
                }
            }
            needs_redraw = true;
            needs_redraw |= dispatch_compacted_history_load(app, remote).await;
        }
        Some(Ok(Event::Paste(text))) => {
            input_attribution.event = Some(format!("paste:{}", text.len()));
            app.note_client_interaction();
            app.handle_paste(text);
            needs_redraw = true;
        }
        Some(Ok(Event::Mouse(mouse))) => {
            input_attribution.event = Some(format!("mouse:{:?}", mouse.kind));
            input_attribution.scroll_delta = mouse_scroll_delta(&mouse);
            if !matches!(mouse.kind, MouseEventKind::Moved) {
                app.note_client_interaction();
                handle_mouse_event(app, mouse);
                needs_redraw = true;
                needs_redraw |= dispatch_compacted_history_load(app, remote).await;
            }
        }
        Some(Ok(Event::Resize(_, _))) => {
            input_attribution.event = Some("resize".to_string());
            needs_redraw = app.should_redraw_after_resize();
        }
        Some(Err(error)) => {
            input_attribution.event = Some(format!("event_error:{}", error));
        }
        _ => {
            input_attribution.event = Some("none".to_string());
        }
    }
    crate::tui::ui::set_frame_input_attribution(input_attribution);
    Ok(needs_redraw)
}

fn key_scroll_delta(key: &KeyEvent) -> Option<i32> {
    match key.code {
        KeyCode::Up => Some(-1),
        KeyCode::PageUp => Some(-10),
        KeyCode::Down => Some(1),
        KeyCode::PageDown => Some(10),
        _ => None,
    }
}

fn mouse_scroll_delta(mouse: &MouseEvent) -> Option<i32> {
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(-1),
        MouseEventKind::ScrollDown => Some(1),
        MouseEventKind::ScrollLeft => Some(-1),
        MouseEventKind::ScrollRight => Some(1),
        _ => None,
    }
}

async fn dispatch_compacted_history_load(app: &mut App, remote: &mut RemoteConnection) -> bool {
    let Some(visible_messages) = app.take_pending_compacted_history_load() else {
        return false;
    };
    match remote.get_compacted_history(visible_messages).await {
        Ok(_) => true,
        Err(error) => {
            app.restore_pending_compacted_history_load(visible_messages);
            app.set_status_notice(format!("Failed to request older history: {}", error));
            true
        }
    }
}
