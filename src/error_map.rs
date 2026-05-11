use std::collections::HashSet;

use crate::config::FallbackConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    Deepseek,
    Zhipu,
    Kimi,
    OpenAi,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::Anthropic
    }
}

impl ProviderType {
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "deepseek" => Self::Deepseek,
            "zhipu" => Self::Zhipu,
            "kimi" => Self::Kimi,
            "openai" => Self::OpenAi,
            _ => Self::Anthropic,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Classification {
    Success,
    Retryable {
        error_type: Option<String>,
        description: Option<&'static str>,
    },
    NonRetryable {
        error_type: Option<String>,
        description: Option<&'static str>,
    },
    Fatal,
}

struct CodeRule {
    code: u16,
    error_type_filter: Option<&'static str>,
    description: &'static str,
}

struct ProviderPreset {
    retryable: &'static [CodeRule],
    non_retryable: &'static [CodeRule],
}

// Static presets — each is a named static so we can take a &'static reference.

static ANTHROPIC_PRESET: ProviderPreset = ProviderPreset {
    retryable: &[
        CodeRule { code: 429, error_type_filter: Some("rate_limit_error"), description: "Rate limited (HTTP 429): Anthropic rate limit exceeded" },
        CodeRule { code: 500, error_type_filter: Some("api_error"), description: "API error (HTTP 500): Anthropic server error" },
        CodeRule { code: 529, error_type_filter: Some("overloaded_error"), description: "Overloaded (HTTP 529): Anthropic service overloaded" },
    ],
    non_retryable: &[
        CodeRule { code: 400, error_type_filter: Some("invalid_request_error"), description: "Invalid request (HTTP 400): Bad request to Anthropic" },
        CodeRule { code: 401, error_type_filter: Some("authentication_error"), description: "Auth error (HTTP 401): Invalid Anthropic API key" },
        CodeRule { code: 403, error_type_filter: Some("permission_error"), description: "Permission denied (HTTP 403): Insufficient Anthropic permissions" },
        CodeRule { code: 404, error_type_filter: Some("not_found_error"), description: "Not found (HTTP 404): Anthropic resource not found" },
    ],
};

static DEEPSEEK_PRESET: ProviderPreset = ProviderPreset {
    retryable: &[
        CodeRule { code: 429, error_type_filter: None, description: "Rate limited (HTTP 429): DeepSeek rate limit" },
        CodeRule { code: 500, error_type_filter: None, description: "Server error (HTTP 500): DeepSeek internal error" },
        CodeRule { code: 503, error_type_filter: None, description: "Unavailable (HTTP 503): DeepSeek service unavailable" },
    ],
    non_retryable: &[
        CodeRule { code: 400, error_type_filter: None, description: "Bad request (HTTP 400): Invalid DeepSeek request" },
        CodeRule { code: 401, error_type_filter: None, description: "Auth error (HTTP 401): Invalid DeepSeek API key" },
        CodeRule { code: 402, error_type_filter: None, description: "Payment required (HTTP 402): DeepSeek billing issue" },
        CodeRule { code: 422, error_type_filter: None, description: "Unprocessable (HTTP 422): DeepSeek request validation failed" },
    ],
};

static ZHIPU_PRESET: ProviderPreset = ProviderPreset {
    retryable: &[
        CodeRule { code: 429, error_type_filter: None, description: "Rate limited (HTTP 429): Zhipu rate limit" },
        CodeRule { code: 500, error_type_filter: None, description: "Server error (HTTP 500): Zhipu internal error" },
        CodeRule { code: 503, error_type_filter: None, description: "Unavailable (HTTP 503): Zhipu service unavailable" },
    ],
    non_retryable: &[
        CodeRule { code: 400, error_type_filter: None, description: "Bad request (HTTP 400): Invalid Zhipu request" },
        CodeRule { code: 401, error_type_filter: None, description: "Auth error (HTTP 401): Invalid Zhipu API key" },
        CodeRule { code: 402, error_type_filter: None, description: "Payment required (HTTP 402): Zhipu billing issue" },
        CodeRule { code: 403, error_type_filter: None, description: "Forbidden (HTTP 403): Zhipu access denied" },
        CodeRule { code: 404, error_type_filter: None, description: "Not found (HTTP 404): Zhipu resource not found" },
    ],
};

static KIMI_PRESET: ProviderPreset = ProviderPreset {
    retryable: &[
        CodeRule { code: 429, error_type_filter: Some("rate_limit_reached_error"), description: "Rate limited (HTTP 429): Kimi rate limit reached" },
        CodeRule { code: 500, error_type_filter: Some("server_error"), description: "Server error (HTTP 500): Kimi internal error" },
        CodeRule { code: 503, error_type_filter: Some("engine_overloaded_error"), description: "Overloaded (HTTP 503): Kimi engine overloaded" },
    ],
    non_retryable: &[
        CodeRule { code: 400, error_type_filter: None, description: "Bad request (HTTP 400): Invalid Kimi request" },
        CodeRule { code: 401, error_type_filter: None, description: "Auth error (HTTP 401): Invalid Kimi API key" },
        CodeRule { code: 403, error_type_filter: Some("exceeded_current_quota_error"), description: "Quota exceeded (HTTP 403): Kimi quota exceeded" },
        CodeRule { code: 403, error_type_filter: Some("permission_denied_error"), description: "Permission denied (HTTP 403): Kimi access denied" },
        CodeRule { code: 404, error_type_filter: None, description: "Not found (HTTP 404): Kimi resource not found" },
    ],
};

static OPENAI_PRESET: ProviderPreset = ProviderPreset {
    retryable: &[
        CodeRule { code: 429, error_type_filter: Some("rate_limit_error"), description: "Rate limited (HTTP 429): OpenAI rate limit" },
        CodeRule { code: 500, error_type_filter: Some("server_error"), description: "Server error (HTTP 500): OpenAI internal error" },
        CodeRule { code: 503, error_type_filter: Some("service_unavailable"), description: "Unavailable (HTTP 503): OpenAI service unavailable" },
    ],
    non_retryable: &[
        CodeRule { code: 400, error_type_filter: Some("invalid_request_error"), description: "Bad request (HTTP 400): Invalid OpenAI request" },
        CodeRule { code: 401, error_type_filter: None, description: "Auth error (HTTP 401): Invalid OpenAI API key" },
        CodeRule { code: 403, error_type_filter: Some("permission_error"), description: "Permission denied (HTTP 403): OpenAI access denied" },
        CodeRule { code: 404, error_type_filter: None, description: "Not found (HTTP 404): OpenAI resource not found" },
    ],
};

fn get_preset(provider_type: ProviderType) -> &'static ProviderPreset {
    match provider_type {
        ProviderType::Anthropic => &ANTHROPIC_PRESET,
        ProviderType::Deepseek => &DEEPSEEK_PRESET,
        ProviderType::Zhipu => &ZHIPU_PRESET,
        ProviderType::Kimi => &KIMI_PRESET,
        ProviderType::OpenAi => &OPENAI_PRESET,
    }
}

pub struct ErrorClassifier {
    retryable_codes: HashSet<u16>,
    non_retryable_codes: HashSet<u16>,
    retryable_error_types: HashSet<String>,
    non_retryable_error_types: HashSet<String>,
    preset: &'static ProviderPreset,
    has_global_overrides: bool,
}

impl ErrorClassifier {
    pub fn new(
        global_retryable_codes: &[u16],
        global_retryable_error_types: &[String],
        global_non_retryable_codes: &[u16],
        global_non_retryable_error_types: &[String],
        provider_type: ProviderType,
    ) -> Self {
        let has_global_overrides = !global_retryable_codes.is_empty()
            || !global_retryable_error_types.is_empty()
            || !global_non_retryable_codes.is_empty()
            || !global_non_retryable_error_types.is_empty();

        Self {
            retryable_codes: global_retryable_codes.iter().copied().collect(),
            non_retryable_codes: global_non_retryable_codes.iter().copied().collect(),
            retryable_error_types: global_retryable_error_types.iter().cloned().collect(),
            non_retryable_error_types: global_non_retryable_error_types.iter().cloned().collect(),
            preset: get_preset(provider_type),
            has_global_overrides,
        }
    }

    pub fn from_config(config: &FallbackConfig, provider_type: ProviderType) -> Self {
        Self::new(
            &config.retryable_codes,
            &config.retryable_error_types,
            &config.non_retryable_codes,
            &config.non_retryable_error_types,
            provider_type,
        )
    }

    pub fn classify(&self, status: u16, body: &str) -> Classification {
        // 1. 2xx -> Success
        if (200..300).contains(&status) {
            return Classification::Success;
        }

        // 2. Check global overrides first (when non-empty)
        if self.has_global_overrides {
            if self.retryable_codes.contains(&status) {
                return Classification::Retryable {
                    error_type: None,
                    description: None,
                };
            }
            if self.non_retryable_codes.contains(&status) {
                return Classification::NonRetryable {
                    error_type: None,
                    description: None,
                };
            }
        }

        // 3. Check preset code rules (with optional error_type_filter)
        if let Some(rule) = find_matching_rule(self.preset.retryable, status, body) {
            return Classification::Retryable {
                error_type: extract_error_type(body),
                description: Some(rule.description),
            };
        }
        if let Some(rule) = find_matching_rule(self.preset.non_retryable, status, body) {
            return Classification::NonRetryable {
                error_type: extract_error_type(body),
                description: Some(rule.description),
            };
        }

        // 4. Check global error_type overrides
        let body_error_type = extract_error_type(body);
        if let Some(ref et) = body_error_type {
            if self.retryable_error_types.contains(et) {
                return Classification::Retryable {
                    error_type: body_error_type,
                    description: None,
                };
            }
            if self.non_retryable_error_types.contains(et) {
                return Classification::NonRetryable {
                    error_type: body_error_type,
                    description: None,
                };
            }
        }

        // 5. Non-JSON body fallback: 5xx/429 -> Retryable, other >=400 -> Fatal
        if body_error_type.is_none() {
            if status >= 500 || status == 429 {
                return Classification::Retryable {
                    error_type: None,
                    description: None,
                };
            }
            if status >= 400 {
                return Classification::Fatal;
            }
        }

        // 6. Default: unparseable or unexpected status
        if status >= 500 {
            Classification::Retryable {
                error_type: body_error_type,
                description: None,
            }
        } else if status >= 400 {
            Classification::Fatal
        } else {
            Classification::Success
        }
    }
}

fn find_matching_rule(rules: &'static [CodeRule], status: u16, body: &str) -> Option<&'static CodeRule> {
    for rule in rules {
        if rule.code != status {
            continue;
        }
        match rule.error_type_filter {
            None => return Some(rule),
            Some(filter) => {
                let body_type = extract_error_type(body);
                match body_type {
                    Some(ref bt) if bt == filter => return Some(rule),
                    Some(_) => continue,
                    None => continue, // non-JSON body: can't verify error_type, skip rule
                }
            }
        }
    }
    None
}

fn extract_error_type(body: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(body).ok()?;
    val.get("error")
        .and_then(|e| e.get("type"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier_for(provider_type: ProviderType) -> ErrorClassifier {
        ErrorClassifier::new(&[], &[], &[], &[], provider_type)
    }

    #[test]
    fn test_success_2xx() {
        let c = classifier_for(ProviderType::Anthropic);
        assert_eq!(c.classify(200, ""), Classification::Success);
        assert_eq!(c.classify(201, "{}"), Classification::Success);
        assert_eq!(c.classify(299, ""), Classification::Success);
    }

    #[test]
    fn test_anthropic_retryable_429() {
        let c = classifier_for(ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"too many"}}"#;
        match c.classify(429, body) {
            Classification::Retryable { description, .. } => {
                assert!(description.is_some());
            }
            other => panic!("expected Retryable, got {:?}", other),
        }
    }

    #[test]
    fn test_anthropic_retryable_529() {
        let c = classifier_for(ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"overloaded"}}"#;
        assert!(matches!(c.classify(529, body), Classification::Retryable { .. }));
    }

    #[test]
    fn test_anthropic_non_retryable_400() {
        let c = classifier_for(ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad"}}"#;
        match c.classify(400, body) {
            Classification::NonRetryable { description, .. } => {
                assert!(description.is_some());
            }
            other => panic!("expected NonRetryable, got {:?}", other),
        }
    }

    #[test]
    fn test_anthropic_non_retryable_401() {
        let c = classifier_for(ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"bad key"}}"#;
        assert!(matches!(c.classify(401, body), Classification::NonRetryable { .. }));
    }

    #[test]
    fn test_non_json_body_5xx_fallback_retryable() {
        let c = classifier_for(ProviderType::Anthropic);
        assert!(matches!(c.classify(502, "<html>Bad Gateway</html>"), Classification::Retryable { .. }));
        assert!(matches!(c.classify(500, "Internal Server Error"), Classification::Retryable { .. }));
    }

    #[test]
    fn test_non_json_body_4xx_fallback_fatal() {
        let c = classifier_for(ProviderType::Anthropic);
        assert!(matches!(c.classify(405, "Method Not Allowed"), Classification::Fatal));
    }

    #[test]
    fn test_global_overrides_take_precedence() {
        let c = ErrorClassifier::new(
            &[418],
            &[],
            &[451],
            &[],
            ProviderType::Anthropic,
        );
        assert!(matches!(c.classify(418, "{}"), Classification::Retryable { .. }));
        assert!(matches!(c.classify(451, "{}"), Classification::NonRetryable { .. }));
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad"}}"#;
        assert!(matches!(c.classify(400, body), Classification::NonRetryable { .. }));
    }

    #[test]
    fn test_global_error_type_overrides() {
        let c = ErrorClassifier::new(
            &[],
            &["custom_retryable".to_string()],
            &[],
            &["custom_fatal".to_string()],
            ProviderType::Anthropic,
        );
        let body_retry = r#"{"type":"error","error":{"type":"custom_retryable","message":"x"}}"#;
        assert!(matches!(c.classify(500, body_retry), Classification::Retryable { .. }));

        let body_fatal = r#"{"type":"error","error":{"type":"custom_fatal","message":"x"}}"#;
        assert!(matches!(c.classify(500, body_fatal), Classification::NonRetryable { .. }));
    }
}
