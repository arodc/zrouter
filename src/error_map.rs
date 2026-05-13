// ---------------------------------------------------------------------------
// Provider type
// ---------------------------------------------------------------------------

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

    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Deepseek => "deepseek",
            Self::Zhipu => "zhipu",
            Self::Kimi => "kimi",
            Self::OpenAi => "openai",
        }
    }
}

// ---------------------------------------------------------------------------
// TOML deserialization types (public)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CodeRuleDef {
    pub code: u16,
    #[serde(default, rename = "error_type")]
    pub error_type_filter: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderPresetDef {
    #[serde(default)]
    pub retryable: Vec<CodeRuleDef>,
    #[serde(default)]
    pub non_retryable: Vec<CodeRuleDef>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorCodesFile {
    pub default: Option<ProviderPresetDef>,
    pub anthropic: Option<ProviderPresetDef>,
    pub deepseek: Option<ProviderPresetDef>,
    pub zhipu: Option<ProviderPresetDef>,
    pub kimi: Option<ProviderPresetDef>,
    pub openai: Option<ProviderPresetDef>,
}

impl ErrorCodesFile {
    pub fn load(
        path: Option<&str>,
        config_dir: &std::path::Path,
    ) -> Option<Self> {
        let Some(path) = path else {
            return None;
        };
        let full_path = if std::path::Path::new(path).is_absolute() {
            std::path::PathBuf::from(path)
        } else {
            config_dir.join(path)
        };
        if !full_path.exists() {
            tracing::warn!("error_codes_file '{}' not found, error descriptions will be unavailable", full_path.display());
            return None;
        }
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("failed to read error_codes_file '{}': {}, error descriptions will be unavailable", full_path.display(), e);
                return None;
            }
        };
        match toml::from_str::<ErrorCodesFile>(&content) {
            Ok(codes) => Some(codes),
            Err(e) => {
                tracing::warn!("failed to parse error_codes_file '{}': {}, error descriptions will be unavailable", full_path.display(), e);
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Classification result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Classification {
    Success,
    Retryable {
        error_type: Option<String>,
        description: Option<String>,
    },
    NonRetryable {
        error_type: Option<String>,
        description: Option<String>,
    },
    Fatal,
}

// ---------------------------------------------------------------------------
// Internal types (owned)
// ---------------------------------------------------------------------------

struct CodeRule {
    code: u16,
    error_type_filter: Option<String>,
    description: String,
}

struct ProviderPreset {
    retryable: Vec<CodeRule>,
    non_retryable: Vec<CodeRule>,
}

// ---------------------------------------------------------------------------
// Resolve preset from ErrorCodesFile (empty when no config)
// ---------------------------------------------------------------------------

fn resolve_preset(
    provider_type: ProviderType,
    error_codes: Option<&ErrorCodesFile>,
) -> ProviderPreset {
    let Some(codes) = error_codes else {
        return ProviderPreset { retryable: vec![], non_retryable: vec![] };
    };
    let def = match provider_type {
        ProviderType::Anthropic => codes.anthropic.as_ref(),
        ProviderType::Deepseek => codes.deepseek.as_ref(),
        ProviderType::Zhipu => codes.zhipu.as_ref(),
        ProviderType::Kimi => codes.kimi.as_ref(),
        ProviderType::OpenAi => codes.openai.as_ref(),
    };
    match def {
        Some(d) => preset_def_to_owned(d),
        None => codes
            .default
            .as_ref()
            .map(preset_def_to_owned)
            .unwrap_or(ProviderPreset {
                retryable: vec![],
                non_retryable: vec![],
            }),
    }
}

fn preset_def_to_owned(def: &ProviderPresetDef) -> ProviderPreset {
    ProviderPreset {
        retryable: def
            .retryable
            .iter()
            .map(|r| CodeRule {
                code: r.code,
                error_type_filter: r.error_type_filter.clone(),
                description: r.description.clone(),
            })
            .collect(),
        non_retryable: def
            .non_retryable
            .iter()
            .map(|r| CodeRule {
                code: r.code,
                error_type_filter: r.error_type_filter.clone(),
                description: r.description.clone(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// ErrorClassifier
// ---------------------------------------------------------------------------

pub struct ErrorClassifier {
    preset: ProviderPreset,
}

impl ErrorClassifier {
    pub fn new(
        provider_type: ProviderType,
        error_codes: Option<&ErrorCodesFile>,
    ) -> Self {
        Self {
            preset: resolve_preset(provider_type, error_codes),
        }
    }

    pub fn classify(&self, status: u16, body: &str) -> Classification {
        // 1. 2xx -> Success
        if (200..300).contains(&status) {
            return Classification::Success;
        }

        // 2. Check preset code rules (with optional error_type_filter)
        if let Some(rule) = find_matching_rule(&self.preset.retryable, status, body) {
            return Classification::Retryable {
                error_type: extract_error_type(body),
                description: Some(rule.description.clone()),
            };
        }
        if let Some(rule) = find_matching_rule(&self.preset.non_retryable, status, body) {
            return Classification::NonRetryable {
                error_type: extract_error_type(body),
                description: Some(rule.description.clone()),
            };
        }

        // 3. No matching rule -> Fatal
        Classification::Fatal
    }
}

fn find_matching_rule<'a>(rules: &'a [CodeRule], status: u16, body: &str) -> Option<&'a CodeRule> {
    for rule in rules {
        if rule.code != status {
            continue;
        }
        match &rule.error_type_filter {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier_for(provider_type: ProviderType) -> ErrorClassifier {
        ErrorClassifier::new(provider_type, None)
    }

    fn classifier_with_codes(toml_str: &str, provider_type: ProviderType) -> ErrorClassifier {
        let codes: ErrorCodesFile = toml::from_str(toml_str).unwrap();
        ErrorClassifier::new(provider_type, Some(&codes))
    }

    #[test]
    fn test_success_2xx() {
        let c = classifier_for(ProviderType::Anthropic);
        assert_eq!(c.classify(200, ""), Classification::Success);
        assert_eq!(c.classify(201, "{}"), Classification::Success);
        assert_eq!(c.classify(299, ""), Classification::Success);
    }

    #[test]
    fn test_no_config_no_rules() {
        let c = classifier_for(ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"too many"}}"#;
        // No config at all → no rules → Fatal
        assert!(matches!(c.classify(429, body), Classification::Fatal));
    }

    #[test]
    fn test_no_config_5xx_fatal() {
        let c = classifier_for(ProviderType::Anthropic);
        // No config → no rules → Fatal
        assert!(matches!(
            c.classify(502, "<html>Bad Gateway</html>"),
            Classification::Fatal
        ));
        assert!(matches!(
            c.classify(500, "Internal Server Error"),
            Classification::Fatal
        ));
    }

    #[test]
    fn test_no_config_4xx_fatal() {
        let c = classifier_for(ProviderType::Anthropic);
        assert!(matches!(
            c.classify(405, "Method Not Allowed"),
            Classification::Fatal
        ));
    }

    // --- Tests with TOML error codes config ---

    #[test]
    fn test_anthropic_retryable_429_with_config() {
        let toml = r#"
[anthropic]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Rate limit (HTTP 429): request exceeded rate limit — reduce frequency" },
]
non_retryable = []
"#;
        let c = classifier_with_codes(toml, ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"too many"}}"#;
        match c.classify(429, body) {
            Classification::Retryable { description, .. } => {
                assert!(description.is_some());
                assert!(description.unwrap().contains("Rate limit"));
            }
            other => panic!("expected Retryable, got {:?}", other),
        }
    }

    #[test]
    fn test_anthropic_non_retryable_400_with_config() {
        let toml = r#"
[anthropic]
retryable = []
non_retryable = [
  { code = 400, error_type = "invalid_request_error", description = "Invalid request (HTTP 400): bad format" },
]
"#;
        let c = classifier_with_codes(toml, ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad"}}"#;
        match c.classify(400, body) {
            Classification::NonRetryable { description, .. } => {
                assert!(description.is_some());
                assert!(description.unwrap().contains("Invalid request"));
            }
            other => panic!("expected NonRetryable, got {:?}", other),
        }
    }

    #[test]
    fn test_kimi_engine_overloaded_429_with_config() {
        let toml = r#"
[kimi]
retryable = [
  { code = 429, error_type = "engine_overloaded_error", description = "Overloaded (HTTP 429): engine overloaded — try again later" },
]
non_retryable = []
"#;
        let c = classifier_with_codes(toml, ProviderType::Kimi);
        let body = r#"{"error":{"type":"engine_overloaded_error","message":"overloaded"}}"#;
        match c.classify(429, body) {
            Classification::Retryable { description, .. } => {
                assert!(description.is_some());
                assert!(description.unwrap().contains("Overloaded"));
            }
            other => panic!(
                "expected Retryable for 429 engine_overloaded_error, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_zhipu_file_too_large_435_with_config() {
        let toml = r#"
[zhipu]
retryable = []
non_retryable = [
  { code = 435, description = "File too large (HTTP 435): file exceeds 100 MB limit" },
]
"#;
        let c = classifier_with_codes(toml, ProviderType::Zhipu);
        match c.classify(435, r#"{"error":{"code":"435","message":"too big"}}"#) {
            Classification::NonRetryable { description, .. } => {
                assert!(description.is_some());
                assert!(description.unwrap().contains("100 MB"));
            }
            other => panic!("expected NonRetryable for 435, got {:?}", other),
        }
    }

    #[test]
    fn test_deepseek_422_non_retryable_with_config() {
        let toml = r#"
[deepseek]
retryable = []
non_retryable = [
  { code = 422, description = "Unprocessable (HTTP 422): invalid parameters" },
]
"#;
        let c = classifier_with_codes(toml, ProviderType::Deepseek);
        match c.classify(422, r#"{"error":{"message":"bad param"}}"#) {
            Classification::NonRetryable { description, .. } => {
                assert!(description.is_some());
            }
            other => panic!("expected NonRetryable, got {:?}", other),
        }
    }

    // --- TOML parsing and config tests ---

    #[test]
    fn test_error_codes_toml_parse() {
        let toml = r#"
[anthropic]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Rate limit" },
]
non_retryable = [
  { code = 401, error_type = "authentication_error", description = "Auth error" },
]
"#;
        let parsed: ErrorCodesFile = toml::from_str(toml).unwrap();
        assert!(parsed.anthropic.is_some());
        assert!(parsed.deepseek.is_none());
        let a = parsed.anthropic.unwrap();
        assert_eq!(a.retryable.len(), 1);
        assert_eq!(a.non_retryable.len(), 1);
    }

    #[test]
    fn test_error_codes_override() {
        let toml = r#"
[anthropic]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Custom rate limit desc" },
]
non_retryable = []
"#;
        let codes: ErrorCodesFile = toml::from_str(toml).unwrap();
        let c = ErrorClassifier::new(
            ProviderType::Anthropic,
            Some(&codes),
        );
        let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"x"}}"#;
        match c.classify(429, body) {
            Classification::Retryable { description, .. } => {
                assert_eq!(description, Some("Custom rate limit desc".to_string()));
            }
            other => panic!("expected Retryable, got {:?}", other),
        }
    }

    #[test]
    fn test_error_codes_partial_config() {
        let toml = r#"
[anthropic]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Custom" },
]
non_retryable = []
"#;
        let codes: ErrorCodesFile = toml::from_str(toml).unwrap();
        // DeepSeek not in file, no [default] → no rules → Fatal
        let c = ErrorClassifier::new(
            ProviderType::Deepseek,
            Some(&codes),
        );
        assert!(matches!(c.classify(429, "{}"), Classification::Fatal));
    }

    #[test]
    fn test_no_config_5xx_with_json_body_fatal() {
        let c = classifier_for(ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"timeout_error","message":"timed out"}}"#;
        // No config → no rules → Fatal
        assert!(matches!(c.classify(504, body), Classification::Fatal));
    }

    #[test]
    fn test_no_config_4xx_with_json_body_fatal() {
        let c = classifier_for(ProviderType::Anthropic);
        let body = r#"{"type":"error","error":{"type":"billing_error","message":"payment"}}"#;
        // No config → no rules → Fatal
        assert!(matches!(c.classify(402, body), Classification::Fatal));
    }

    // --- [default] section tests ---

    #[test]
    fn test_default_section_used_as_fallback() {
        let toml = r#"
[default]
retryable = [
  { code = 429, description = "Rate limited" },
  { code = 500, description = "Server error" },
]
non_retryable = [
  { code = 400, description = "Bad request" },
]
"#;
        let codes: ErrorCodesFile = toml::from_str(toml).unwrap();
        // DeepSeek not listed → uses [default]
        let c = ErrorClassifier::new(ProviderType::Deepseek, Some(&codes));
        match c.classify(429, "{}") {
            Classification::Retryable { description, .. } => {
                assert_eq!(description, Some("Rate limited".to_string()));
            }
            other => panic!("expected Retryable, got {:?}", other),
        }
        match c.classify(500, "error") {
            Classification::Retryable { description, .. } => {
                assert_eq!(description, Some("Server error".to_string()));
            }
            other => panic!("expected Retryable, got {:?}", other),
        }
        match c.classify(400, "{}") {
            Classification::NonRetryable { description, .. } => {
                assert_eq!(description, Some("Bad request".to_string()));
            }
            other => panic!("expected NonRetryable, got {:?}", other),
        }
        // 404 not in [default] → Fatal
        assert!(matches!(c.classify(404, "{}"), Classification::Fatal));
    }

    #[test]
    fn test_default_section_absent() {
        let toml = r#"
[anthropic]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Custom" },
]
non_retryable = []
"#;
        let codes: ErrorCodesFile = toml::from_str(toml).unwrap();
        assert!(codes.default.is_none());
        // DeepSeek not in file, no [default] → no rules → Fatal
        let c = ErrorClassifier::new(ProviderType::Deepseek, Some(&codes));
        assert!(matches!(c.classify(429, "{}"), Classification::Fatal));
    }

    #[test]
    fn test_provider_specific_overrides_default() {
        let toml = r#"
[default]
retryable = [
  { code = 429, description = "Generic rate limit" },
]
non_retryable = []

[anthropic]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Anthropic rate limit" },
]
non_retryable = []
"#;
        let codes: ErrorCodesFile = toml::from_str(toml).unwrap();
        // Anthropic has its own section → uses it, not [default]
        let c = ErrorClassifier::new(ProviderType::Anthropic, Some(&codes));
        let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"x"}}"#;
        match c.classify(429, body) {
            Classification::Retryable { description, .. } => {
                assert_eq!(description, Some("Anthropic rate limit".to_string()));
            }
            other => panic!("expected Retryable with Anthropic desc, got {:?}", other),
        }
    }

    #[test]
    fn test_default_section_parsed() {
        let toml = r#"
[default]
retryable = [{ code = 429, description = "Rate limited" }]
non_retryable = [{ code = 401, description = "Unauthorized" }]
"#;
        let parsed: ErrorCodesFile = toml::from_str(toml).unwrap();
        let d = parsed.default.unwrap();
        assert_eq!(d.retryable.len(), 1);
        assert_eq!(d.non_retryable.len(), 1);
        assert_eq!(d.retryable[0].code, 429);
        assert_eq!(d.non_retryable[0].code, 401);
    }

    #[test]
    fn test_error_type_filter_mismatch_skips_rule() {
        let toml = r#"
[anthropic]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Rate limit" },
]
non_retryable = []
"#;
        let codes: ErrorCodesFile = toml::from_str(toml).unwrap();
        let c = ErrorClassifier::new(ProviderType::Anthropic, Some(&codes));
        // Body has overloaded_error, rule filters on rate_limit_error → no match → Fatal
        let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"overloaded"}}"#;
        assert!(matches!(c.classify(429, body), Classification::Fatal));
    }

    #[test]
    fn test_error_type_filter_non_json_body_skips_rule() {
        let toml = r#"
[default]
retryable = [
  { code = 429, error_type = "rate_limit_error", description = "Rate limit" },
]
non_retryable = []
"#;
        let codes: ErrorCodesFile = toml::from_str(toml).unwrap();
        let c = ErrorClassifier::new(ProviderType::Deepseek, Some(&codes));
        // Non-JSON body can't verify error_type filter → rule skipped → Fatal
        assert!(matches!(c.classify(429, "upstream timeout"), Classification::Fatal));
    }

    #[test]
    fn test_error_codes_toml_unknown_key_rejected() {
        let toml = r#"
[anthopi]
retryable = []
non_retryable = []
"#;
        assert!(toml::from_str::<ErrorCodesFile>(toml).is_err());
    }

    #[test]
    fn test_provider_type_as_str_roundtrip() {
        for pt in [
            ProviderType::Anthropic,
            ProviderType::Deepseek,
            ProviderType::Zhipu,
            ProviderType::Kimi,
            ProviderType::OpenAi,
        ] {
            assert_eq!(ProviderType::from_str_lossy(pt.as_str()), pt);
        }
    }
}
