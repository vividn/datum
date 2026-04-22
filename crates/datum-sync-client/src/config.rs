//! Pure URL and server-name helpers. No filesystem, no HTTP — safe to
//! compile on every target.
//!
//! Where local files live (token, master key) is the CLI's concern; it
//! resolves per-server paths with [`server_host_from_url`] and owns the
//! XDG conventions.

/// Extract a stable "host" component for an HTTPS URL — used to key per-server
/// token/key files on the client. Falls back to the URL itself on parse
/// failure so callers never silently lose the server identity.
pub fn server_host_from_url(url: &str) -> String {
    match ::url::Url::parse(url) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(h), Some(p)) => format!("{h}_{p}"),
            (Some(h), None) => h.to_string(),
            _ => url.to_string(),
        },
        Err(_) => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_only() {
        assert_eq!(server_host_from_url("https://sync.example.com"), "sync.example.com");
    }

    #[test]
    fn host_with_port() {
        assert_eq!(
            server_host_from_url("http://localhost:3000"),
            "localhost_3000"
        );
    }

    #[test]
    fn garbage_falls_back_to_input() {
        assert_eq!(server_host_from_url("not a url"), "not a url");
    }
}
