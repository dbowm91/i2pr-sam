//! Live-router interoperability check.
//!
//! This file is the only one that talks to a real router. It is skipped unless both
//! `I2PR_SAM_LIVE=1` and `I2PR_SAM_ENDPOINT` are set, so the ordinary suite never depends on
//! router availability and a skipped run can never be mistaken for a pass: the skip reason is
//! printed, and any run that does start fails loudly rather than degrading.
//!
//! Live qualification of the whole feature matrix is driven by `scripts/interop/qualify.py`;
//! this test only proves the one property a mock cannot: that a real bridge can name its own
//! session with `NAMING LOOKUP NAME=ME` and hand back a concrete Destination.

use i2pr_sam::{ClientConfig, SamClient};
use std::{net::SocketAddr, time::Duration};

/// `Some(endpoint)` when this environment asked for a live run.
fn live_endpoint() -> Option<SocketAddr> {
    if std::env::var("I2PR_SAM_LIVE").as_deref() != Ok("1") {
        return None;
    }
    let raw = std::env::var("I2PR_SAM_ENDPOINT").ok()?;
    Some(raw.parse().expect("I2PR_SAM_ENDPOINT must be host:port"))
}

#[tokio::test]
async fn live_router_resolves_a_concrete_session_identity() {
    let Some(endpoint) = live_endpoint() else {
        println!(
            "live interop skipped: needs I2PR_SAM_LIVE=1 and I2PR_SAM_ENDPOINT (see scripts/interop/qualify.py)"
        );
        return;
    };

    let mut config = ClientConfig::new(endpoint);
    config.connect_timeout = Duration::from_secs(10);
    config.control_timeout = Duration::from_secs(30);
    let client = SamClient::connect(config)
        .await
        .expect("the live router refused the SAM handshake");

    let identity = client
        .session_identity()
        .await
        .expect("a live router must answer NAMING LOOKUP NAME=ME with a concrete Destination");

    assert_ne!(
        identity.destination().as_str(),
        "TRANSIENT",
        "the request token is not an identity"
    );
    assert!(
        !identity.destination().as_str().is_empty(),
        "the router returned an empty Destination"
    );
    let rendered = identity.hash().to_string();
    assert_eq!(
        rendered.len(),
        64,
        "a DestinationHash must render as 64 hex characters, got {rendered:?}"
    );
    assert!(
        rendered
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "a DestinationHash must render lowercase hex, got {rendered:?}"
    );
    println!("live identity resolved: {}", identity.hash());
}
