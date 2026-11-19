//! Conservative, offline tier classification for account routing.
//!
//! Strings here come from provider APIs, so matching is case-insensitive and
//! tolerant of separators. No network access or credential material is used.

/// Detected tier for an account. Lower numbers are preferred.
pub fn detected_tier(provider: &str, plan: Option<&str>) -> u8 {
    let provider = normalize(provider);
    let plan = match plan {
        Some(plan) => normalize(plan),
        None => String::new(),
    };

    if (provider == "claude" && plan.starts_with("max"))
        || (provider == "openai" && (plan == "pro" || plan == "promax"))
    {
        return 1;
    }
    if plan == "free" || plan == "apikey" || plan == "api" {
        return 3;
    }
    // Metered API-key accounts are identified by an `api` suffix/marker in
    // the provider id (for example `openai-api`) when no plan is available.
    if provider.ends_with("api") || provider.contains("apikey") {
        return 3;
    }
    2
}

/// Manual override always wins; otherwise use the detected tier.
pub fn effective_tier(detected: u8, override_priority: Option<u8>) -> u8 {
    override_priority.unwrap_or(detected)
}

/// Lowercase and drop separators so `Pro Max`, `pro-max`, and `PRO_MAX` match.
fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{detected_tier, effective_tier};

    #[test]
    fn claude_max_is_tier_1() {
        assert_eq!(detected_tier("claude", Some("max")), 1);
        assert_eq!(detected_tier("claude", Some("max_5x")), 1);
        assert_eq!(detected_tier("claude", Some("max-20x")), 1);
    }

    #[test]
    fn openai_pro_and_promax_are_tier_1() {
        assert_eq!(detected_tier("openai", Some("pro")), 1);
        assert_eq!(detected_tier("openai", Some("promax")), 1);
        assert_eq!(detected_tier("openai", Some("Pro Max")), 1);
    }

    #[test]
    fn standard_plans_are_tier_2() {
        assert_eq!(detected_tier("claude", Some("pro")), 2);
        assert_eq!(detected_tier("openai", Some("plus")), 2);
        assert_eq!(detected_tier("openai", Some("team")), 2);
    }

    #[test]
    fn unknown_plan_defaults_to_tier_2() {
        assert_eq!(detected_tier("openai", Some("future-plan")), 2);
        assert_eq!(detected_tier("claude", None), 2);
    }

    #[test]
    fn free_plans_are_tier_3() {
        assert_eq!(detected_tier("claude", Some("free")), 3);
        assert_eq!(detected_tier("openai", Some("FREE")), 3);
    }

    #[test]
    fn api_key_accounts_default_to_tier_3() {
        assert_eq!(detected_tier("openai-api", None), 3);
        assert_eq!(detected_tier("openai", Some("api-key")), 3);
    }

    #[test]
    fn mixed_case_max_matches_tier_1() {
        assert_eq!(detected_tier("CLAUDE", Some(" mAx ")), 1);
    }

    #[test]
    fn override_beats_detection_upward_and_downward() {
        assert_eq!(effective_tier(1, Some(3)), 3);
        assert_eq!(effective_tier(3, Some(1)), 1);
        assert_eq!(effective_tier(2, None), 2);
    }
}
