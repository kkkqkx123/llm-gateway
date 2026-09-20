use crate::config::types::{ThinkingConfig, ThinkingLevel};
use regex::Regex;

/// suffix resolver
pub struct SuffixParser;

impl SuffixParser {
    /// Extract suffixes from model names
    /// Format: model-name(config) or model-name-(config)
    pub fn extract_suffix(model: &str) -> Option<String> {
        let re = Regex::new(r"\(([^)]+)\)$").unwrap();
        if let Some(caps) = re.captures(model) {
            caps.get(1).map(|m| m.as_str().to_string())
        } else {
            None
        }
    }

    /// Resolving Suffix to Thinking Configuration
    pub fn parse_suffix(suffix: &str) -> Option<ThinkingConfig> {
        let config = Self::parse_suffix_value(suffix);
        if config.enabled {
            Some(config)
        } else {
            None
        }
    }

    /// Resolve suffix values
    fn parse_suffix_value(suffix: &str) -> ThinkingConfig {
        let trimmed = suffix.trim();

        if trimmed.eq_ignore_ascii_case("none") {
            return ThinkingConfig {
                enabled: false,
                budget_tokens: Some(0),
                level: Some(ThinkingLevel::None),
                effort: None,
            };
        }

        if trimmed.eq_ignore_ascii_case("auto") {
            return ThinkingConfig {
                enabled: true,
                budget_tokens: None,
                level: Some(ThinkingLevel::Auto),
                effort: None,
            };
        }

        if trimmed.starts_with('-') || trimmed.eq_ignore_ascii_case("-1") {
            return ThinkingConfig {
                enabled: false,
                budget_tokens: Some(0),
                level: Some(ThinkingLevel::None),
                effort: None,
            };
        }

        // Attempt to resolve to level name
        if let Some(level) = Self::parse_level(trimmed) {
            return ThinkingConfig {
                enabled: true,
                budget_tokens: level.to_budget(),
                level: Some(level),
                effort: Some(level.as_str().to_string()),
            };
        }

        // Try to parse it into a number (budget)
        if let Ok(budget) = trimmed.parse::<u32>() {
            return ThinkingConfig {
                enabled: true,
                budget_tokens: Some(budget),
                level: Some(ThinkingLevel::from_budget(budget)),
                effort: None,
            };
        }

        ThinkingConfig::default()
    }

    /// Resolution level name
    fn parse_level(s: &str) -> Option<ThinkingLevel> {
        match s.to_lowercase().as_str() {
            "minimal" => Some(ThinkingLevel::Minimal),
            "low" => Some(ThinkingLevel::Low),
            "medium" => Some(ThinkingLevel::Medium),
            "high" => Some(ThinkingLevel::High),
            "xhigh" | "extra-high" | "x-high" | "extra" => Some(ThinkingLevel::XHigh),
            _ => None,
        }
    }

    /// Extract thinking configuration from model name
    pub fn extract_from_model(model: &str) -> Option<ThinkingConfig> {
        if let Some(suffix) = Self::extract_suffix(model) {
            Self::parse_suffix(&suffix)
        } else {
            None
        }
    }

    /// Remove suffix from model name
    pub fn strip_suffix(model: &str) -> String {
        let re = Regex::new(r"\s*\([^)]+\)$").unwrap();
        re.replace(model, "").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_suffix_basic() {
        assert_eq!(
            SuffixParser::extract_suffix("gemini-pro(8192)"),
            Some("8192".to_string())
        );
        assert_eq!(
            SuffixParser::extract_suffix("gemini-pro(high)"),
            Some("high".to_string())
        );
        assert_eq!(SuffixParser::extract_suffix("gemini-pro"), None);
    }

    #[test]
    fn test_parse_suffix_none() {
        assert!(SuffixParser::parse_suffix("none").is_none());
        assert!(SuffixParser::parse_suffix("NONE").is_none());
    }

    #[test]
    fn test_parse_suffix_value_none() {
        let config = SuffixParser::parse_suffix_value("none");
        assert!(!config.enabled);
        assert_eq!(config.budget_tokens, Some(0));
        assert_eq!(config.level, Some(ThinkingLevel::None));
    }

    #[test]
    fn test_parse_suffix_auto() {
        let config = SuffixParser::parse_suffix("auto").unwrap();
        assert!(config.enabled);
        assert!(config.budget_tokens.is_none());
        assert_eq!(config.level, Some(ThinkingLevel::Auto));
    }

    #[test]
    fn test_parse_suffix_minus_one() {
        assert!(SuffixParser::parse_suffix("-1").is_none());
    }

    #[test]
    fn test_parse_suffix_value_minus_one() {
        let config = SuffixParser::parse_suffix_value("-1");
        assert!(!config.enabled);
        assert_eq!(config.budget_tokens, Some(0));
        assert_eq!(config.level, Some(ThinkingLevel::None));
    }

    #[test]
    fn test_parse_suffix_level() {
        let config = SuffixParser::parse_suffix("low").unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::Low));
        assert_eq!(config.budget_tokens, Some(4096));
    }

    #[test]
    fn test_parse_suffix_budget() {
        let config = SuffixParser::parse_suffix("8192").unwrap();
        assert!(config.enabled);
        assert_eq!(config.budget_tokens, Some(8192));
        assert_eq!(config.level, Some(ThinkingLevel::Medium));
    }

    #[test]
    fn test_extract_from_model() {
        let config = SuffixParser::extract_from_model("gemini-pro(8192)");
        assert!(config.is_some());
        assert_eq!(config.unwrap().budget_tokens, Some(8192));

        let config = SuffixParser::extract_from_model("gemini-pro(high)");
        assert!(config.is_some());
        assert_eq!(config.unwrap().level, Some(ThinkingLevel::High));

        assert!(SuffixParser::extract_from_model("gemini-pro").is_none());
    }

    #[test]
    fn test_strip_suffix() {
        assert_eq!(SuffixParser::strip_suffix("gemini-pro(8192)"), "gemini-pro");
        assert_eq!(
            SuffixParser::strip_suffix("gemini-pro (high)"),
            "gemini-pro"
        );
        assert_eq!(SuffixParser::strip_suffix("gemini-pro"), "gemini-pro");
    }

    #[test]
    fn test_xhigh_variations() {
        assert_eq!(
            SuffixParser::parse_suffix("xhigh").unwrap().level,
            Some(ThinkingLevel::XHigh)
        );
        assert_eq!(
            SuffixParser::parse_suffix("extra-high").unwrap().level,
            Some(ThinkingLevel::XHigh)
        );
        assert_eq!(
            SuffixParser::parse_suffix("x-high").unwrap().level,
            Some(ThinkingLevel::XHigh)
        );
        assert_eq!(
            SuffixParser::parse_suffix("extra").unwrap().level,
            Some(ThinkingLevel::XHigh)
        );
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(
            SuffixParser::parse_suffix("Auto").unwrap().level,
            Some(ThinkingLevel::Auto)
        );
        assert_eq!(
            SuffixParser::parse_suffix("HIGH").unwrap().level,
            Some(ThinkingLevel::High)
        );
        assert_eq!(
            SuffixParser::parse_suffix("low").unwrap().level,
            Some(ThinkingLevel::Low)
        );
    }

    #[test]
    fn test_none_suffix_returns_none() {
        assert!(SuffixParser::parse_suffix("none").is_none());
        assert!(SuffixParser::parse_suffix("NONE").is_none());
    }

    #[test]
    fn test_parse_suffix_returns_config() {
        let config = SuffixParser::parse_suffix("low").unwrap();
        assert!(config.enabled);
        assert_eq!(config.budget_tokens, Some(4096));
        assert_eq!(config.level, Some(ThinkingLevel::Low));
    }
}
