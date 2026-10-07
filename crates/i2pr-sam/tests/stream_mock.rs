//! Stream framing regression tests.
//!
//! Defects closed here:
//!
//! * **A** — a non-silent `STREAM ACCEPT` is answered with `STREAM STATUS RESULT=OK` and only
//!   then with the router's peer identity block (`$destination`, optional `FROM_PORT=` and
//!   `TO_PORT=`, then a blank line). The previous implementation started payload framing at
//!   the status line, so the identity block leaked into application data and
//!   `remote_destination()` was always `None`.
//! * **B** — when the router omits the terminating blank line, the first payload line that is
//!   not part of the peer block must be pushed back and still delivered byte-exact.
//! * **C** — `SILENT=true` announces nothing, so nothing may be consumed before payload.
//! * **D** — `STREAM CONNECT` has no identity block; payload starts immediately.

mod support;

use i2pr_sam::{SamClient, SessionDestination};
use support::{
    HELLO_REPLY, Match, OWNER_DESTINATION, PADDED_PEER_DESTINATION, PEER_DESTINATION, Rule,
    SESSION_OK, Script, bytes, frame, naming_me_ok,
};
use tokio::io::AsyncReadExt;

const SESSION_CREATE_STREAM: &str =
    "SESSION CREATE STYLE=STREAM ID=listener DESTINATION=TRANSIENT SIGNATURE_TYPE=7";

/// Connection 0 answers the utility handshake, connection 1 creates the STREAM session,
/// connection 2 carries the accepted stream.
fn accept_scripts(accept_reply: &[u8], accept_command: Match) -> Vec<Script> {
    vec![
        vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(Match::exact(SESSION_CREATE_STREAM), SESSION_OK),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
        ],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line_bytes(accept_command, bytes("STREAM STATUS RESULT=OK\n")),
            Rule::push(accept_reply.to_vec()),
        ],
    ]
}

async fn accept_session(bridge: &support::MockBridge) -> i2pr_sam::StreamSession {
    let client = SamClient::connect(bridge.client_config()).await.unwrap();
    client
        .create_stream_session(&SessionDestination::Transient, "listener", &[])
        .await
        .unwrap()
}

/// A: the peer identity block belongs to the handshake, never to the payload.
#[tokio::test]
async fn a_non_silent_stream_accept_consumes_the_peer_identity_block_before_payload() {
    const PAYLOAD: &[u8] = b"first application byte\0binary\nexact";
    let mut reply = format!("{PEER_DESTINATION}\nFROM_PORT=1\nTO_PORT=2\n\n").into_bytes();
    reply.extend_from_slice(PAYLOAD);
    let scripts = accept_scripts(&reply, Match::exact("STREAM ACCEPT ID=listener"));
    let bridge = support::MockBridge::start_with(scripts).await;
    let session = accept_session(&bridge).await;

    let mut stream = session.accept().await.unwrap();

    let peer = stream
        .peer()
        .expect("a non-silent accept must announce the authenticated peer");
    assert_eq!(
        peer.destination.as_str(),
        PEER_DESTINATION,
        "peer destination must be the Destination the router announced"
    );
    assert_eq!(peer.from_port.map(|port| port.get()), Some(1));
    assert_eq!(peer.to_port.map(|port| port.get()), Some(2));
    assert_eq!(
        stream.remote_destination().map(|d| d.as_str()),
        Some(PEER_DESTINATION)
    );

    // The first bytes off the stream must be payload, with no identity-block bytes.
    let mut received = vec![0u8; PAYLOAD.len()];
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        stream.read_exact(&mut received),
    )
    .await
    .expect("payload read timed out")
    .unwrap();
    assert_eq!(
        received, PAYLOAD,
        "identity-block bytes leaked into application data"
    );
    assert!(
        !received.starts_with(PEER_DESTINATION.as_bytes()),
        "payload still starts with the identity block"
    );

    // Byte-level proof of the command the client actually wrote.
    bridge
        .connection(2)
        .assert_wrote(b"HELLO VERSION MIN=3.1 MAX=3.3\nSTREAM ACCEPT ID=listener\n");
    bridge.assert_scripts_clean();
    session.close().await;
}

/// B: a router that forgets the blank line must not cost the first payload line.
#[tokio::test]
async fn b_stream_accept_pushes_back_the_first_payload_line_when_the_blank_terminator_is_missing() {
    // No blank line: the router goes straight from TO_PORT= to the payload.
    let reply = format!("{PEER_DESTINATION}\nFROM_PORT=1\nTO_PORT=2\nfirst-line\nsecond-line");
    let bridge = support::MockBridge::start_with(accept_scripts(
        reply.as_bytes(),
        Match::exact("STREAM ACCEPT ID=listener"),
    ))
    .await;
    let session = accept_session(&bridge).await;

    let mut stream = session.accept().await.unwrap();

    let peer = stream.peer().expect("peer block still parsed");
    assert_eq!(peer.destination.as_str(), PEER_DESTINATION);
    assert_eq!(peer.from_port.map(|port| port.get()), Some(1));
    assert_eq!(peer.to_port.map(|port| port.get()), Some(2));

    // The line that terminated the peer block is payload and must come back untouched.
    let mut received = vec![0u8; b"first-line\nsecond-line".len()];
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        stream.read_exact(&mut received),
    )
    .await
    .expect("payload read timed out")
    .unwrap();
    assert_eq!(
        received, b"first-line\nsecond-line",
        "the first payload line was consumed by the peer-block reader"
    );
    bridge.assert_scripts_clean();
    session.close().await;
}

/// C: a silent accept announces nothing, so nothing may be consumed before payload.
#[tokio::test]
async fn c_silent_stream_accept_consumes_nothing_before_payload() {
    const PAYLOAD: &[u8] = b"\n\nthis line begins with a blank line";
    let bridge = support::MockBridge::start_with(accept_scripts(
        PAYLOAD,
        Match::exact("STREAM ACCEPT ID=listener SILENT=true"),
    ))
    .await;
    let session = accept_session(&bridge).await;

    let mut stream = session.accept_with(true).await.unwrap();

    assert!(
        stream.peer().is_none(),
        "a silent accept cannot know the peer, so it must not claim one"
    );
    assert!(stream.remote_destination().is_none());
    let mut received = vec![0u8; PAYLOAD.len()];
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        stream.read_exact(&mut received),
    )
    .await
    .expect("payload read timed out")
    .unwrap();
    assert_eq!(
        received, PAYLOAD,
        "the silent accept reader consumed bytes that belong to the payload"
    );
    bridge
        .connection(2)
        .assert_wrote(b"HELLO VERSION MIN=3.1 MAX=3.3\nSTREAM ACCEPT ID=listener SILENT=true\n");
    bridge.assert_scripts_clean();
    session.close().await;
}

/// D: `STREAM CONNECT` never carries an identity block, in either direction.
#[tokio::test]
async fn d_stream_connect_never_consumes_a_peer_identity_block() {
    const PAYLOAD: &[u8] = b"OUTBOUND=1\nraw payload\n";
    let bridge = support::MockBridge::start_with(vec![
        vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(Match::exact(SESSION_CREATE_STREAM), SESSION_OK),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
        ],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::exact(
                    "STREAM CONNECT ID=listener DESTINATION=peer.b32.i2p FROM_PORT=7 TO_PORT=8",
                ),
                "STREAM STATUS RESULT=OK\n",
            ),
            Rule::push(frame(&[PAYLOAD])),
        ],
    ])
    .await;
    let session = accept_session(&bridge).await;

    let mut stream = session
        .connect(
            "peer.b32.i2p",
            Some(i2pr_sam_proto::Port::new(7)),
            Some(i2pr_sam_proto::Port::new(8)),
        )
        .await
        .unwrap();

    assert!(
        stream.peer().is_none(),
        "STREAM CONNECT has no identity block, so no peer may be reported"
    );
    assert!(stream.remote_destination().is_none());
    let mut received = vec![0u8; PAYLOAD.len()];
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        stream.read_exact(&mut received),
    )
    .await
    .expect("payload read timed out")
    .unwrap();
    assert_eq!(received, PAYLOAD);
    bridge.connection(2).assert_wrote(
        b"HELLO VERSION MIN=3.1 MAX=3.3\nSTREAM CONNECT ID=listener DESTINATION=peer.b32.i2p FROM_PORT=7 TO_PORT=8\n",
    );
    bridge.assert_scripts_clean();
    session.close().await;
}

/// A (shape): a Destination with the padding a real 256-byte keypair produces must be
/// accepted as the peer identity.
///
/// Standard base64 of a 256-byte Destination always ends in `=`, so the peer line is
/// indistinguishable from a `KEY=VALUE` port line by punctuation alone. Tests A-D use a
/// padding-free fixture so their framing assertions do not depend on this; this test exists
/// so the realistic shape cannot regress into a blanket refusal.
#[tokio::test]
async fn a2_non_silent_stream_accept_accepts_a_base64_padded_peer_destination() {
    const PAYLOAD: &[u8] = b"padded-destination-payload";
    let mut reply = format!("{PADDED_PEER_DESTINATION}\nFROM_PORT=3\nTO_PORT=4\n\n").into_bytes();
    reply.extend_from_slice(PAYLOAD);
    let bridge = support::MockBridge::start_with(accept_scripts(
        &reply,
        Match::exact("STREAM ACCEPT ID=listener"),
    ))
    .await;
    let session = accept_session(&bridge).await;

    let mut stream = session.accept().await.unwrap();

    let peer = stream
        .peer()
        .expect("a padded base64 Destination is still a Destination");
    assert_eq!(peer.destination.as_str(), PADDED_PEER_DESTINATION);
    assert_eq!(peer.from_port.map(|port| port.get()), Some(3));
    assert_eq!(peer.to_port.map(|port| port.get()), Some(4));
    let mut received = vec![0u8; PAYLOAD.len()];
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        stream.read_exact(&mut received),
    )
    .await
    .expect("payload read timed out")
    .unwrap();
    assert_eq!(received, PAYLOAD);
    bridge.assert_scripts_clean();
    session.close().await;
}
