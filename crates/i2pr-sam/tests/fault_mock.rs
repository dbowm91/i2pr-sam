//! Fault-tolerance regression tests.
//!
//! Defect closed here (**N**): a router that disappears mid-session, replies with a line that
//! violates the framing bounds, or claims a payload size far larger than any buffer must
//! produce a bounded, typed error. It must never panic, never hang, and never allocate from
//! an untrusted size claim.

mod support;

use std::time::Duration;
use support::{
    HELLO_REPLY, Match, OWNER_DESTINATION, Rule, SESSION_OK, Script, THIRD_DESTINATION, bytes,
    datagram_delivery, naming_me_ok,
};

fn utility_script() -> Script {
    vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)]
}

/// N: an EOF in the middle of a session must not leak a panic or an unbounded wait.
#[tokio::test]
async fn n_mid_session_eof_fails_the_next_operation_with_a_bounded_error() {
    let bridge = support::MockBridge::start_with_fallback(
        vec![
            utility_script(),
            vec![
                Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
                Rule::line(Match::starts_with("SESSION CREATE"), SESSION_OK),
                // The router vanishes right after answering the session creation.
                Rule::close(),
            ],
            vec![
                Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
                Rule::close(),
            ],
        ],
        support::default_script(),
    )
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();

    let session = tokio::time::timeout(
        Duration::from_secs(3),
        client.create_stream_session(&i2pr_sam::SessionDestination::Transient, "ghost", &[]),
    )
    .await
    .expect("session creation must not hang after the router closed the socket")
    .expect("an ordinary STREAM session tolerates a failed identity lookup");

    // The next operation must fail promptly and with a typed error, never a panic.
    let outcome = tokio::time::timeout(Duration::from_secs(3), session.accept())
        .await
        .expect("accept must not hang after the router closed the socket");
    let error = outcome
        .err()
        .expect("accept must fail after mid-session EOF");
    assert!(
        matches!(
            error,
            i2pr_sam::SamError::Closed | i2pr_sam::SamError::Io(_)
        ),
        "unexpected error after mid-session EOF: {error:?}"
    );
    assert!(
        matches!(
            i2pr_sam::classify_failure(&error),
            i2pr_sam::FailureClass::TransportTransient | i2pr_sam::FailureClass::CancelledOrClosed
        ),
        "a vanished router must stay retryable-or-closed, never a permanent verdict, got {:?}",
        i2pr_sam::classify_failure(&error)
    );
    bridge.assert_scripts_clean();
    session.close().await;
}

/// N: a reply longer than the configured frame bound must be refused, not buffered.
#[tokio::test]
async fn n_reply_beyond_the_framing_bound_is_refused_without_buffering_it() {
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            // A single line four kilobytes long, with no terminator in sight.
            Rule::line_bytes(Match::starts_with("SESSION CREATE"), vec![b'x'; 4096]),
        ],
    ])
    .await;
    let mut config = bridge.client_config();
    config.max_frame_bytes = 256;
    let client = i2pr_sam::SamClient::connect(config).await.unwrap();

    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        client.create_stream_session(&i2pr_sam::SessionDestination::Transient, "oversized", &[]),
    )
    .await
    .expect("an oversized reply must be refused, not waited on");
    let error = outcome.err().expect("an oversized reply must fail");
    assert!(
        matches!(error, i2pr_sam::SamError::Protocol(_)),
        "a line past the framing bound is a protocol error, got {error:?}"
    );
    assert_eq!(
        i2pr_sam::classify_failure(&error),
        i2pr_sam::FailureClass::ProtocolPermanent
    );
    bridge.assert_scripts_clean();
}

/// N: an untrusted `SIZE` claim must be refused on its face, not allocated.
#[tokio::test]
async fn n_oversized_size_claim_is_refused_without_allocating_the_claimed_bytes() {
    let bogus = format!("DATAGRAM RECEIVED DESTINATION={THIRD_DESTINATION} SIZE=4294967295\n");
    let recovered = datagram_delivery(THIRD_DESTINATION, 7, 8, b"recovered");
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=DATAGRAM ID=oversize"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
            Rule::push(bytes(&bogus)),
            // The router dies mid-frame, as a real bridge would after a false size claim.
            Rule::close(),
        ],
        // A second, well-behaved link: the bad frame must stay contained to its own socket.
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=DATAGRAM ID=fresh"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
            Rule::push(recovered),
        ],
    ])
    .await;
    let mut config = bridge.client_config();
    config.max_datagram_bytes = 64;
    config.max_inbox_bytes = 64;
    config.max_inbox_datagrams = 4;
    let client = i2pr_sam::SamClient::connect(config).await.unwrap();
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "oversize",
            i2pr_sam_proto::SessionStyle::Datagram,
            i2pr_sam::DatagramTransport::ControlSocketV1,
            &[],
        )
        .await
        .unwrap();

    // A four-gigabyte claim must be refused on its face: nothing from that frame may reach
    // the queue, in any form, and closing must not have to wait for the claimed bytes.
    assert_eq!(
        session.queued_datagrams().await,
        0,
        "a refused size claim must never become a queued delivery"
    );
    assert_eq!(
        session.dropped_datagrams().await,
        0,
        "a refused size claim is not a dropped delivery either"
    );
    let first_close = tokio::time::timeout(Duration::from_secs(3), session.close()).await;
    assert!(
        first_close.is_ok(),
        "close must not block behind an untrusted SIZE claim"
    );
    let second_close = tokio::time::timeout(Duration::from_secs(3), session.close()).await;
    assert!(second_close.is_ok(), "close must stay idempotent");

    // The rest of the client is unaffected: a fresh link still decodes byte-exact.
    let fresh = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "fresh",
            i2pr_sam_proto::SessionStyle::Datagram,
            i2pr_sam::DatagramTransport::ControlSocketV1,
            &[],
        )
        .await
        .unwrap();
    match tokio::time::timeout(Duration::from_secs(3), fresh.recv())
        .await
        .expect("an unrelated session must not be starved by a bad frame")
        .unwrap()
    {
        i2pr_sam_proto::ReceivedDatagram::Authenticated(message) => {
            assert_eq!(message.payload, b"recovered");
            assert_eq!(message.from_port.get(), 7);
            assert_eq!(message.to_port.get(), 8);
        }
        other => panic!("expected an authenticated delivery, got {other:?}"),
    }
    bridge.assert_scripts_clean();
    fresh.close().await;
}

/// N: a delivery line that violates its own framing must not desynchronise the stream.
#[tokio::test]
async fn n_malformed_delivery_header_does_not_desynchronise_later_deliveries() {
    let good = datagram_delivery(THIRD_DESTINATION, 3, 4, b"intact");
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=DATAGRAM ID=garbled"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
            // No SIZE, and an unparsable port: the frame must be skipped whole.
            Rule::push(bytes(&format!(
                "DATAGRAM RECEIVED DESTINATION={THIRD_DESTINATION} FROM_PORT=nine\n"
            ))),
            Rule::push(bytes("RAW RECEIVED SIZE=abc\n")),
            Rule::push(good),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "garbled",
            i2pr_sam_proto::SessionStyle::Datagram,
            i2pr_sam::DatagramTransport::ControlSocketV1,
            &[],
        )
        .await
        .unwrap();

    match tokio::time::timeout(Duration::from_secs(3), session.recv())
        .await
        .expect("a valid delivery behind a malformed one must still arrive")
        .unwrap()
    {
        i2pr_sam_proto::ReceivedDatagram::Authenticated(message) => {
            assert_eq!(message.source.as_str(), THIRD_DESTINATION);
            assert_eq!(message.from_port.get(), 3);
            assert_eq!(message.to_port.get(), 4);
            assert_eq!(
                message.payload, b"intact",
                "a skipped header must not shift the following delivery"
            );
        }
        other => panic!("expected an authenticated DATAGRAM1 delivery, got {other:?}"),
    }
    bridge.assert_scripts_clean();
    session.close().await;
}
