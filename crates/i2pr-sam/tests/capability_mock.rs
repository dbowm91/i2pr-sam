//! Capability-learning regression tests.
//!
//! Defect closed here (**L**): a negotiated version is not a capability. Support is learned
//! from a semantic operation, `RESULT=INVALID_STYLE` is a capability verdict
//! (`SamError::Unsupported`), and `RESULT=CANT_REACH_PEER` stays a rejected, retryable-looking
//! result that must never be recorded as an unsupported style.

mod support;

use i2pr_sam::proto::Support;
use support::{HELLO_REPLY, Match, OWNER_DESTINATION, Rule, SESSION_OK, Script, naming_me_ok};

fn utility_script() -> Script {
    vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)]
}

/// L: a negotiated 3.3 must not make any optional feature look supported.
#[tokio::test]
async fn l_capability_stays_unknown_until_a_semantic_operation_succeeds() {
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=DATAGRAM2"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();

    let before = client.capabilities().await;
    assert_eq!(
        before.negotiated_version,
        Some(i2pr_sam_proto::SamVersion::V3_3),
        "the fixture negotiates 3.3"
    );
    for (name, support) in [
        ("datagram", before.datagram),
        ("datagram2", before.datagram2),
        ("datagram3", before.datagram3),
        ("raw", before.raw),
        ("raw_direct", before.raw_direct),
        ("shared_master", before.shared_master),
        ("session_add_remove", before.session_add_remove),
    ] {
        assert_eq!(
            support,
            Support::Unknown,
            "{name} must be unknown after a handshake that never exercised it"
        );
    }

    client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "d2",
            i2pr_sam_proto::SessionStyle::Datagram2,
            i2pr_sam::DatagramTransport::UdpForward,
            &[],
        )
        .await
        .expect("the fixture accepts DATAGRAM2");

    let after = client.capabilities().await;
    assert_eq!(
        after.datagram2,
        Support::Supported,
        "DATAGRAM2 becomes supported only because the session succeeded"
    );
    assert_eq!(
        after.datagram3,
        Support::Unknown,
        "a successful DATAGRAM2 says nothing about DATAGRAM3"
    );
    bridge.assert_scripts_clean();
}

/// L: the router's verdict decides the error class and the recorded support.
#[tokio::test]
async fn l_invalid_style_is_unsupported_and_cant_reach_peer_stays_rejected() {
    // INVALID_STYLE is a capability verdict.
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE"),
                "SESSION STATUS RESULT=INVALID_STYLE\n",
            ),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let outcome = client
        .create_stream_session(&i2pr_sam::SessionDestination::Transient, "styled", &[])
        .await;
    let error = outcome.err().expect("INVALID_STYLE must fail");
    assert!(
        matches!(error, i2pr_sam::SamError::Unsupported(_)),
        "INVALID_STYLE is a capability verdict, not a rejection: {error:?}"
    );
    assert_eq!(
        i2pr_sam::classify_failure(&error),
        i2pr_sam::FailureClass::CapabilityUnsupported
    );
    assert_eq!(
        client.capabilities().await.stream,
        Support::Unsupported,
        "an explicit style rejection must be recorded as unsupported"
    );
    bridge.assert_scripts_clean();

    // CANT_REACH_PEER is a transient condition and must not be recorded as unsupported.
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE"),
                "SESSION STATUS RESULT=CANT_REACH_PEER\n",
            ),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let outcome = client
        .create_stream_session(&i2pr_sam::SessionDestination::Transient, "unreachable", &[])
        .await;
    let error = outcome.err().expect("CANT_REACH_PEER must fail");
    assert!(
        matches!(error, i2pr_sam::SamError::Rejected(_)),
        "CANT_REACH_PEER is a rejection, not a capability verdict: {error:?}"
    );
    assert_ne!(
        i2pr_sam::classify_failure(&error),
        i2pr_sam::FailureClass::CapabilityUnsupported,
        "an unreachable peer must never look like a missing feature"
    );
    assert_eq!(
        i2pr_sam::classify_failure(&error),
        i2pr_sam::FailureClass::ConfigurationPermanent
    );
    assert_ne!(
        client.capabilities().await.stream,
        Support::Unsupported,
        "a transient failure must not be recorded as an unsupported style"
    );
    bridge.assert_scripts_clean();
}

#[tokio::test]
async fn i2pd_unknown_style_message_is_an_explicit_unsupported_verdict() {
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE"),
                "SESSION STATUS RESULT=I2P_ERROR MESSAGE=\"Unknown STYLE\"\n",
            ),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let outcome = client
        .create_shared_session(
            &i2pr_sam::SessionDestination::Transient,
            "primary",
            i2pr_sam_proto::SharedDialect::Primary,
            &[],
        )
        .await;
    let error = outcome.err().expect("the mock router rejects this dialect");
    assert!(matches!(error, i2pr_sam::SamError::Unsupported(_)));
    assert_eq!(
        client.capabilities().await.shared_primary,
        Support::Unsupported
    );
    bridge.assert_scripts_clean();
}

/// L: support for STREAM must come from a successful session, not from the handshake.
///
/// This is the strict form of the invariant. The handshake negotiates 3.3 and says nothing
/// about whether the router implements STREAM, so the slot must still be `Unknown` until a
/// session actually succeeds.
#[tokio::test]
async fn l_stream_capability_is_not_seeded_by_negotiation_alone() {
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=STREAM"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();

    assert_eq!(
        client.capabilities().await.stream,
        Support::Unknown,
        "a negotiated version is not a capability: nothing has exercised STREAM yet"
    );

    let session = client
        .create_stream_session(&i2pr_sam::SessionDestination::Transient, "learned", &[])
        .await
        .unwrap();
    assert_eq!(
        client.capabilities().await.stream,
        Support::Supported,
        "a successful session is the observation that proves support"
    );
    bridge.assert_scripts_clean();
    session.close().await;
}
