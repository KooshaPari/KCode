//! Integration layer for `kcode-session-memory`.
//!
//! Provides budget enforcement for session notes and template-based prompt
//! building. When saving ambient state or building prompts, call into this
//! module to validate that session notes fit within the 12K token budget and
//! to render custom templates with variable substitution.

use kcode_session_memory::{
    BudgetReport, TemplateLoader, VariableSubstitutor, analyze_sections, check_budget,
    check_budget_from_content, generate_budget_warnings, truncate_section,
};
use std::collections::HashMap;

/// Maximum tokens allowed per section.
pub const MAX_SECTION_TOKENS: usize = kcode_session_memory::MAX_SECTION_LENGTH;
/// Maximum total tokens across all sections.
pub const MAX_TOTAL_TOKENS: usize = kcode_session_memory::MAX_TOTAL_SESSION_MEMORY_TOKENS;

/// Validate session notes against the budget and return a report.
///
/// Use this when saving ambient state to detect oversized notes before
/// persisting them.
pub fn validate_session_notes(content: &str) -> BudgetReport {
    check_budget_from_content(content)
}

/// Check budget from a pre-built [`SectionAnalysis`].
pub fn validate_from_analysis(analysis: &kcode_session_memory::SectionAnalysis) -> BudgetReport {
    check_budget(analysis)
}

/// Generate human-readable budget warnings for a report.
pub fn budget_warnings(report: &BudgetReport) -> Vec<String> {
    generate_budget_warnings(report)
}

/// Truncate a section to fit within the per-section token budget.
pub fn fit_section(content: &str) -> String {
    truncate_section(content, MAX_SECTION_TOKENS)
}

/// Build a session notes prompt from a custom template.
///
/// Loads `template.md` from `~/.kcode/session-memory/config/`, substitutes
/// `{{variable}}` placeholders, and returns the rendered text. Falls back
/// to the raw content if no template is found.
pub fn render_session_template(content: &str, variables: &HashMap<String, String>) -> String {
    let substitutor = VariableSubstitutor::new(variables.clone());
    let Ok(kcode_home) = crate::storage::kcode_dir() else {
        return substitutor.substitute(content);
    };
    let template_dir = std::env::var_os("KCODE_SESSION_MEMORY_TEMPLATE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| kcode_home.join("session-memory/config"));
    let template_path = template_dir.join("template.md");
    if let Err(error) = crate::storage::reject_dev_home_symlink_path(&template_path) {
        crate::logging::warn(&format!(
            "rejected session-memory template path {}: {error}",
            template_path.display()
        ));
        return substitutor.substitute(content);
    }
    let loader = TemplateLoader::with_base_dir(template_dir.to_string_lossy().into_owned());

    match loader.load_template() {
        Some(template) => {
            // Merge user content into the template sections, then substitute
            let mut rendered = template.raw.clone();
            for section in &template.sections {
                if let Some(value) = variables.get(&section.name) {
                    let section_header = format!("## {}", section.name);
                    let header_line = format!("{section_header}\n");
                    let Some(header_start) = rendered
                        .match_indices(&header_line)
                        .find(|(start, _)| *start == 0 || rendered.as_bytes()[start - 1] == b'\n')
                        .map(|(start, _)| start)
                    else {
                        continue;
                    };
                    let body_start = rendered[header_start..]
                        .find('\n')
                        .map(|offset| header_start + offset + 1)
                        .unwrap_or(rendered.len());
                    let body_end = rendered[body_start..]
                        .find("\n## ")
                        .map(|offset| body_start + offset)
                        .unwrap_or(rendered.len());
                    let body = &rendered[body_start..body_end];
                    let replacement_range = if body.trim().is_empty() {
                        body_end..body_end
                    } else {
                        let trailing = body.len() - body.trim_end().len();
                        body_start..body_end - trailing
                    };
                    rendered.replace_range(replacement_range, value);
                }
            }
            substitutor.substitute(&rendered)
        }
        None => substitutor.substitute(content),
    }
}

/// Analyze sections of a markdown document for size diagnostics.
pub fn diagnose_sections(content: &str) -> kcode_session_memory::SectionAnalysis {
    analyze_sections(content)
}

/// Find sections that exceed the per-section token limit.
pub fn oversized_sections(content: &str) -> Vec<(String, usize)> {
    let analysis = analyze_sections(content);
    kcode_session_memory::find_oversized_sections(&analysis, MAX_SECTION_TOKENS)
}

/// Convenience: check if content fits within the total token budget.
pub fn fits_budget(content: &str) -> bool {
    !check_budget_from_content(content).is_over_budget
}

#[cfg(all(test, unix))]
mod isolation_tests {
    use super::*;

    struct RestoreEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                if let Some(value) = value {
                    crate::env::set_var(key, value);
                } else {
                    crate::env::remove_var(key);
                }
            }
        }
    }

    #[test]
    fn session_template_symlink_does_not_import_target_contents() {
        let _lock = crate::storage::lock_test_env();
        let _restore = RestoreEnv(
            [
                "HOME",
                "KCODE_HOME",
                "KCODE_DEV_NAMESPACE",
                "KCODE_SESSION_MEMORY_TEMPLATE",
            ]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
        );
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".kcode-dev");
        std::fs::create_dir_all(&home).unwrap();
        let home = std::fs::canonicalize(home).unwrap();
        let template_dir = home.join("session-memory/config");
        let target = temp.path().join("stable-template.md");
        std::fs::create_dir_all(&template_dir).unwrap();
        std::fs::write(&target, "STABLE_TEMPLATE_SENTINEL").unwrap();
        std::os::unix::fs::symlink(&target, template_dir.join("template.md")).unwrap();
        crate::env::set_var("HOME", temp.path());
        crate::env::set_var("KCODE_HOME", &home);
        crate::env::set_var("KCODE_DEV_NAMESPACE", "1");
        crate::env::remove_var("KCODE_SESSION_MEMORY_TEMPLATE");

        assert!(crate::storage::running_in_dev_namespace());
        let rendered = render_session_template("local content", &HashMap::new());

        assert_eq!(rendered, "local content");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "STABLE_TEMPLATE_SENTINEL"
        );
    }

    #[test]
    fn session_template_replaces_section_with_parser_trimmed_body() {
        let _lock = crate::storage::lock_test_env();
        let _restore = RestoreEnv(
            [
                "HOME",
                "KCODE_HOME",
                "KCODE_DEV_NAMESPACE",
                "KCODE_SESSION_MEMORY_TEMPLATE",
            ]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
        );
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".kcode-dev");
        let template_dir = home.join("session-memory/config");
        std::fs::create_dir_all(&template_dir).unwrap();
        std::fs::write(
            template_dir.join("template.md"),
            "## Summary\n\nOriginal summary\n\n## Details\nKeep details\n",
        )
        .unwrap();
        crate::env::set_var("HOME", temp.path());
        crate::env::set_var("KCODE_HOME", &home);
        crate::env::set_var("KCODE_DEV_NAMESPACE", "1");
        crate::env::remove_var("KCODE_SESSION_MEMORY_TEMPLATE");
        assert!(crate::storage::running_in_dev_namespace());
        let variables = HashMap::from([("Summary".to_string(), "Replacement summary".to_string())]);

        let rendered = render_session_template("unused fallback", &variables);

        assert_eq!(
            rendered,
            "## Summary\nReplacement summary\n\n## Details\nKeep details\n"
        );
        assert!(!rendered.contains("Original summary"));
    }
}
