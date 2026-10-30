//! ACP read-only permission mode.

use super::*;

/// Apply a permission mode to a session, and record what was actually applied.
///
/// `read-only` disables every tool that can change the workspace or reach the
/// outside world, by asking the daemon to drop them from the session's tool
/// set. The recorded mode is what the `mode` config option then advertises,
/// so a host that reads it back learns the truth.
///
/// A mode this agent does not implement is refused rather than accepted and
/// ignored: a host that selected it would believe the session was restricted
/// when it was not.
async fn apply_permission_mode(session: &Arc<DaemonSession>, value: &str) -> Result<()> {
    if value != MODE_READ_ONLY {
        anyhow::bail!(
            "Unsupported permission mode: {value}. This agent implements {MODE_READ_ONLY} only."
        );
    }
    let request_id = session.next_id();
    session
        .send(&Request::ConfigureTools {
            id: request_id,
            tools: SessionToolConfig {
                enabled: None,
                disabled: READ_ONLY_DENIED_TOOLS
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect(),
                custom: Vec::new(),
            },
        })
        .await?;
    wait_for_ack(session, request_id).await?;
    session.ui_state.lock().await.permission_mode = Some(MODE_READ_ONLY.to_string());
    Ok(())
}

/// ACP permission-mode selector.
///
/// ACP v1 hosts that gate tool calls through the agent's own session
/// configuration (Kiro Crew's ``codex`` slot is one) look for a config option
/// whose id is ``mode`` and that advertises the values the host may select.
/// Advertising it is a PROMISE: the value must actually restrict the session,
/// or a host that selected it would believe the session cannot write.
const CONFIG_ID_MODE: &str = "mode";

/// The only permission mode this agent advertises.
///
/// `read-only` disables every tool that can change the workspace or reach the
/// network, and leaves the read-only ones mounted. It is the mode a host
/// selects when it wants the agent to look but not touch.
const MODE_READ_ONLY: &str = "read-only";

/// The permission mode a session is in when no mode has been selected.
///
/// A session that has not been restricted is NOT read-only. It is this mode,
/// and it is named differently from `read-only` on purpose: a host that reads
/// the advertised `currentValue` must be able to tell "restricted" from
/// "unrestricted", and it must never read an unrestricted session as though
/// it had been restricted.
const MODE_UNRESTRICTED: &str = "unrestricted";

/// Tools that can change the workspace, the machine, or the outside world.
///
/// A `read-only` session disables exactly these and keeps the rest, so the
/// session can still read the repository and search it. The list is the
/// dangerous half of the builtin set; everything omitted here is read-only by
/// construction.
///
/// The set is deliberately over-inclusive: a tool that spawns other tools
/// (`batch`), spawns agents (`swarm`), or opens a UI that can drive the
/// machine is in it even though its own name does not say "write". A name that
/// is merely unfamiliar is also in it, because an unfamiliar tool is one
/// whose behaviour nobody has verified to be read-only.
const READ_ONLY_DENIED_TOOLS: &[&str] = &[
    // Filesystem mutation.
    "write",
    "edit",
    "replace",
    "apply_patch",
    // Process execution and its relatives.
    "bash",
    "bg",
    "compile_remote",
    // Outbound network and desktop control.
    "browser",
    "webfetch",
    "websearch",
    "macos_computer_use",
    "open",
    // Session and memory mutation.
    "todo",
    "initiative",
    "memory",
    "session_search",
    "gmail",
    "calendar",
    "schedule",
    "selfdev",
    "desktop_selfdev",
    // Tools that run other tools, spawn agents, or open a machine-driving UI.
    "batch",
    "swarm",
    "applet",
    "panel",
    "side_panel",
    "skill_manage",
    "integration_tools",
    "elicitate_mcp",
    // Every MCP tool. The name is a prefix match in the daemon's own policy
    // check, so this one entry covers `mcp_call`, `mcp_search` and every
    // `mcp__<server>__<tool>` an MCP server contributed. Without it a
    // read-only session could still reach an MCP server and change remote
    // data through it.
    "mcp",
];

const CONFIG_ID_MODEL: &str = "model";
const CONFIG_ID_EFFORT: &str = "reasoning_effort";

fn acp_available_commands() -> Vec<Value> {
    vec![
        json!({
            "name": "model",
            "description": "Switch the model for this session, or show the current model",
            "input": { "hint": "model id (optional)" },
        }),
        json!({
            "name": "models",
            "description": "List models available from the active provider",
        }),
        json!({
            "name": "effort",
            "description": "Set reasoning effort, or show the current effort",
            "input": { "hint": "none|minimal|low|medium|high|xhigh|max (optional)" },
        }),
    ]
}

fn insert_session_configuration(result: &mut Value, state: &SessionUiState) {
    let Some(object) = result.as_object_mut() else {
        return;
    };
    let config_options = session_config_options(state);
    if !config_options.is_empty() {
        object.insert("configOptions".to_string(), Value::Array(config_options));
    }
    if let Some(models) = session_models(state) {
        object.insert("models".to_string(), models);
    }
}

fn session_models(state: &SessionUiState) -> Option<Value> {
    let current = state.model.as_deref()?;
    let mut models = state.available_models.clone();
    if !models.iter().any(|candidate| candidate == current) {
        models.insert(0, current.to_string());
    }
    Some(json!({
        "availableModels": models
            .into_iter()
            .map(|model| json!({ "modelId": model, "name": model }))
            .collect::<Vec<_>>(),
        "currentModelId": current,
    }))
}

fn available_efforts(state: &SessionUiState) -> Vec<&'static str> {
    crate::provider::inferred_reasoning_efforts(
        state.provider_name.as_deref(),
        state.model.as_deref(),
    )
    .into_iter()
    // `swarm`/`swarm-deep` are TUI sentinels, not provider effort levels.
    .filter(|effort| !effort.starts_with("swarm"))
    .collect()
}

/// Build the ACP `configOptions` array (model selector plus reasoning effort)
/// from the current session provider state. Empty when the daemon reported no
/// usable model state.
fn session_config_options(state: &SessionUiState) -> Vec<Value> {
    let mut options = Vec::new();

    // The permission mode is advertised FIRST and unconditionally. A host that
    // gates tool calls through the agent's session configuration looks for this
    // option by id, and a session that only advertised it once a host had
    // already asked for it would be a session the host had to trust blind.
    options.push(json!({
        "type": "select",
        "id": CONFIG_ID_MODE,
        "name": "Permission mode",
        "category": "permission",
        // The advertised value is the mode that is ACTUALLY in force. A session
        // that has not been restricted says so, rather than claiming a
        // restriction that was never applied.
        "currentValue": state.permission_mode.as_deref().unwrap_or(MODE_UNRESTRICTED),
        "options": [{
            "value": MODE_READ_ONLY,
            "name": "Read-only",
        }],
    }));

    if let Some(model) = state.model.as_deref() {
        let mut models = state.available_models.clone();
        if !models.iter().any(|candidate| candidate == model) {
            models.insert(0, model.to_string());
        }
        let select_options: Vec<Value> = models
            .iter()
            .map(|name| json!({ "value": name, "name": name }))
            .collect();
        options.push(json!({
            "type": "select",
            "id": CONFIG_ID_MODEL,
            "name": "Model",
            "category": "model",
            "currentValue": model,
            "options": select_options,
        }));
    }

    let efforts = available_efforts(state);
    if !efforts.is_empty() {
        let current = state
            .reasoning_effort
            .as_deref()
            .filter(|effort| efforts.contains(effort))
            .unwrap_or_else(|| {
                if efforts.contains(&"medium") {
                    "medium"
                } else {
                    efforts[0]
                }
            });
        let select_options: Vec<Value> = efforts
            .iter()
            .map(|name| json!({ "value": name, "name": name }))
            .collect();
        options.push(json!({
            "type": "select",
            "id": CONFIG_ID_EFFORT,
            "name": "Reasoning effort",
            "category": "thought_level",
            "currentValue": current,
            "options": select_options,
        }));
    }

    options
}
