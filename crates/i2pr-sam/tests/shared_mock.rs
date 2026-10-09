//! Shared-session regression tests.
//!
//! Defects closed here:
//!
//! * **I** — a shared session's identity must be concrete. The client resolves it with
//!   `NAMING LOOKUP NAME=ME` and hashes the decoded Destination; the request token
//!   `TRANSIENT` is never an acceptable answer, and a router that cannot resolve `NAME=ME`
//!   must fail the session rather than degrade silently.
//! * **J** — stream acceptance on a shared subsession behaves exactly like an ordinary
//!   session: the peer identity block is consumed and payload starts at byte zero.
//! * **M** — shared-session invariants: datagram routing options live on subsessions, never on
//!   the owner; duplicate child IDs and duplicate listener tuples are refused; a RAW
//!   subsession may not claim the streaming protocol.

mod support;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use i2pr_sam::{SamError, SessionDestination, SessionStyle, SharedDialect};
use sha2::{Digest, Sha256};
use support::{
    HELLO_REPLY, Match, OWNER_DESTINATION, PEER_DESTINATION, Rule, SESSION_OK, Script, bytes,
    naming_me_ok,
};

/// SHA-256 of the decoded Destination, computed here rather than read back from the client,
/// so the assertion cannot pass by comparing the client against itself.
fn expected_hash(destination: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(BASE64.decode(destination).expect("fixture must be base64"));
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn utility_script() -> Script {
    vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)]
}

/// The second connection: create a MASTER session, resolve `NAME=ME`, then answer `adds`
/// subsession commands.
fn owner_script(naming_reply: &str, adds: usize) -> Script {
    let mut script = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::exact(
                "SESSION CREATE STYLE=MASTER ID=owner DESTINATION=TRANSIENT SIGNATURE_TYPE=7",
            ),
            SESSION_OK,
        ),
        Rule::line(Match::exact("NAMING LOOKUP NAME=ME"), naming_reply),
    ];
    script.extend((0..adds).map(|_| Rule::line(Match::starts_with("SESSION ADD"), SESSION_OK)));
    script
}

/// I: a `TRANSIENT` shared session still resolves to a concrete, hashed identity, and every
/// child reports the owner's Destination and hash.
#[tokio::test]
async fn i_shared_session_identity_is_concrete_and_every_child_reports_it() {
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        owner_script(&naming_me_ok(OWNER_DESTINATION), 2),
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();

    let owner = client
        .create_shared_session(
            &SessionDestination::Transient,
            "owner",
            SharedDialect::Master,
            &[],
        )
        .await
        .unwrap();
    let identity = owner.identity();
    assert_ne!(
        identity.destination().as_str(),
        "TRANSIENT",
        "the request token is not an identity"
    );
    assert_eq!(identity.destination().as_str(), OWNER_DESTINATION);
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
    assert_eq!(
        rendered,
        expected_hash(OWNER_DESTINATION),
        "the hash must be SHA-256 of the decoded base64 Destination"
    );

    for (id, style, options) in [
        ("inbound", SessionStyle::Stream, vec![]),
        (
            "udp",
            SessionStyle::Datagram,
            vec![("LISTEN_PORT".to_owned(), "9".to_owned())],
        ),
    ] {
        let child = owner.add_child(id, style, &options).await.unwrap();
        assert_eq!(
            child.identity().destination().as_str(),
            identity.destination().as_str(),
            "child {id} must report the owner's concrete Destination"
        );
        assert_eq!(
            child.identity().hash().to_string(),
            rendered,
            "child {id} must report the owner's hash"
        );
        assert!(child.is_open());
    }
    bridge.assert_scripts_clean();
    owner.close().await;
}

/// I: a router that cannot name the session must fail the session, not hand back a token.
#[tokio::test]
async fn i_shared_session_fails_when_the_router_cannot_resolve_name_me() {
    for (label, reply) in [
        (
            "NAME=ME is not found",
            "NAMING REPLY RESULT=KEY_NOT_FOUND NAME=ME\n".to_owned(),
        ),
        (
            "NAME=ME resolves to the request token",
            "NAMING REPLY RESULT=OK NAME=ME VALUE=TRANSIENT\n".to_owned(),
        ),
        (
            "NAME=ME resolves to text that is not base64",
            "NAMING REPLY RESULT=OK NAME=ME VALUE=not-a-destination\n".to_owned(),
        ),
    ] {
        let bridge =
            support::MockBridge::start_with(vec![utility_script(), owner_script(&reply, 0)]).await;
        let client = i2pr_sam::SamClient::connect(bridge.client_config())
            .await
            .unwrap();
        let outcome = client
            .create_shared_session(
                &SessionDestination::Transient,
                "owner",
                SharedDialect::Master,
                &[],
            )
            .await;
        assert!(
            matches!(outcome, Err(SamError::IdentityUnavailable)),
            "{label}: a shared session without a concrete identity must fail, got {:?}",
            outcome.err()
        );
        bridge.assert_lacks(b"SESSION ADD");
        bridge.assert_scripts_clean();
    }
}

/// J: a shared subsession's stream accept consumes the peer block like any other session.
#[tokio::test]
async fn j_shared_subsession_stream_accept_consumes_the_peer_identity_block() {
    const PAYLOAD: &[u8] = b"subsession-payload\0with-nul";
    let stream_script = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::exact("STREAM ACCEPT ID=inbound"),
            "STREAM STATUS RESULT=OK\n",
        ),
        Rule::push(bytes(&format!(
            "{PEER_DESTINATION}\nFROM_PORT=11\nTO_PORT=22\n\n"
        ))),
        Rule::push(PAYLOAD.to_vec()),
    ];
    let bridge = support::MockBridge::start_with(vec![
        utility_script(),
        owner_script(&naming_me_ok(OWNER_DESTINATION), 1),
        stream_script,
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let owner = client
        .create_shared_session(
            &SessionDestination::Transient,
            "owner",
            SharedDialect::Master,
            &[],
        )
        .await
        .unwrap();
    let child = owner
        .add_child("inbound", SessionStyle::Stream, &[])
        .await
        .unwrap();

    let mut stream = child.accept().await.unwrap();
    let peer = stream
        .peer()
        .expect("a non-silent shared accept must announce the peer");
    assert_eq!(peer.destination.as_str(), PEER_DESTINATION);
    assert_eq!(peer.from_port.map(|port| port.get()), Some(11));
    assert_eq!(peer.to_port.map(|port| port.get()), Some(22));

    let mut received = vec![0u8; PAYLOAD.len()];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut received)
        .await
        .unwrap();
    assert_eq!(
        received, PAYLOAD,
        "the peer identity block leaked into a shared subsession's payload"
    );
    bridge
        .connection(2)
        .assert_wrote(b"HELLO VERSION MIN=3.1 MAX=3.3\nSTREAM ACCEPT ID=inbound\n");
    bridge.assert_scripts_clean();
    owner.close().await;
}

/// M: datagram routing options belong to subsessions, and the owner's shape stays unique.
#[tokio::test]
async fn m_shared_session_rejects_routing_options_duplicate_ids_and_duplicate_listeners() {
    let bridge = support::MockBridge::start_with_fallback(
        vec![
            utility_script(),
            owner_script(&naming_me_ok(OWNER_DESTINATION), 3),
        ],
        // Every connection after the owner is an attempt that must be refused before its
        // `SESSION CREATE` is written, so the router answers the handshake and hangs up.
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::close(),
        ],
    )
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();

    let owner = client
        .create_shared_session(
            &SessionDestination::Transient,
            "owner",
            SharedDialect::Master,
            &[],
        )
        .await
        .unwrap();

    for reserved in [
        "PORT",
        "HOST",
        "FROM_PORT",
        "TO_PORT",
        "PROTOCOL",
        "LISTEN_PORT",
        "LISTEN_PROTOCOL",
        "HEADER",
    ] {
        let options = vec![(reserved.to_owned(), "1".to_owned())];
        let outcome = client
            .create_shared_session(
                &SessionDestination::Transient,
                "owner",
                SharedDialect::Master,
                &options,
            )
            .await;
        assert!(
            matches!(outcome, Err(SamError::Rejected(_))),
            "{reserved} must be refused on a shared session: it belongs on a subsession"
        );
    }
    for index in 2..bridge.accepted_connections() {
        bridge
            .connection(index)
            .assert_lacks(b"SESSION CREATE STYLE=MASTER");
    }

    let port = |value: &str| vec![("LISTEN_PORT".to_owned(), value.to_owned())];
    owner
        .add_child("dup", SessionStyle::Datagram, &port("20"))
        .await
        .unwrap();
    assert!(
        owner
            .add_child("dup", SessionStyle::Datagram, &port("21"))
            .await
            .is_err(),
        "a duplicate child ID must be refused"
    );
    owner
        .add_child("listener-a", SessionStyle::Datagram, &port("22"))
        .await
        .unwrap();
    assert!(
        owner
            .add_child("listener-b", SessionStyle::Datagram, &port("22"))
            .await
            .is_err(),
        "two subsessions may not claim the same listener tuple"
    );
    assert!(
        owner
            .add_child(
                "raw-streaming",
                SessionStyle::Raw,
                &[("LISTEN_PROTOCOL".to_owned(), "6".to_owned())],
            )
            .await
            .is_err(),
        "a RAW subsession may not advertise the streaming protocol"
    );

    // A RAW subsession that asks for nothing inconsistent must be creatable. Without this
    // the previous assertion would pass for the wrong reason, because the rejection above
    // could be an unrelated blanket refusal rather than the LISTEN_PROTOCOL rule.
    let raw = owner
        .add_child(
            "raw-ok",
            SessionStyle::Raw,
            &[
                ("LISTEN_PORT".to_owned(), "23".to_owned()),
                ("PROTOCOL".to_owned(), "16".to_owned()),
            ],
        )
        .await;
    assert!(
        raw.is_ok(),
        "a RAW subsession with its own listener and a legal protocol must be creatable, got {:?}",
        raw.err()
    );
    assert_eq!(
        raw.unwrap().identity().hash().to_string(),
        owner.identity().hash().to_string()
    );
    bridge.assert_scripts_clean();
    owner.close().await;
}
