//! "Open in ConzaCut" links: what bundle URL a link points at. Both
//! editors read links by these rules (the web app's
//! `apps/web/src/project/open-link.ts`, checked against the same cases in
//! `fixtures/open-links/cases.json`):
//!
//! - a bundle URL itself: `https://…`, or `http://` only on this computer
//!   (localhost, 127.0.0.1, [::1]);
//! - a ConzaCut open link wrapping one: `https://<app>/open?bundle=<url>`
//!   or `conzacut://open?bundle=<url>` (one level; a wrapped open link is
//!   refused).
//!
//! Anything with credentials, spaces, control characters or a malformed
//! host is refused. The URL is returned as given (trimmed), not rewritten.

const MAX_LINK_LENGTH: usize = 4096;

struct Parts<'a> {
    scheme: &'a str,
    authority: &'a str,
    path: &'a str,
    query: Option<&'a str>,
}

fn split(url: &str) -> Option<Parts<'_>> {
    let (scheme, rest) = url.split_once("://")?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let after = &rest[authority_end..];
    let path_end = after.find(['?', '#']).unwrap_or(after.len());
    let path = &after[..path_end];
    let query = after[path_end..].strip_prefix('?').map(|query| query.split('#').next().unwrap_or(""));
    Some(Parts { scheme, authority, path, query })
}

fn valid_host(authority: &str) -> Option<&str> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (inside, after) = rest.split_once(']')?;
        if inside.is_empty() || !inside.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
            return None;
        }
        let port = if after.is_empty() {
            None
        } else {
            Some(after.strip_prefix(':')?)
        };
        (&authority[..inside.len() + 2], port)
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if host.is_empty() || (!host.starts_with('[') && !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')) {
        return None;
    }
    if let Some(port) = port
        && (port.is_empty() || port.len() > 5 || !port.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    Some(host)
}

/// A bundle URL itself (no unwrapping).
fn direct(url: &str) -> Option<String> {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let parts = split(url)?;
    if parts.authority.contains('@') {
        return None;
    }
    let host = valid_host(parts.authority)?.to_ascii_lowercase();
    match parts.scheme.to_ascii_lowercase().as_str() {
        "https" => Some(url.to_owned()),
        "http" if matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]") => Some(url.to_owned()),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = value.get(index + 1..index + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn is_open_link(url: &str) -> Option<Option<&str>> {
    let parts = split(url)?;
    let wrapper = parts.scheme.eq_ignore_ascii_case("conzacut") || parts.path.ends_with("/open");
    if !wrapper {
        return None;
    }
    let bundle = parts
        .query
        .unwrap_or("")
        .split('&')
        .find_map(|pair| pair.strip_prefix("bundle="));
    Some(bundle)
}

/// The bundle URL a link points at, or `None` when the link is refused.
pub fn bundle_url_from_link(link: &str) -> Option<String> {
    let link = link.trim();
    if link.is_empty() || link.len() > MAX_LINK_LENGTH {
        return None;
    }
    match is_open_link(link) {
        Some(Some(encoded)) => {
            let inner = percent_decode(encoded)?;
            if is_open_link(&inner).is_some() {
                return None;
            }
            direct(&inner)
        }
        Some(None) => None,
        None => direct(link),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_shared_cases() {
        let cases: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/open-links/cases.json")).unwrap();
        for case in cases["cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            let expected = case["url"].as_str().map(str::to_owned);
            assert_eq!(bundle_url_from_link(input), expected, "{input}");
        }
    }
}
