//! Path contract for the opt-in `kcode-dev` namespace.
//!
//! The launcher sets these paths before starting the binary. The binary checks
//! them again before build/promotion/import operations so inherited overrides
//! cannot silently redirect those operations into the normal Kcode install.

use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};

#[path = "dev_namespace/path_guard.rs"]
mod path_guard;
#[cfg(unix)]
use self::path_guard::validate_existing_dev_home;
use self::path_guard::{
    canonicalize_existing_prefix, paths_overlap, reject_dynamic_symlinks,
    reject_symlink_components, validate_optional_path_under, validate_socket_path,
};

const DEV_HOME_NAME: &str = ".kcode-dev";
const PRIVATE_STORE_ROOTS: &str = r#"
.grok .grok/auth.json external external/aws external/aws/credentials external/aws/config
external/azure external/azure/azureProfile.json external/azure/az.json
external/azure/msal_token_cache.json external/azure/accessTokens.json
external/gcloud external/gcloud/credentials.db external/gcloud/access_tokens.db
external/gcloud/application_default_credentials.json external/gcloud/active_config
external/gcloud/configurations/config_default
external/.agents external/.agents/skills external/.claude external/.claude/skills
external/.claude/.credentials.json external/.claude.json external/.codex
external/.codex/skills external/.codex/auth.json external/.codex/config.toml
external/.config external/.config/github-copilot/hosts.json
external/.config/github-copilot/apps.json external/.config/cursor/auth.json
external/.copilot external/.copilot/config.json external/.cursor
external/.cursor/auth.json external/.cursor/cli-config.json external/.cursor/projects
external/.gemini external/.gemini/oauth_creds.json external/.hermes external/.hermes/auth.json
external/.local external/.local/share/opencode/auth.json
external/.local/share/opencode/storage/session external/.local/share/opencode/storage/message
external/.local/share/opencode/storage/part external/.openclaw
external/.openclaw/agent/auth.json external/.openclaw/credentials/oauth.json
external/.openclaw/agents external/.pi external/.pi/agent/auth.json external/.pi/agent/sessions
external/AppData/Roaming external/AppData/Roaming/Cursor/auth.json external/AGENTS.md
config config/kcode config/kcode/kcode-subscription.env config/kcode/openrouter.env
config/kcode/openai.env config/kcode/anthropic.env config/kcode/gemini.env
config/kcode/cursor.env config/kcode/bedrock.env config/kcode/custom.env
config/kcode/302ai.env config/kcode/alibaba-coding-plan.env config/kcode/baseten.env
config/kcode/belvedir.env config/kcode/celeris.env config/kcode/cerebras.env
config/kcode/chutes.env config/kcode/comtegra.env config/kcode/conifer.env
config/kcode/cortecs.env config/kcode/deepinfra.env config/kcode/deepseek.env
config/kcode/fireworks.env config/kcode/firmware.env config/kcode/fpt.env
config/kcode/groq.env config/kcode/huggingface.env config/kcode/kimi.env
config/kcode/lmstudio.env config/kcode/meta-muse.env config/kcode/minimax.env
config/kcode/mistral.env config/kcode/moonshotai.env config/kcode/nebius.env
config/kcode/novita.env config/kcode/nvidia-nim.env config/kcode/ollama.env
config/kcode/openai-compatible.env config/kcode/opencode-go.env config/kcode/opencode.env
config/kcode/orcarouter.env config/kcode/perplexity.env config/kcode/scaleway.env
config/kcode/stackit.env config/kcode/togetherai.env config/kcode/xai.env
config/kcode/xiaomi-mimo.env config/kcode/zai.env
sessions session-metadata-v1.sqlite3 browser browser/.setup-complete logs notifications
Applications state cache memory cache/session-picker-list-v2.json profiles/heap
ambient/transcripts safety/queue.json safety/history.json goals/global
notifications/macos/inbox Applications/KcodeNotificationBroker.app
Applications/KcodeNotificationBroker.app/Contents/MacOS/KcodeNotificationBroker
session-memory/config schema-quirks.json last_seen_changelog telemetry_active_sessions
run/durable-state run/durable-state/swarm run/kcode-swarm-state
notes profiles overnight loops
active_pids streaming_pids internal_pids ssh-control provider-backends
provider-backends/grok-build provider-backends/grok provider-backends/grok-build/grok
provider-backends/grok/grok source source/kcode builds/source build.log build-progress
pending-login openai_oauth_usage.json auth-refresh-state.json auth-validation.json
composio_gmail.json cloud_sessions.json cloud_sessions_sync.json model-usage-v1.sqlite3
mcp-schema-cache.json schema-cache restart-snapshot.json update_metadata.json reload-context.json
reload-recovery reload-traces selfdev-build-requests selfdev-build-locks debug_control testers.json
devices.json servers.json ssh_remotes.json generated-images models side_panel
last_focused_client_session keymap-snapshot.json preferred_terminal.json setup_hints.json
hotkey launcher pending-soft-interrupts preferred-tools.md prompt-overlay.md telemetry_install_sent
install_conversion_id telemetry_version_sent telemetry_milestone catchup_seen.json ambient safety goals
auth.json config.toml openai-auth.json antigravity_oauth.json google_credentials.json google_oauth.json
gemini_oauth.json telemetry_id no_telemetry
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevNamespacePaths {
    pub home: PathBuf,
    pub install: PathBuf,
    pub runtime: PathBuf,
    pub target: PathBuf,
    pub xdg_config: PathBuf,
    pub xdg_data: PathBuf,
    pub xdg_state: PathBuf,
    pub xdg_cache: PathBuf,
}

pub fn launcher_requested() -> bool {
    if std::env::var_os("KCODE_DEV_NAMESPACE").as_deref() != Some(std::ffi::OsStr::new("1")) {
        return false;
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return false;
    };
    let Ok(paths) = resolve_for_home(&home) else {
        return false;
    };
    std::env::var_os("KCODE_HOME")
        .map(PathBuf::from)
        .is_some_and(|configured| {
            configured.is_absolute()
                && canonicalize_existing_prefix(&configured).is_ok_and(|path| path == paths.home)
        })
}

#[cfg(unix)]
pub fn resolve_for_home(home: &Path) -> Result<DevNamespacePaths> {
    if !home.is_absolute() {
        bail!("kcode-dev requires an absolute HOME path");
    }
    let home = canonicalize_existing_prefix(home)?;
    if home.components().any(|component| {
        matches!(
            component,
            Component::Normal(name)
                if name == ".kcode" || name == ".kcode-dev"
        )
    }) {
        bail!("kcode-dev HOME overlaps a protected Kcode data namespace");
    }
    let dev_home = home.join(DEV_HOME_NAME);
    reject_symlink_components(&dev_home)?;
    validate_existing_dev_home(&dev_home)?;

    let paths = DevNamespacePaths {
        home: dev_home.clone(),
        install: dev_home.join("bin"),
        runtime: dev_home.join("run"),
        target: dev_home.join("cargo-target"),
        xdg_config: dev_home.join("xdg/config"),
        xdg_data: dev_home.join("xdg/data"),
        xdg_state: dev_home.join("xdg/state"),
        xdg_cache: dev_home.join("xdg/cache"),
    };

    let protected = [
        home.join(".kcode"),
        home.join(".local/bin/kcode"),
        home.join(".kcode/builds/current"),
        home.join(".kcode/builds/shared-server"),
    ];
    let candidates = [
        &paths.home,
        &paths.install,
        &paths.runtime,
        &paths.target,
        &paths.xdg_config,
        &paths.xdg_data,
        &paths.xdg_state,
        &paths.xdg_cache,
    ]
    .into_iter()
    .cloned()
    .chain(
        PRIVATE_STORE_ROOTS
            .split_whitespace()
            .map(|relative| paths.home.join(relative)),
    )
    .collect::<Vec<_>>();
    for candidate in &candidates {
        reject_symlink_components(candidate)?;
        let resolved = canonicalize_existing_prefix(candidate)?;
        for protected_path in &protected {
            let protected_resolved = canonicalize_existing_prefix(protected_path)?;
            if paths_overlap(&resolved, &protected_resolved) {
                bail!(
                    "kcode-dev path {} overlaps protected production path {}",
                    resolved.display(),
                    protected_resolved.display()
                );
            }
        }
    }
    reject_dynamic_symlinks(&paths.home)?;

    Ok(paths)
}

#[cfg(not(unix))]
pub fn resolve_for_home(_home: &Path) -> Result<DevNamespacePaths> {
    bail!("kcode-dev is unavailable because this platform cannot verify private-root ACLs")
}

pub fn validate_environment() -> Result<DevNamespacePaths> {
    if !launcher_requested() {
        bail!("kcode-dev namespace marker is missing; launch through the kcode-dev wrapper");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("kcode-dev requires HOME to be set")?;
    let paths = resolve_for_home(&home)?;
    require_exact_env_path("KCODE_HOME", &paths.home)?;
    require_exact_env_path("KCODE_INSTALL_DIR", &paths.install)?;
    require_exact_env_path("KCODE_RUNTIME_DIR", &paths.runtime)?;
    require_exact_env_path("CARGO_TARGET_DIR", &paths.target)?;
    require_exact_env_path("XDG_CONFIG_HOME", &paths.xdg_config)?;
    require_exact_env_path("XDG_DATA_HOME", &paths.xdg_data)?;
    require_exact_env_path("XDG_STATE_HOME", &paths.xdg_state)?;
    require_exact_env_path("XDG_CACHE_HOME", &paths.xdg_cache)?;
    validate_optional_path_under("KCODE_MACOS_NOTIFICATION_INBOX", &paths.home)?;
    validate_optional_path_under(
        "KCODE_MACOS_NOTIFICATION_BROKER_APP",
        &paths.home.join("Applications"),
    )?;
    validate_optional_path_under(
        "KCODE_SESSION_MEMORY_TEMPLATE",
        &paths.home.join("session-memory/config"),
    )?;
    validate_optional_path_under("KCODE_SCHEMA_QUIRKS_PATH", &paths.home)?;

    if let Some(socket) = std::env::var_os("KCODE_SOCKET") {
        validate_socket_path(&PathBuf::from(socket), &paths.runtime)?;
    }
    if let Some(socket) = std::env::var_os("KCODE_API_SOCKET") {
        validate_socket_path(&PathBuf::from(socket), &paths.runtime)?;
    }
    Ok(paths)
}

/// Prevent the isolated fork from silently borrowing credentials or SDK
/// profiles inherited from the normal launcher environment.
pub fn isolate_provider_environment(paths: &DevNamespacePaths) {
    let secret_suffixes = [
        "_API_KEY",
        "_KEY",
        "_TOKEN",
        "_TOKEN_FILE",
        "_SECRET",
        "_PASSWORD",
        "_CREDENTIAL",
        "_CREDENTIALS",
        "_CREDENTIALS_FILE",
    ];
    let external_profile_vars = [
        "AWS_PROFILE",
        "AWS_DEFAULT_PROFILE",
        "AWS_SHARED_CREDENTIALS_FILE",
        "AWS_CONFIG_FILE",
        "AWS_WEB_IDENTITY_TOKEN_FILE",
        "AWS_CONTAINER_CREDENTIALS_FULL_URI",
        "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
        "GOOGLE_APPLICATION_CREDENTIALS",
    ];
    let provider_overrides = [
        "CLAUDE_CONFIG_DIR",
        "KCODE_GROK_CLI_PATH",
        "KCODE_ANTHROPIC_HEADERS",
        "KCODE_ANTHROPIC_AUTH_HEADER",
        "KCODE_ANTHROPIC_API_BASE",
        "ANTHROPIC_BASE_URL",
        "KCODE_OPENROUTER_API_BASE",
    ];
    let to_remove = std::env::vars_os()
        .filter_map(|(name, _)| {
            let name = name.to_str()?;
            let normalized = name.to_ascii_uppercase();
            (provider_overrides.contains(&normalized.as_str())
                || secret_suffixes
                    .iter()
                    .any(|suffix| normalized.ends_with(suffix))
                || external_profile_vars.contains(&normalized.as_str()))
            .then(|| name.to_string())
        })
        .collect::<Vec<_>>();
    for name in to_remove {
        crate::env::remove_var(name);
    }
    crate::env::remove_var("KCODE_COPILOT_ALLOW_GH_AUTH_TOKEN");

    crate::env::set_var(
        "AWS_SHARED_CREDENTIALS_FILE",
        paths.home.join("external/aws/credentials"),
    );
    crate::env::set_var("AWS_CONFIG_FILE", paths.home.join("external/aws/config"));
    crate::env::set_var("GROK_HOME", paths.home.join(".grok"));
    crate::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
    crate::env::set_var("AZURE_CONFIG_DIR", paths.home.join("external/azure"));
    crate::env::set_var("CLOUDSDK_CONFIG", paths.home.join("external/gcloud"));
}

pub fn ensure_global_integrations_allowed(action: &str) -> Result<()> {
    if std::env::var_os("KCODE_DEV_NAMESPACE").is_some() {
        bail!("`{action}` is disabled in the kcode-dev namespace to protect global user settings");
    }
    Ok(())
}

pub fn validate_socket_override(socket: Option<&str>) -> Result<()> {
    let Some(socket) = socket else {
        return Ok(());
    };
    let paths = validate_environment()?;
    validate_socket_path(Path::new(socket), &paths.runtime)
}

pub fn validate_api_socket_override(socket: Option<&str>) -> Result<()> {
    let Some(socket) = socket else {
        return Ok(());
    };
    let paths = validate_environment()?;
    validate_socket_path(Path::new(socket), &paths.runtime)
}

/// Copy one selected session's recoverable files from the legacy store.
/// The source is opened read-only; provider credentials and all other state are
/// intentionally excluded, and this operation never writes back to `~/.kcode`.
#[cfg(unix)]
#[path = "dev_namespace/legacy_import.rs"]
mod legacy_import;

#[cfg(unix)]
pub fn import_legacy_session(session_id: &str) -> Result<Vec<PathBuf>> {
    let paths = validate_environment()?;
    legacy_import::import(&paths, session_id)
}

#[cfg(not(unix))]
pub fn import_legacy_session(_session_id: &str) -> Result<Vec<PathBuf>> {
    bail!("legacy session import is unavailable on this platform")
}

fn require_exact_env_path(name: &str, expected: &Path) -> Result<()> {
    let value = std::env::var_os(name)
        .map(PathBuf::from)
        .with_context(|| format!("kcode-dev requires {name} to be set by its wrapper"))?;
    if !value.is_absolute() || canonicalize_existing_prefix(&value)? != *expected {
        bail!(
            "unsafe kcode-dev {name}={}; expected {}",
            value.display(),
            expected.display()
        );
    }
    reject_symlink_components(&value)?;
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "dev_namespace_tests.rs"]
mod tests;

#[cfg(all(test, not(unix)))]
#[test]
fn dev_namespace_fails_closed_when_private_acl_validation_is_unavailable() {
    assert!(resolve_for_home(Path::new("C:\\Users\\kcode")).is_err());
}
