use std::path::Path;

use path_slash::PathExt;

pub fn join_http_paths(left: &str, right: &str) -> String {
    let normalized_left = normalize_http_path(left);
    let normalized_right = normalize_http_path(right);
    if normalized_left == "/" {
        return normalized_right;
    }
    let joined = format!(
        "{}/{}",
        normalized_left.trim_end_matches('/'),
        normalized_right.trim_start_matches('/')
    );
    normalize_http_path(&joined)
}

pub fn normalize_fs_path(value: &str) -> String {
    dunce::canonicalize(Path::new(value))
        .ok()
        .and_then(|p| p.to_slash().map(|s| s.to_string()))
        .unwrap_or_else(|| value.to_string())
}

pub fn normalize_http_path(value: &str) -> String {
    let without_query = value.split('?').next().unwrap_or(value);
    let without_hash = without_query.split('#').next().unwrap_or(without_query);
    let without_origin = strip_http_origin(without_hash);

    let with_leading = if without_origin.starts_with('/') {
        without_origin.to_string()
    } else {
        format!("/{without_origin}")
    };
    collapse_slashes(&with_leading)
}

pub fn expand_http_search_tokens(http_path: &str) -> Vec<String> {
    let normalized = normalize_http_path(http_path);
    let mut tokens = vec![normalized.clone()];

    let without_params = remove_colon_params(&normalized);
    tokens.push(without_params.clone());
    tokens.push(without_params.trim_end_matches('/').to_string());

    if let Some(last_slash) = normalized.rfind('/')
        && last_slash > 0
    {
        let base = normalized[..last_slash].to_string();
        tokens.push(base.clone());
        tokens.push(format!("{base}/"));
    };

    tokens.into_iter().filter(|t| !t.is_empty()).collect()
}

fn strip_http_origin(input: &str) -> &str {
    let is_http = input.starts_with("http://") || input.starts_with("https://");
    if !is_http {
        return input;
    }
    let Some(after_scheme) = input.find("://").map(|idx| idx + 3) else {
        return input;
    };
    let rest = &input[after_scheme..];
    let Some(first_slash) = rest.find('/') else {
        return "/";
    };
    &rest[first_slash..]
}

fn collapse_slashes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_slash = false;
    for ch in input.chars() {
        if ch == '/' {
            if last_was_slash {
                continue;
            }
            last_was_slash = true;
            out.push('/');
            continue;
        }
        last_was_slash = false;
        out.push(ch);
    }
    if out.is_empty() { "/".to_string() } else { out }
}

fn remove_colon_params(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut is_in_param = false;
    for ch in input.chars() {
        if is_in_param {
            if ch == '/' {
                is_in_param = false;
                out.push('/');
            }
            continue;
        }
        if ch == ':' {
            is_in_param = true;
            continue;
        }
        out.push(ch);
    }
    collapse_slashes(&out)
}
