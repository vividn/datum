//! Pure URL and server-name helpers. No filesystem, no HTTP — safe to
//! compile on every target.
//!
//! Where local files live (token, master key) is the CLI's concern; it
//! resolves per-server paths with [`server_host_from_url`] and owns the
//! XDG conventions.

/// Extract a stable "host" component for an HTTPS URL — used to key per-server
/// token/key files on the client. Falls back to the URL itself on parse
/// failure so callers never silently lose the server identity; path
/// separators in the fallback are replaced with `_` since the result
/// becomes a filename component (`auth/<host>.json`).
pub fn server_host_from_url(url: &str) -> String {
    if let Ok(u) = ::url::Url::parse(url) {
        if let Some(h) = u.host_str() {
            return match u.port() {
                Some(p) => format!("{h}_{p}"),
                None => h.to_string(),
            };
        }
    }
    url.replace(['/', '\\'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_only() {
        assert_eq!(
            server_host_from_url("https://sync.example.com"),
            "sync.example.com"
        );
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

    #[test]
    fn fallback_sanitizes_path_separators() {
        // The fallback becomes a filename component (auth/<host>.json) —
        // separators must not nest directories.
        assert_eq!(server_host_from_url("foo/bar"), "foo_bar");
        assert_eq!(server_host_from_url("foo\\bar"), "foo_bar");
    }
}
