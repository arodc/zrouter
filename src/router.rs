use crate::config::RouteConfig;

#[derive(Debug, Clone, PartialEq)]
enum MatchKind {
    Exact,
    Prefix,
    Default,
}

fn classify_pattern(pattern: &str) -> MatchKind {
    if pattern == "default" {
        MatchKind::Default
    } else if pattern.ends_with('*') {
        MatchKind::Prefix
    } else {
        MatchKind::Exact
    }
}

fn matches_pattern(pattern: &str, model: &str) -> bool {
    if pattern == "default" {
        return true;
    }
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        model.starts_with(prefix)
    } else {
        model == pattern
    }
}

pub fn resolve_route<'a>(
    routes: &'a [RouteConfig],
    model: &str,
) -> Option<&'a RouteConfig> {
    let mut default_route: Option<&'a RouteConfig> = None;
    let mut prefix_match: Option<&'a RouteConfig> = None;

    for route in routes {
        let kind = classify_pattern(&route.model);
        match kind {
            MatchKind::Exact if matches_pattern(&route.model, model) => {
                return Some(route);
            }
            MatchKind::Prefix if matches_pattern(&route.model, model) && prefix_match.is_none() => {
                prefix_match = Some(route);
            }
            MatchKind::Default if default_route.is_none() => {
                default_route = Some(route);
            }
            _ => {}
        }
    }

    prefix_match.or(default_route)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RouteStep;

    fn make_route(model: &str, providers: &[&str]) -> RouteConfig {
        RouteConfig {
            model: model.to_string(),
            steps: providers
                .iter()
                .map(|&p| RouteStep {
                    provider: p.to_string(),
                    model: None,
                })
                .collect(),
        }
    }

    #[test]
    fn test_exact_match_takes_priority() {
        let routes = vec![
            make_route("claude-opus-4-20250514", &["anthropic"]),
            make_route("claude-opus-*", &["openrouter"]),
            make_route("default", &["closeai"]),
        ];

        let result = resolve_route(&routes, "claude-opus-4-20250514");
        assert_eq!(result.unwrap().steps[0].provider, "anthropic");
    }

    #[test]
    fn test_prefix_match() {
        let routes = vec![
            make_route("claude-opus-*", &["anthropic"]),
            make_route("default", &["closeai"]),
        ];

        let result = resolve_route(&routes, "claude-opus-4-20260101");
        assert_eq!(result.unwrap().steps[0].provider, "anthropic");
    }

    #[test]
    fn test_default_fallback() {
        let routes = vec![
            make_route("claude-opus-*", &["anthropic"]),
            make_route("default", &["closeai"]),
        ];

        let result = resolve_route(&routes, "some-random-model");
        assert_eq!(result.unwrap().steps[0].provider, "closeai");
    }

    #[test]
    fn test_no_match_no_default() {
        let routes = vec![make_route("claude-opus-*", &["anthropic"])];
        assert!(resolve_route(&routes, "some-random-model").is_none());
    }

    #[test]
    fn test_prefix_first_match_wins() {
        let routes = vec![
            make_route("claude-*", &["anthropic"]),
            make_route("claude-opus-*", &["openrouter"]),
        ];

        let result = resolve_route(&routes, "claude-opus-4-20250514");
        assert_eq!(result.unwrap().steps[0].provider, "anthropic");
    }
}
