pub(super) async fn handle_bus_event(
    app: &mut App,
    remote: &mut RemoteConnection,
    bus_event: std::result::Result<BusEvent, tokio::sync::broadcast::error::RecvError>,
    state: &mut RemoteRunState,
) -> bool {
    match bus_event {
        Ok(BusEvent::UsageReport(results)) => {
            app.handle_usage_report(results);
            true
        }
        Ok(BusEvent::ClipboardPasteCompleted(result)) => {
            app.handle_clipboard_paste_completed(result)
        }
        Ok(BusEvent::ModelRefreshCompleted(result)) => {
            app.handle_model_refresh_completed(result);
            true
        }
        Ok(BusEvent::UiActivity(activity)) => super::local::handle_ui_activity(app, activity),
        Ok(BusEvent::GitStatusCompleted(result)) => {
            super::commands::handle_git_status_completed(app, result);
            true
        }
        Ok(BusEvent::ProductivityReportReady(event)) => {
            app.handle_productivity_report_ready(event);
            true
        }
        Ok(BusEvent::MermaidRenderCompleted) => true,
        Ok(BusEvent::UsageReportProgress(progress)) => {
            app.handle_usage_report_progress(progress);
            true
        }
        Ok(BusEvent::LoginCompleted(login)) => {
            if crate::tui::is_ssh_remote() {
                app.set_status_notice("Local login does not change SSH server credentials");
                return true;
            }
            let success = login.success && login.provider != "copilot_code";
            let provider_hint = auth_provider_hint_for_login_provider(&login.provider);
            let auth = auth_changed_event_for_login_provider(&login.provider);
            let prefer_strongest = success && app.onboarding_should_prefer_strongest_model();
            app.handle_login_completed(login);
            if success
                && let Err(error) = remote
                    .notify_auth_changed_event(provider_hint, auth, prefer_strongest)
                    .await
            {
                crate::logging::warn(&format!(
                    "Failed to notify server about refreshed auth: {error}"
                ));
                app.finish_auth_catalog_refresh();
                app.set_status_notice("Model setup will retry after reconnect");
            }
            true
        }
        Ok(BusEvent::OnboardingModelValidated(result)) => {
            app.handle_onboarding_model_validated(result)
        }
        Ok(BusEvent::UpdateStatus(status)) => {
            app.handle_update_status(status);
            true
        }
        Ok(BusEvent::SessionUpdateStatus(status)) => {
            app.handle_session_update_status(status);
            true
        }
        Ok(BusEvent::DictationCompleted {
            dictation_id,
            session_id,
            text,
            mode,
        }) => {
            if !app.owns_dictation_event(&dictation_id, session_id.as_deref()) {
                return false;
            }
            if reconnect::has_unresolved_for_session(app, state) {
                apply_transcript_event(app, text, crate::protocol::TranscriptMode::Append);
                app.mark_dictation_delivered();
                app.set_status_notice("Transcript retained in draft; accept the unknown outcome with Ctrl+E before sending");
                return true;
            }
            match remote.send_transcript(text, mode).await {
                Ok(()) => app.mark_dictation_delivered(),
                Err(error) => app.handle_dictation_failure(error.to_string()),
            }
            true
        }
        Ok(BusEvent::DictationFailed {
            dictation_id,
            session_id,
            message,
        }) => {
            if !app.owns_dictation_event(&dictation_id, session_id.as_deref()) {
                return false;
            }
            app.handle_dictation_failure(message);
            true
        }
        Ok(BusEvent::VoiceInputWake) if reconnect::has_unresolved_for_session(app, state) => {
            let session = app
                .remote_session_id
                .clone()
                .or_else(|| app.resume_session_id.clone());
            if !state.held_voice_wakes.contains(&session) {
                state.held_voice_wakes.push(session);
            }
            app.set_status_notice(
                "Voice input held; accept the unknown outcome with Ctrl+E before sending",
            );
            true
        }
        Ok(BusEvent::VoiceInputWake) => match app.poll_voice_input() {
            super::voice_input::VoicePoll::Idle => false,
            super::voice_input::VoicePoll::Changed => true,
            super::voice_input::VoicePoll::Transcript(text) => {
                if let Err(error) = submit_remote_voice_transcript(app, remote, &text).await {
                    app.push_display_message(DisplayMessage::error(format!(
                        "Failed to send voice transcript: {error}"
                    )));
                    app.set_status_notice(if input_dispatch::is_uncertain_ssh_write(&error) {
                        "Voice transcript outcome unknown"
                    } else {
                        "Voice transcript not sent"
                    });
                }
                process_remote_followups_guarded(app, remote, state).await;
                true
            }
        },
        _ => false,
    }
}

/// Resolve the canonical auth provider id the server uses to attribute an
/// auth-change refresh for a completed login.
///
/// `LoginCompleted.provider` is the login descriptor's display label (e.g.
/// "Anthropic API"), id, or alias - not the canonical server provider id. This
/// used to only map Azure and OpenAI-compatible logins, so direct logins
/// (Claude OAuth/API key, OpenAI, OpenRouter, Bedrock, ...) sent no hint. With
/// no hint the server fell back to the session's currently active provider,
/// mislabeling the catalog-refresh message ("OpenAI credentials are active"
/// after an Anthropic API-key login) and skipping the post-login model switch.
fn auth_provider_hint_for_login_provider(provider: &str) -> Option<&'static str> {
    let provider = provider.trim();
    // Azure's runtime id ("azure-openai") differs from its login descriptor id
    // ("azure"); keep the dedicated mapping used across the auth lifecycle.
    if provider.eq_ignore_ascii_case("azure")
        || provider.eq_ignore_ascii_case("azure-openai")
        || provider.eq_ignore_ascii_case("azure openai")
    {
        return Some("azure-openai");
    }

    use crate::provider_catalog::LoginProviderTarget;
    let descriptor = crate::provider_catalog::resolve_login_provider_loose(provider)?;
    match descriptor.target {
        LoginProviderTarget::Azure => Some("azure-openai"),
        // OpenAI-compatible profiles carry their own catalog namespace id.
        LoginProviderTarget::OpenAiCompatible(profile) => Some(profile.id),
        // Auto-import has no single runtime to attribute the refresh to.
        LoginProviderTarget::AutoImport => None,
        _ => Some(descriptor.id),
    }
}

fn auth_changed_event_for_login_provider(provider: &str) -> Option<crate::protocol::AuthChanged> {
    use crate::provider_catalog::LoginProviderTarget;
    let provider_id = auth_provider_hint_for_login_provider(provider)?;
    let mut auth = crate::protocol::AuthChanged::new(provider_id);
    // These fields are informational; the server routes off `provider` and the
    // `expected_*` hints. Reflect the descriptor's auth kind so OAuth logins are
    // not recorded as API-key pastes.
    let descriptor = crate::provider_catalog::resolve_login_provider_loose(provider);
    let api_key_login = descriptor
        .map(|descriptor| {
            use crate::provider_catalog::LoginProviderAuthKind;
            matches!(
                descriptor.auth_kind,
                LoginProviderAuthKind::ApiKey | LoginProviderAuthKind::Hybrid
            )
        })
        .unwrap_or(true);
    if api_key_login {
        auth.auth_method = Some(crate::protocol::AuthMethod::RemoteTuiPasteApiKey);
        auth.credential_source = Some(crate::protocol::AuthCredentialSource::ApiKeyFile);
    }
    // Only logins whose descriptor actually targets the OpenAI-compatible
    // runtime claim its namespace. Do not key this off
    // `openai_compatible_profile_by_id`: native providers (`anthropic-api`,
    // `openai-api`) alias doctor-probe compat profiles with the same id, but
    // their auth activation deliberately routes through the native runtime.
    if provider_id == "azure-openai" {
        auth.expected_runtime = Some(crate::protocol::RuntimeProviderKey::new("azure-openai"));
        auth.expected_catalog_namespace =
            Some(crate::protocol::CatalogNamespace::new("azure-openai"));
    } else if descriptor
        .is_some_and(|d| matches!(d.target, LoginProviderTarget::OpenAiCompatible(_)))
    {
        auth.expected_runtime = Some(crate::protocol::RuntimeProviderKey::new(
            "openai-compatible",
        ));
        auth.expected_catalog_namespace = Some(crate::protocol::CatalogNamespace::new(provider_id));
    }
    Some(auth)
}

pub(super) async fn check_debug_command(
    app: &mut App,
    remote: &mut RemoteConnection,
) -> Option<String> {
    let cmd_path = super::debug_cmd_path();
    if let Ok(cmd) = std::fs::read_to_string(&cmd_path) {
        let _ = std::fs::remove_file(&cmd_path);
        let cmd = cmd.trim();

        app.debug_trace.record("cmd", cmd.to_string());

        let response = handle_debug_command(app, cmd, remote).await;
        let _ = std::fs::write(super::debug_response_path(), &response);
        return Some(response);
    }
    None
}

fn handle_terminal_event_while_disconnected(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    event: Option<std::result::Result<Event, std::io::Error>>,
) -> Result<bool> {
    let mut needs_redraw = false;

    match event {
        Some(Ok(Event::FocusGained)) => {
            crate::tui::reapply_configured_terminal_modes_after_focus();
            needs_redraw |= app.set_client_focused(true);
            app.note_client_focus(true);
        }
        Some(Ok(Event::FocusLost)) => {
            app.set_client_focused(false);
        }
        Some(Ok(Event::Key(key))) => {
            app.note_client_interaction();
            app.update_copy_badge_key_event(key);
            app.observe_voice_key_release(&key);
            if app.handle_voice_key_event(&key) {
                // Voice keys work from every screen and never type.
            } else if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                handle_disconnected_key_event(app, key)?;
            }
            needs_redraw = true;
        }
        Some(Ok(Event::Paste(text))) => {
            app.note_client_interaction();
            app.handle_paste(text);
            needs_redraw = true;
        }
        Some(Ok(Event::Mouse(mouse))) => {
            if !matches!(mouse.kind, MouseEventKind::Moved) {
                app.note_client_interaction();
                handle_mouse_event(app, mouse);
                needs_redraw = true;
            }
        }
        Some(Ok(Event::Resize(_, _))) => {
            needs_redraw = app.should_redraw_after_resize();
        }
        None => {
            // Input EOF: if the controlling terminal is gone this client is an
            // orphan (window died without a deliverable SIGHUP). Quit instead
            // of reconnect-looping forever with no way to ever receive input.
            if super::terminal_liveness::terminal_abandoned() {
                crate::logging::warn(
                    "Terminal input closed and controlling terminal is gone while disconnected; exiting orphaned client",
                );
                app.should_quit = true;
            }
        }
        _ => {}
    }

    if needs_redraw {
        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026h");
        terminal.draw(|frame| crate::tui::ui::draw(frame, app))?;
        let _ = std::io::Write::write_all(&mut std::io::stdout(), b"\x1b[?2026l");
    }

    Ok(app.should_quit)
}
