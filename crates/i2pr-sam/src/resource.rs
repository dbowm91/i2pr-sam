//! Process-wide resource accounting.
//!
//! Long-lived SAM clients leak in ways that unit tests never notice: a control socket
//! whose reader task outlives its session, a stream that is never dropped, a retry loop
//! that keeps admission permits. Exposing live counters turns "we think we cleaned up" into
//! something a soak test can assert.
//!
//! Counters are deliberately monotonic per event and cheap: relaxed atomics, no locking, no
//! per-session allocation. They are an observability aid, not an enforcement mechanism, and
//! a counter never gates a protocol decision.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceUsage {
    /// Sessions created and not yet closed.
    pub live_sessions: u64,
    /// Sessions created since process start.
    pub sessions_created: u64,
    /// Stream connections or accepted streams currently alive.
    pub live_streams: u64,
    /// Highest simultaneous stream count observed.
    pub peak_streams: u64,
    /// Control sockets currently open.
    pub live_sockets: u64,
    /// Control sockets opened since process start.
    pub sockets_opened: u64,
    /// Datagram deliveries dropped because a bounded queue was full.
    pub datagrams_dropped: u64,
}

#[derive(Debug, Default)]
struct Ledger {
    live_sessions: AtomicU64,
    sessions_created: AtomicU64,
    live_streams: AtomicU64,
    peak_streams: AtomicU64,
    live_sockets: AtomicU64,
    sockets_opened: AtomicU64,
    datagrams_dropped: AtomicU64,
}

static LEDGER: Ledger = Ledger::new_static();

impl Ledger {
    const fn new_static() -> Self {
        Self {
            live_sessions: AtomicU64::new(0),
            sessions_created: AtomicU64::new(0),
            live_streams: AtomicU64::new(0),
            peak_streams: AtomicU64::new(0),
            live_sockets: AtomicU64::new(0),
            sockets_opened: AtomicU64::new(0),
            datagrams_dropped: AtomicU64::new(0),
        }
    }
}

/// Current process-wide usage.
pub fn resource_usage() -> ResourceUsage {
    let live_streams = LEDGER.live_streams.load(Ordering::Relaxed);
    ResourceUsage {
        live_sessions: LEDGER.live_sessions.load(Ordering::Relaxed),
        sessions_created: LEDGER.sessions_created.load(Ordering::Relaxed),
        live_streams,
        peak_streams: LEDGER
            .peak_streams
            .load(Ordering::Relaxed)
            .max(live_streams),
        live_sockets: LEDGER.live_sockets.load(Ordering::Relaxed),
        sockets_opened: LEDGER.sockets_opened.load(Ordering::Relaxed),
        datagrams_dropped: LEDGER.datagrams_dropped.load(Ordering::Relaxed),
    }
}

pub(crate) fn session_opened() {
    LEDGER.live_sessions.fetch_add(1, Ordering::Relaxed);
    LEDGER.sessions_created.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn session_closed() {
    decrement_saturating(&LEDGER.live_sessions);
}

pub(crate) fn socket_opened() {
    LEDGER.live_sockets.fetch_add(1, Ordering::Relaxed);
    LEDGER.sockets_opened.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn socket_closed() {
    decrement_saturating(&LEDGER.live_sockets);
}

pub(crate) fn stream_opened() {
    let live = LEDGER.live_streams.fetch_add(1, Ordering::Relaxed) + 1;
    // Record the high-water mark without a compare-exchange loop on the hot path.
    LEDGER.peak_streams.fetch_max(live, Ordering::Relaxed);
}

pub(crate) fn stream_closed() {
    decrement_saturating(&LEDGER.live_streams);
}

/// Decrement without wrapping: a double release must not look like a huge live count.
fn decrement_saturating(counter: &AtomicU64) {
    let mut current = counter.load(Ordering::Relaxed);
    while current > 0 {
        match counter.compare_exchange_weak(
            current,
            current - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

pub(crate) fn datagrams_dropped(count: u64) {
    if count > 0 {
        LEDGER.datagrams_dropped.fetch_add(count, Ordering::Relaxed);
    }
}

/// Decrements a live counter exactly once, however the owner goes away.
pub(crate) struct ReleaseOnDrop {
    release: fn(),
}

impl ReleaseOnDrop {
    pub(crate) fn new(release: fn()) -> Self {
        Self { release }
    }
}

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        (self.release)();
    }
}
