use super::{App, DisplayMessage};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PokeContextScope {
    AllRepos,
    Repo(String),
}

pub fn parse_poke_context_command(input: &str) -> Option<Result<PokeContextScope, String>> {
    let input = input.trim();
    if input == "/poke" {
        return Some(Ok(PokeContextScope::AllRepos));
    }
    let Some(rest) = input.strip_prefix("/poke ") else {
        return None;
    };
    let rest = rest.trim();
    if rest == "on" || rest == "off" || rest == "status" {
        return None;
    }
    if rest == "all" {
        return Some(Ok(PokeContextScope::AllRepos));
    }
    if rest.is_empty()
        || !rest
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Some(Err("Usage: /poke [repo-name]".to_string()));
    }
    Some(Ok(PokeContextScope::Repo(rest.to_string())))
}

pub fn build_poke_context_prompt(scope: &PokeContextScope) -> String {
    let scope = match scope {
        PokeContextScope::AllRepos => "all active repositories".to_string(),
        PokeContextScope::Repo(repo) => format!("repository `{repo}`"),
    };
    format!(
        "You were started by `/poke` in a fresh Jcode session to recover the operator's active work context and continue safely. Scope: {scope}.\n\n\
         First recover context before acting: read the global memory registry and its relevant entries under `~/.jcode/memories/global/`; read relevant project memories for the scoped repositories; inspect this session's recent conversation history with `conversation_search` (stats, then recent turns and relevant prior requests). Treat the latest user instruction as authoritative and preserve prior constraints.\n\n\
         Then report the active repositories in scope, their current state, work already completed, blockers, and the next concrete actions. Derive active repositories from recovered memories and session history, then verify current repository state with read-only inspection. Mark unavailable facts UNKNOWN.\n\n\
         Continue only with non-harmful, trivially reversible work such as reading, auditing, planning, and isolated local edits or tests. Before any destructive, externally visible, irreversible, or consequential action, stop and request the operator's decision. Preserve unrelated work, never rewrite Git history, and never use screenshots or recordings. Do not ask the operator to restate context that can be recovered from the memories or this session."
    )
}

pub(super) fn handle_poke_context_command(app: &mut App, input: &str) -> bool {
    let Some(parsed) = parse_poke_context_command(input) else {
        return false;
    };
    match parsed {
        Err(error) => app.push_display_message(DisplayMessage::error(error)),
        Ok(scope) => {
            let prompt = build_poke_context_prompt(&scope);
            if app.is_remote {
                app.push_display_message(DisplayMessage::error(
                    "/poke context launch is unavailable while disconnected from the Jcode server. Reconnect and try again.".to_string(),
                ));
            } else {
                match super::launch_prompt_in_new_session_local(app, prompt, Vec::new()) {
                    Ok(_) => {}
                    Err(error) => app.push_display_message(DisplayMessage::error(format!(
                        "Failed to launch /poke context session: {error}"
                    ))),
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{PokeContextScope, build_poke_context_prompt, parse_poke_context_command};

    #[test]
    fn parse_poke_context_scope() {
        assert_eq!(
            parse_poke_context_command("/poke"),
            Some(Ok(PokeContextScope::AllRepos))
        );
        assert_eq!(
            parse_poke_context_command("/poke all"),
            Some(Ok(PokeContextScope::AllRepos))
        );
        assert_eq!(
            parse_poke_context_command("/poke jcode"),
            Some(Ok(PokeContextScope::Repo("jcode".to_string())))
        );
        assert_eq!(parse_poke_context_command("/poke on"), None);
        assert_eq!(parse_poke_context_command("/poke status"), None);
        assert_eq!(
            parse_poke_context_command("/poke two words"),
            Some(Err("Usage: /poke [repo-name]".to_string()))
        );
        assert_eq!(
            parse_poke_context_command("/poke `inspect secrets`"),
            Some(Err("Usage: /poke [repo-name]".to_string()))
        );
        assert_eq!(parse_poke_context_command("/pokeball"), None);
    }

    #[test]
    fn prompt_requires_context_recovery_and_safe_progress() {
        let prompt = build_poke_context_prompt(&PokeContextScope::AllRepos);
        for phrase in [
            "global memory registry",
            "project memories",
            "conversation_search",
            "active repositories",
            "trivially reversible",
            "request the operator's decision",
            "UNKNOWN",
        ] {
            assert!(prompt.contains(phrase), "missing {phrase:?}");
        }
    }
}
