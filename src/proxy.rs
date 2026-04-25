use bytes::Bytes;

/// Extract the top-level "model" field from a JSON body.
/// Uses serde_json for correctness — the overhead of partial DOM parse is
/// acceptable because we only parse the top-level object keys.
pub fn extract_model(body: &[u8]) -> Option<String> {
    let val: serde_json::Value = serde_json::from_slice(body).ok()?;

    match val.get("model")? {
        serde_json::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Replace the "model" field in a JSON body. Returns the original body if
/// no replacement is needed (new_model is None).
pub fn replace_model(body: &Bytes, new_model: Option<&str>) -> Bytes {
    match new_model {
        Some(model) => {
            let mut val: serde_json::Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(_) => return body.clone(),
            };
            val["model"] = serde_json::Value::String(model.to_string());
            serde_json::to_vec(&val)
                .map(Bytes::from)
                .unwrap_or_else(|_| body.clone())
        }
        None => body.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_model_simple() {
        let body = br#"{"model":"claude-sonnet-4-20250514","max_tokens":1024,"messages":[]}"#;
        assert_eq!(
            extract_model(body),
            Some("claude-sonnet-4-20250514".to_string())
        );
    }

    #[test]
    fn test_extract_model_with_spaces() {
        let body = br#"{"model" : "claude-opus-4", "max_tokens": 100}"#;
        assert_eq!(extract_model(body), Some("claude-opus-4".to_string()));
    }

    #[test]
    fn test_extract_model_no_model() {
        let body = br#"{"max_tokens":1024,"messages":[]}"#;
        assert_eq!(extract_model(body), None);
    }

    #[test]
    fn test_extract_model_nested_model_ignored() {
        // Top-level "model" should be found, not nested ones
        let body = br#"{"model":"claude-sonnet-4","messages":[{"role":"user","content":"use model-x"}]}"#;
        assert_eq!(extract_model(body), Some("claude-sonnet-4".to_string()));
    }

    #[test]
    fn test_extract_model_invalid_json() {
        let body = br#"not json at all"#;
        assert_eq!(extract_model(body), None);
    }

    #[test]
    fn test_replace_model() {
        let body = Bytes::from(r#"{"model":"old","max_tokens":100}"#);
        let result = replace_model(&body, Some("new-model"));
        let val: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(val["model"], "new-model");
        assert_eq!(val["max_tokens"], 100);
    }

    #[test]
    fn test_replace_model_none() {
        let body = Bytes::from(r#"{"model":"old","max_tokens":100}"#);
        let result = replace_model(&body, None);
        assert_eq!(result, body);
    }
}
