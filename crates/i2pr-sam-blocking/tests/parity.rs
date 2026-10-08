//! Blocking parity checks reuse the async crate's scripted bridge and wire rules.

#[path = "../../i2pr-sam/tests/support/mod.rs"]
mod support;

use i2pr_sam::{ClientConfig, DatagramTransport, SamError, SessionDestination, SessionStyle};
use i2pr_sam_blocking::{BlockingClient, BlockingError};
use i2pr_sam_proto::{Port, ReceivedDatagram, SharedDialect, Support};
use std::{
    io::{Read, Write},
    net::UdpSocket,
    time::Duration,
};
use support::{
    HELLO_REPLY, Match, OWNER_DESTINATION, PEER_DESTINATION, Rule, SESSION_OK, Script,
    THIRD_DESTINATION, datagram_delivery, frame, naming_me_ok,
};

fn utility() -> Script {
    vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)]
}

fn fixture(scripts: Vec<Script>) -> (tokio::runtime::Runtime, support::MockBridge) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap();
    let bridge = runtime.block_on(support::MockBridge::start_with(scripts));
    (runtime, bridge)
}

fn blocking_client(bridge: &support::MockBridge) -> BlockingClient {
    BlockingClient::connect(bridge.client_config()).unwrap()
}

fn session_script(style: &str, id: &str, extra: Vec<Rule>) -> Script {
    let mut script = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::starts_with(format!("SESSION CREATE STYLE={style} ID={id}")),
            SESSION_OK,
        ),
        Rule::line(
            Match::exact("NAMING LOOKUP NAME=ME"),
            &naming_me_ok(OWNER_DESTINATION),
        ),
    ];
    script.extend(extra);
    script
}

fn forwarding_port(bridge: &support::MockBridge, connection: usize) -> u16 {
    bridge
        .connection(connection)
        .written_text()
        .split_whitespace()
        .find_map(|word| word.strip_prefix("PORT=")?.parse().ok())
        .expect("session must announce its local UDP forwarding port")
}

#[test]
fn blocking_destination_naming_and_capabilities_match_async_types() {
    let mut utility = utility();
    utility.extend([
        Rule::line(
            Match::starts_with("DEST GENERATE"),
            &format!("DEST REPLY PUB={OWNER_DESTINATION} PRIV=cm91dGVyLXNpZGUtcHJpdmF0ZS1rZXktYmxvYg==\n"),
        ),
        Rule::line(Match::exact("NAMING LOOKUP NAME=peer.i2p"), &format!("NAMING REPLY RESULT=OK NAME=peer.i2p VALUE={PEER_DESTINATION}\n")),
        Rule::line(Match::exact("NAMING LOOKUP NAME=peer.i2p"), &format!("NAMING REPLY RESULT=OK NAME=peer.i2p VALUE={PEER_DESTINATION}\n")),
        Rule::line(Match::exact("NAMING LOOKUP NAME=hash.i2p"), "NAMING REPLY RESULT=OK NAME=hash.i2p VALUE=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.b32.i2p\n"),
    ]);
    let (_fixture_runtime, bridge) = fixture(vec![utility]);
    let client = blocking_client(&bridge);
    let generated = client.generate_destination(None).unwrap();
    assert_eq!(generated.public().as_str(), OWNER_DESTINATION);
    assert!(format!("{generated:?}").contains("[REDACTED]"));
    assert_eq!(client.lookup("peer.i2p").unwrap(), PEER_DESTINATION);
    assert!(matches!(
        client.resolve_peer("peer.i2p").unwrap(),
        i2pr_sam::PeerTarget::Destination(_)
    ));
    assert!(matches!(
        client.resolve_peer("hash.i2p").unwrap(),
        i2pr_sam::PeerTarget::Base32Hash { .. }
    ));
    assert_eq!(client.capabilities().unwrap().datagram, Support::Unknown);
    bridge.assert_scripts_clean();
}

#[test]
fn blocking_destination_generation_and_naming_failures_are_not_silently_coerced() {
    let mut script = utility();
    script.extend([
        Rule::line(
            Match::starts_with("DEST GENERATE"),
            "DEST REPLY RESULT=INVALID_KEY\n",
        ),
        Rule::line(
            Match::exact("NAMING LOOKUP NAME=missing.i2p"),
            "NAMING REPLY RESULT=KEY_NOT_FOUND NAME=missing.i2p\n",
        ),
    ]);
    let (_fixture_runtime, bridge) = fixture(vec![script]);
    let client = blocking_client(&bridge);
    assert!(matches!(
        client.generate_destination(None),
        Err(BlockingError::Sam(_))
    ));
    assert!(matches!(
        client.resolve_peer("missing.i2p"),
        Err(BlockingError::Sam(_))
    ));
    bridge.assert_scripts_clean();
}

#[test]
fn blocking_control_socket_datagram1_and_raw_exchange_exact_payload_and_metadata() {
    let d1 = session_script(
        "DATAGRAM",
        "d1",
        vec![
            Rule::line(
                Match::exact(
                    "DATAGRAM SEND ID=d1 DESTINATION=peer.b32.i2p FROM_PORT=11 TO_PORT=22 SIZE=7",
                ),
                "DATAGRAM STATUS RESULT=OK MESSAGE=7\n",
            ),
            Rule::raw(7, b"payload"),
            Rule::push(datagram_delivery(THIRD_DESTINATION, 3, 4, b"reply")),
        ],
    );
    let raw_inbound = frame(&[
        b"RAW RECEIVED SIZE=4 FROM_PORT=5 TO_PORT=6 PROTOCOL=16\n",
        b"raw!",
    ]);
    let raw = session_script(
        "RAW",
        "raw",
        vec![
            Rule::line(
                Match::exact("RAW SEND ID=raw DESTINATION=peer.b32.i2p PROTOCOL=16 SIZE=7"),
                "RAW STATUS RESULT=OK MESSAGE=7\n",
            ),
            Rule::raw(7, b"payload"),
            Rule::push(raw_inbound),
        ],
    );
    let (_fixture_runtime, bridge) = fixture(vec![utility(), d1, raw]);
    let client = blocking_client(&bridge);

    let d1 = client
        .create_datagram_session_with(
            &SessionDestination::Transient,
            "d1",
            SessionStyle::Datagram,
            &[],
            DatagramTransport::ControlSocketV1,
        )
        .unwrap();
    assert_eq!(d1.transport(), DatagramTransport::ControlSocketV1);
    d1.send(
        "peer.b32.i2p",
        b"payload",
        Some(Port::new(11)),
        Some(Port::new(22)),
    )
    .unwrap();
    match d1.recv().unwrap() {
        ReceivedDatagram::Authenticated(message) => {
            assert_eq!(message.source.as_str(), THIRD_DESTINATION);
            assert_eq!((message.from_port.get(), message.to_port.get()), (3, 4));
            assert_eq!(message.payload, b"reply");
        }
        other => panic!("DATAGRAM1 must preserve authenticated source: {other:?}"),
    }

    let raw = client
        .create_datagram_session_with(
            &SessionDestination::Transient,
            "raw",
            SessionStyle::Raw,
            &[("PROTOCOL".into(), "16".into())],
            DatagramTransport::ControlSocketV1,
        )
        .unwrap();
    raw.send("peer.b32.i2p", b"payload", None, None).unwrap();
    match raw.recv().unwrap() {
        ReceivedDatagram::Raw(message) => {
            assert_eq!(
                (
                    message.from_port.get(),
                    message.to_port.get(),
                    message.protocol.get()
                ),
                (5, 6, 16)
            );
            assert_eq!(message.payload, b"raw!");
        }
        other => panic!("RAW must not claim a source identity: {other:?}"),
    }
    bridge.assert_wrote(
        b"DATAGRAM SEND ID=d1 DESTINATION=peer.b32.i2p FROM_PORT=11 TO_PORT=22 SIZE=7\npayload",
    );
    bridge.assert_wrote(b"RAW SEND ID=raw DESTINATION=peer.b32.i2p PROTOCOL=16 SIZE=7\npayload");
    bridge.assert_scripts_clean();
}

#[test]
fn blocking_udp_datagram_raw_d2_and_d3_keep_payload_and_trust_semantics() {
    let datagram = session_script("DATAGRAM", "d1", vec![]);
    let raw = session_script("RAW", "raw", vec![]);
    let d2 = session_script("DATAGRAM2", "d2", vec![]);
    let d3 = session_script("DATAGRAM3", "d3", vec![]);
    let (_fixture_runtime, bridge) = fixture(vec![utility(), datagram, raw, d2, d3]);
    let client = blocking_client(&bridge);
    let source_hash = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    for (index, id, style, source) in [
        (1, "d1", SessionStyle::Datagram, THIRD_DESTINATION),
        (2, "raw", SessionStyle::Raw, ""),
        (3, "d2", SessionStyle::Datagram2, THIRD_DESTINATION),
        (4, "d3", SessionStyle::Datagram3, source_hash),
    ] {
        let session = if style == SessionStyle::Raw {
            client.create_datagram_session_with(
                &SessionDestination::Transient,
                id,
                style,
                &[
                    ("HEADER".into(), "true".into()),
                    ("PROTOCOL".into(), "16".into()),
                ],
                DatagramTransport::UdpForward,
            )
        } else {
            client.create_datagram_session(&SessionDestination::Transient, id, style, &[])
        }
        .unwrap();
        let mut session = session;
        session.set_recv_timeout(Duration::from_secs(2));
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let local_port = forwarding_port(&bridge, index);
        let wire = if style == SessionStyle::Raw {
            b"FROM_PORT=7\nTO_PORT=8\nPROTOCOL=16\n\nwire-raw".to_vec()
        } else {
            format!("{source} FROM_PORT=7 TO_PORT=8\nwire-{id}").into_bytes()
        };
        sender.send_to(&wire, ("127.0.0.1", local_port)).unwrap();
        match session.recv().unwrap() {
            ReceivedDatagram::Authenticated(message)
                if style == SessionStyle::Datagram || style == SessionStyle::Datagram2 =>
            {
                assert_eq!(message.source.as_str(), THIRD_DESTINATION);
                assert_eq!(message.payload, format!("wire-{id}").as_bytes());
            }
            ReceivedDatagram::Unverified(message) if style == SessionStyle::Datagram3 => {
                assert_eq!(message.source_hash.as_bytes(), &[0; 32]);
                assert_eq!(message.payload, b"wire-d3");
            }
            ReceivedDatagram::Raw(message) if style == SessionStyle::Raw => {
                assert_eq!(
                    (
                        message.from_port.get(),
                        message.to_port.get(),
                        message.protocol.get()
                    ),
                    (7, 8, 16)
                );
                assert_eq!(message.payload, b"wire-raw");
            }
            other => panic!("unexpected source trust for {style:?}: {other:?}"),
        }
        drop(sender);
    }
    bridge.assert_scripts_clean();
}

#[test]
fn blocking_rejects_legacy_transport_for_datagram2_and_datagram3_before_io() {
    let (_fixture_runtime, bridge) = fixture(vec![utility()]);
    let client = blocking_client(&bridge);
    for (id, style) in [
        ("d2", SessionStyle::Datagram2),
        ("d3", SessionStyle::Datagram3),
    ] {
        assert!(matches!(
            client.create_datagram_session_with(
                &SessionDestination::Transient,
                id,
                style,
                &[],
                DatagramTransport::ControlSocketV1
            ),
            Err(BlockingError::Sam(SamError::Unsupported(_)))
        ));
    }
    assert_eq!(
        bridge.accepted_connections(),
        1,
        "forbidden modes must fail before opening a control socket"
    );
    bridge.assert_lacks(b"SESSION CREATE STYLE=DATAGRAM2");
    bridge.assert_lacks(b"SESSION CREATE STYLE=DATAGRAM3");
}

#[test]
fn blocking_shared_owner_identity_child_stream_and_datagram_lifecycle_match_async() {
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut config = ClientConfig::new("127.0.0.1:1".parse().unwrap());
    config.datagram_endpoint = receiver.local_addr().unwrap();
    let owner_script = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::starts_with("SESSION CREATE STYLE=MASTER ID=owner"),
            SESSION_OK,
        ),
        Rule::line(
            Match::exact("NAMING LOOKUP NAME=ME"),
            &naming_me_ok(OWNER_DESTINATION),
        ),
        Rule::line(Match::starts_with("SESSION ADD"), SESSION_OK),
        Rule::line(Match::starts_with("SESSION ADD"), SESSION_OK),
        Rule::line(Match::starts_with("SESSION ADD"), SESSION_OK),
        Rule::line(
            Match::exact("SESSION REMOVE ID=udp-child"),
            "SESSION STATUS RESULT=OK\n",
        ),
    ];
    let stream_data = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::starts_with("STREAM CONNECT ID=stream-child"),
            "STREAM STATUS RESULT=OK\n",
        ),
        Rule::raw(4, b"ping"),
        Rule::push(b"pong".to_vec()),
    ];
    let accept_data = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::exact("STREAM ACCEPT ID=inbound-child"),
            "STREAM STATUS RESULT=OK\n",
        ),
        Rule::push(support::bytes(&format!(
            "{PEER_DESTINATION}\nFROM_PORT=9\nTO_PORT=10\n\naccepted"
        ))),
    ];
    let (_fixture_runtime, bridge) =
        fixture(vec![utility(), owner_script, stream_data, accept_data]);
    config.endpoint = bridge.endpoint();
    let client = BlockingClient::connect(config).unwrap();
    let owner = client
        .create_shared_session(
            &SessionDestination::Transient,
            "owner",
            SharedDialect::Master,
            &[],
        )
        .unwrap();
    let identity = owner.identity().clone();
    assert_eq!(identity.destination().as_str(), OWNER_DESTINATION);
    let udp = owner
        .add_child("udp-child", SessionStyle::Datagram, &[])
        .unwrap();
    let stream_child = owner
        .add_child("stream-child", SessionStyle::Stream, &[])
        .unwrap();
    let inbound_child = owner
        .add_child(
            "inbound-child",
            SessionStyle::Stream,
            &[
                ("LISTEN_PORT".into(), "7777".into()),
                ("FROM_PORT".into(), "7777".into()),
            ],
        )
        .unwrap();
    assert!(
        owner
            .add_child("udp-child", SessionStyle::Datagram, &[])
            .is_err()
    );
    assert_eq!(udp.identity(), &identity);
    assert_eq!(stream_child.identity(), &identity);
    assert_eq!(inbound_child.identity(), &identity);

    udp.send_datagram("peer.b32.i2p", b"shared-udp", None, None)
        .unwrap();
    let (packet_tx, packet_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut packet = [0; 128];
        let result = receiver
            .recv_from(&mut packet)
            .map(|(size, _)| packet[..size].to_vec());
        let _ = packet_tx.send(result);
    });
    let packet = packet_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(packet, b"3.0 udp-child peer.b32.i2p\nshared-udp");
    owner.remove_child("udp-child").unwrap();
    assert!(
        udp.send_datagram("peer.b32.i2p", b"gone", None, None)
            .is_err()
    );

    let mut stream = stream_child.connect("peer.b32.i2p", None, None).unwrap();
    stream.write_all(b"ping").unwrap();
    let mut pong = [0; 4];
    stream.read_exact(&mut pong).unwrap();
    assert_eq!(&pong, b"pong", "sibling remains usable after removal");
    let mut accepted = inbound_child.accept().unwrap();
    assert_eq!(
        accepted.remote_destination().unwrap().as_str(),
        PEER_DESTINATION
    );
    let mut accepted_payload = [0; 8];
    accepted.read_exact(&mut accepted_payload).unwrap();
    assert_eq!(&accepted_payload, b"accepted");
    owner.close().unwrap();
    assert!(
        stream_child.connect("peer.b32.i2p", None, None).is_err(),
        "owner close invalidates children"
    );
    bridge.assert_scripts_clean();
}

#[test]
fn blocking_timeouts_and_close_are_terminal_and_idempotent() {
    let stream_data = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::starts_with("STREAM CONNECT"),
            "STREAM STATUS RESULT=OK\n",
        ),
        Rule::delay(Duration::from_millis(100)),
    ];
    let datagram = session_script("DATAGRAM", "timeout", vec![]);
    let stream_session = session_script("STREAM", "stream", vec![]);
    let (_fixture_runtime, bridge) =
        fixture(vec![utility(), datagram, stream_session, stream_data]);
    let client = blocking_client(&bridge);
    let mut dg = client
        .create_datagram_session(
            &SessionDestination::Transient,
            "timeout",
            SessionStyle::Datagram,
            &[],
        )
        .unwrap();
    dg.set_recv_timeout(Duration::from_millis(10));
    assert!(matches!(dg.recv(), Err(BlockingError::Timeout)));
    dg.close().unwrap();
    dg.close().unwrap();

    let mut session = client
        .create_stream_session(&SessionDestination::Transient, "stream", &[])
        .unwrap();
    session.set_io_timeout(Duration::from_millis(10));
    let mut stream = session.connect("peer.b32.i2p", None, None).unwrap();
    let mut byte = [0];
    assert_eq!(
        stream.read(&mut byte).unwrap_err().kind(),
        std::io::ErrorKind::TimedOut
    );
    session.close().unwrap();
    session.close().unwrap();
    bridge.assert_scripts_clean();
}

#[tokio::test]
async fn blocking_calls_reject_nested_tokio_runtime() {
    let config = ClientConfig::new("127.0.0.1:1".parse().unwrap());
    let error =
        BlockingClient::connect_with_policy(config, i2pr_sam::ConnectRetryPolicy::default())
            .err()
            .unwrap();
    assert!(matches!(error, BlockingError::NestedRuntime));
}

#[test]
fn blocking_unsupported_style_and_transient_router_errors_keep_error_classes() {
    let unsupported = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::starts_with("SESSION CREATE"),
            "SESSION STATUS RESULT=INVALID_STYLE\n",
        ),
    ];
    let transient = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::starts_with("SESSION CREATE"),
            "SESSION STATUS RESULT=CANT_REACH_PEER\n",
        ),
    ];
    let (_fixture_runtime, bridge) = fixture(vec![utility(), unsupported, transient]);
    let client = blocking_client(&bridge);
    assert!(matches!(
        client.create_datagram_session(
            &SessionDestination::Transient,
            "unsupported",
            SessionStyle::Datagram2,
            &[]
        ),
        Err(BlockingError::Sam(SamError::Unsupported(_)))
    ));
    assert_eq!(
        client.capabilities().unwrap().datagram2,
        Support::Unsupported
    );
    assert!(matches!(
        client.create_datagram_session(
            &SessionDestination::Transient,
            "transient",
            SessionStyle::Datagram,
            &[]
        ),
        Err(BlockingError::Sam(SamError::Rejected(_)))
    ));
    bridge.assert_scripts_clean();
}

#[test]
fn blocking_stream_accept_and_destination_parity_are_preserved() {
    let session = session_script("STREAM", "accept", vec![]);
    let accept = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(
            Match::exact("STREAM ACCEPT ID=accept"),
            "STREAM STATUS RESULT=OK\n",
        ),
        Rule::push(support::bytes(&format!(
            "{PEER_DESTINATION}\nFROM_PORT=5\nTO_PORT=6\n\naccepted"
        ))),
    ];
    let (_fixture_runtime, bridge) = fixture(vec![utility(), session, accept]);
    let client = blocking_client(&bridge);
    let stream_session = client
        .create_stream_session(&SessionDestination::Transient, "accept", &[])
        .unwrap();
    let mut stream = stream_session.accept().unwrap();
    assert_eq!(
        stream.remote_destination().unwrap().as_str(),
        PEER_DESTINATION
    );
    assert_eq!(stream.peer().unwrap().from_port.map(Port::get), Some(5));
    let mut payload = [0; 8];
    stream.read_exact(&mut payload).unwrap();
    assert_eq!(&payload, b"accepted");
    bridge.assert_scripts_clean();
}
