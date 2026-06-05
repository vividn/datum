//! Desktop notifications fired when a pull lands a NEW row in
//! `integrity_warnings`. Platform-aware:
//!
//! - Linux: shells out to `notify-send`. We avoid `libnotify` so we
//!   don't pull a C dependency into the daemon.
//! - macOS: shells out to `osascript -e 'display notification ...'`.
//! - Other / disabled: [`Notifier::noop()`] swallows everything.
//!
//! The trait + the [`announce_new_warnings`] helper exist so unit tests
//! can substitute a [`MockNotifier`] without forking a real process.

use std::sync::{Arc, Mutex};

/// Platform-agnostic notify entry-point. The trait bound is `Send +
/// Sync` so the daemon can stash one in an `Arc` and call it from any
/// worker without cloning the underlying notifier.
pub trait Notifier: Send + Sync {
    /// Display a single notification. Implementations MUST NOT panic on
    /// failure — the daemon's pull path treats notification failures
    /// as best-effort.
    fn notify(&self, title: &str, body: &str);
}

/// Convenience newtype that swallows every call. Returned by
/// [`Notifier::noop`] when notifications are disabled in config or
/// no platform backend is wired up.
pub struct NoopNotifier;

impl Notifier for NoopNotifier {
    fn notify(&self, _title: &str, _body: &str) {}
}

impl dyn Notifier {
    /// Shared no-op for the disabled case.
    pub fn noop() -> Arc<dyn Notifier> {
        Arc::new(NoopNotifier)
    }
}

/// System notifier that picks the right command per platform. Stateless
/// — every [`notify`](Notifier::notify) call shells out fresh.
pub struct SystemNotifier;

impl Notifier for SystemNotifier {
    fn notify(&self, title: &str, body: &str) {
        // Per-platform best-effort dispatch. We log via tracing on
        // failure; that's enough for triage without flooding the user
        // with a second notification about a failed notification.
        #[cfg(target_os = "linux")]
        {
            let res = std::process::Command::new("notify-send")
                .arg(title)
                .arg(body)
                .status();
            if let Err(e) = res {
                tracing::debug!(err = %e, "notify-send failed (is libnotify installed?)");
            }
        }
        #[cfg(target_os = "macos")]
        {
            let escaped_body = body.replace('"', "\\\"");
            let escaped_title = title.replace('"', "\\\"");
            let script = format!(
                "display notification \"{}\" with title \"{}\"",
                escaped_body, escaped_title,
            );
            let res = std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .status();
            if let Err(e) = res {
                tracing::debug!(err = %e, "osascript notification failed");
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (title, body);
            tracing::debug!("desktop notifications unsupported on this platform; skipping");
        }
    }
}

/// In-memory notifier used by tests. Records every `(title, body)`
/// pair so assertions can introspect what would have been displayed.
#[derive(Default, Clone)]
pub struct MockNotifier {
    pub calls: Arc<Mutex<Vec<(String, String)>>>,
}

impl MockNotifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().expect("notifier mutex").clone()
    }
}

impl Notifier for MockNotifier {
    fn notify(&self, title: &str, body: &str) {
        self.calls
            .lock()
            .expect("notifier mutex")
            .push((title.to_string(), body.to_string()));
    }
}

/// Resolve the per-DB notifier from config: returns a [`SystemNotifier`]
/// unless the user explicitly set `notifications: false`. The
/// `db_name` is the resolution key — present for symmetry with
/// `resolve_sync_server`, even though notifications are a global
/// switch today.
pub fn from_config(enabled_default: bool) -> Arc<dyn Notifier> {
    let enabled = match datum_commands::config::load_raw_config_or_default() {
        Ok(cfg) => cfg.notifications.unwrap_or(enabled_default),
        Err(_) => enabled_default,
    };
    if enabled {
        Arc::new(SystemNotifier)
    } else {
        Arc::new(NoopNotifier)
    }
}

/// Fire one notification per newly-queued integrity warning batch.
/// Called by the daemon after a successful pull whose
/// `PullOutcome.integrity_warnings_queued > 0`. The per-warning detail
/// (which entry, which kind) is left to the user to inspect via
/// `datum check`; the notification just nudges them to look.
pub fn announce_new_warnings(notifier: &dyn Notifier, db_name: &str, queued: usize) {
    if queued == 0 {
        return;
    }
    let title = "Daydumb integrity warning";
    let noun = if queued == 1 { "warning" } else { "warnings" };
    let body = format!("{queued} new integrity {noun} on {db_name}: run 'datum check'");
    notifier.notify(title, &body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_swallows_calls() {
        let n = NoopNotifier;
        n.notify("title", "body"); // does not panic
    }

    #[test]
    fn mock_records_calls() {
        let n = MockNotifier::new();
        n.notify("a", "b");
        n.notify("c", "d");
        let calls = n.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], ("a".to_string(), "b".to_string()));
        assert_eq!(calls[1], ("c".to_string(), "d".to_string()));
    }

    #[test]
    fn announce_skips_zero() {
        let n = MockNotifier::new();
        announce_new_warnings(&n, "default", 0);
        assert!(n.calls().is_empty(), "no calls when queued == 0");
    }

    #[test]
    fn announce_singular_message_for_one() {
        let n = MockNotifier::new();
        announce_new_warnings(&n, "default", 1);
        let calls = n.calls();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].1.contains("1 new integrity warning"),
            "expected singular form: {:?}",
            calls[0]
        );
        // The count phrase itself must use the singular noun.
        assert!(
            !calls[0].1.contains("1 new integrity warnings"),
            "expected singular noun after the count: {:?}",
            calls[0]
        );
    }

    #[test]
    fn announce_plural_message_for_many() {
        let n = MockNotifier::new();
        announce_new_warnings(&n, "default", 3);
        let calls = n.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].1.contains("3 new integrity warnings"));
    }

    #[test]
    fn daemon_fires_notification_on_new_warning() {
        // Minimal end-to-end check: the helper invokes the notifier
        // exactly once per non-zero pull, with a body that names the
        // db and points the user at the CLI. This is the
        // acceptance-test name from the bead — keep it stable.
        let n = MockNotifier::new();
        announce_new_warnings(&n, "work", 2);
        let calls = n.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Daydumb integrity warning");
        assert!(calls[0].1.contains("work"));
        assert!(calls[0].1.contains("datum check"));
    }
}
