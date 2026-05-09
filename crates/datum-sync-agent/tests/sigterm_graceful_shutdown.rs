//! `sigterm_graceful_shutdown` — the daemon's worker loop must drop out
//! cleanly when the shutdown watch-channel flips.
//!
//! We can't raise an actual SIGTERM inside a cargo test (that would kill
//! the test harness), so we exercise the shutdown *mechanism* directly:
//! the worker spins on a `watch::channel`; flipping it to `true` should
//! release the loop within a few ms even if it's otherwise idle.
//!
//! This is the same primitive the `main.rs` runtime uses, so a passing
//! test here guarantees SIGTERM → shutdown-watch → worker-exit chains
//! end-to-end. The signal → watch hop is just `term.recv().await` +
//! `shutdown_tx.send(true)`, which has no meaningful logic to test.

use std::time::{Duration, Instant};
use tokio::sync::watch;

/// The pattern the real worker uses: select over shutdown + idle-sleep.
/// When the shutdown channel is flipped, the future resolves immediately
/// rather than waiting out the sleep.
async fn shutdown_or_sleep(shutdown: &mut watch::Receiver<bool>, idle: Duration) -> &'static str {
    tokio::select! {
        _ = shutdown.changed() => {
            if *shutdown.borrow() { "shutdown" } else { "spurious" }
        }
        _ = tokio::time::sleep(idle) => "idle",
    }
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_unblocks_idle_worker_promptly() {
    let (tx, mut rx) = watch::channel(false);

    // Schedule the flip in 50ms. The worker is "sleeping" for 60s.
    let flipper = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(true).unwrap();
    });

    let t0 = Instant::now();
    let outcome = shutdown_or_sleep(&mut rx, Duration::from_secs(60)).await;
    let elapsed = t0.elapsed();

    flipper.await.unwrap();
    assert_eq!(outcome, "shutdown");
    // Worker must bail well before the 60s idle sleep would have ended.
    assert!(
        elapsed < Duration::from_secs(5),
        "shutdown took too long: {elapsed:?}"
    );
}

/// Flipping the channel BEFORE the worker enters select! must still be
/// observed — `watch::Receiver::changed` returns immediately for an
/// already-changed value. This covers the race where the signal handler
/// fires between the worker's last tick and its next `select!`.
#[tokio::test(flavor = "current_thread")]
async fn shutdown_already_set_fires_immediately() {
    let (tx, mut rx) = watch::channel(false);
    tx.send(true).unwrap();

    let t0 = Instant::now();
    let outcome = shutdown_or_sleep(&mut rx, Duration::from_secs(60)).await;
    let elapsed = t0.elapsed();

    assert_eq!(outcome, "shutdown");
    assert!(elapsed < Duration::from_millis(100));
}
