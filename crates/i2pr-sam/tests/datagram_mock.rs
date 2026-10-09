//! Datagram transport regression tests.
//!
//! Defects closed here:
//!
//! * **E** — ordinary DATAGRAM1 without a forwarding `PORT` receives through the
//!   v1/v2-compatible control socket: `DATAGRAM SEND ID=.. DESTINATION=.. [FROM_PORT=]
//!   [TO_PORT=] SIZE=<n>\n` followed by exactly `n` **raw** bytes (never base64). The
//!   bridge answers nothing for a v1 send, so the send completes once the bytes reach
//!   the socket; delivery is proven by receipt, never by a reply line. Inbound is an
//!   unsolicited `DATAGRAM RECEIVED DESTINATION=<d> SIZE=<n> [FROM_PORT=] [TO_PORT=]\n`
//!   plus `n` raw bytes.
//! * **F** — the RAW form of the same transport, with `PROTOCOL=` preserved and no source.
//! * **G** — that transport is only legal for STYLE=DATAGRAM and STYLE=RAW. DATAGRAM2,
//!   DATAGRAM3 and every shared subsession must be refused before any command is written.
//! * **H** — the control-socket receive queue is bounded by both count and bytes, so a
//!   flood drops whole deliveries and never interleaves or corrupts one.
//! * **K** — a forwarded RAW datagram carries a FROM_PORT/TO_PORT/PROTOCOL block only when
//!   the session was created with `HEADER=true`.
//! * **L** — a UDP-forwarded session keeps its control socket open for its whole lifetime:
//!   SAM sessions live and die with that socket, so dropping it at creation time kills the
//!   router-side session and every later send fails with session-not-found. `close()` must
//!   release it.

mod support;

use std::time::Duration;
use support::{
    HELLO_REPLY, Match, OWNER_DESTINATION, Rule, SESSION_OK, THIRD_DESTINATION, datagram_delivery,
    naming_me_ok,
};
use tokio::net::UdpSocket;

const DATAGRAM_SEND_LINE: &str =
    "DATAGRAM SEND ID=dg DESTINATION=peer.b32.i2p FROM_PORT=1 TO_PORT=2 SIZE=7\n";
const DATAGRAM_PAYLOAD: &[u8] = b"payload";

/// E: `DATAGRAM SEND` carries raw payload bytes on the same socket that carries the reply.
#[tokio::test]
async fn e_control_socket_datagram1_sends_raw_bytes_and_receives_an_announced_source() {
    let inbound_payload = b"inbound-7";
    let inbound = datagram_delivery(THIRD_DESTINATION, 1, 2, inbound_payload);
    let bridge = support::MockBridge::start_with(vec![
        vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            // No PORT and no HOST: this transport carries the datagram itself, so the client
            // must not ask the router to forward UDP on its behalf.
            Rule::line(
                Match::exact(
                    "SESSION CREATE STYLE=DATAGRAM ID=dg DESTINATION=TRANSIENT SIGNATURE_TYPE=7",
                ),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
            // No reply rule: the bridge answers nothing for a v1 send. If the client
            // waited for a reply line here, this test would time out.
            Rule::line(Match::exact(DATAGRAM_SEND_LINE.trim_end_matches('\n')), ""),
            Rule::raw(DATAGRAM_PAYLOAD.len(), DATAGRAM_PAYLOAD),
            Rule::push(inbound),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "dg",
            i2pr_sam_proto::SessionStyle::Datagram,
            i2pr_sam::DatagramTransport::ControlSocketV1,
            &[],
        )
        .await
        .unwrap();

    session
        .send(
            "peer.b32.i2p",
            DATAGRAM_PAYLOAD,
            Some(i2pr_sam_proto::Port::new(1)),
            Some(i2pr_sam_proto::Port::new(2)),
        )
        .await
        .unwrap();

    match session.recv().await.unwrap() {
        i2pr_sam_proto::ReceivedDatagram::Authenticated(message) => {
            assert_eq!(
                message.source.as_str(),
                THIRD_DESTINATION,
                "a DATAGRAM1 delivery must keep its announced source Destination"
            );
            assert_eq!(message.from_port.get(), 1);
            assert_eq!(message.to_port.get(), 2);
            assert_eq!(message.payload, inbound_payload);
        }
        other => panic!("DATAGRAM1 must decode as authenticated, not {other:?}"),
    }
    // Exact bytes: the header line then seven raw payload bytes, with no base64 framing.
    // These assertions run after recv() so the mock is guaranteed to have consumed the
    // send: a v1 send carries no reply to synchronize on.
    bridge.connection(1).assert_wrote(
        b"DATAGRAM SEND ID=dg DESTINATION=peer.b32.i2p FROM_PORT=1 TO_PORT=2 SIZE=7\npayload",
    );
    bridge.connection(1).assert_lacks(b"payload\n");
    assert_eq!(
        bridge.connection(1).occurrences(b"DATAGRAM SEND ID=dg"),
        1,
        "one send must produce exactly one header line"
    );
    bridge.assert_scripts_clean();
    session.close().await;
}

/// F: the RAW form preserves `PROTOCOL` and never invents a source identity.
#[tokio::test]
async fn f_control_socket_raw_preserves_protocol_and_carries_no_source_identity() {
    let inbound_payload = b"in!b";
    // SIZE governs the body: the client must read exactly that many bytes, so the declared
    // size always matches the payload the fixture sends.
    let inbound = frame_of_raw_delivery(inbound_payload.len(), 7, 9, 16, inbound_payload);
    let bridge = support::MockBridge::start_with(vec![
        vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=RAW ID=rw"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
            // No reply rule: the bridge answers nothing for a v1 send. If the client
            // waited for a reply line here, the send below would time out.
            Rule::line(
                Match::exact("RAW SEND ID=rw DESTINATION=peer.b32.i2p PROTOCOL=16 SIZE=5"),
                "",
            ),
            Rule::raw(5, b"bytes"),
            Rule::push(inbound),
        ],
    ])
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "rw",
            i2pr_sam_proto::SessionStyle::Raw,
            i2pr_sam::DatagramTransport::ControlSocketV1,
            &[("PROTOCOL".to_owned(), "16".to_owned())],
        )
        .await
        .unwrap();

    session
        .send("peer.b32.i2p", b"bytes", None, None)
        .await
        .unwrap();

    match session.recv().await.unwrap() {
        i2pr_sam_proto::ReceivedDatagram::Raw(message) => {
            assert_eq!(
                message.protocol.get(),
                16,
                "RAW must preserve the protocol the router announced"
            );
            assert_eq!(message.from_port.get(), 7);
            assert_eq!(message.to_port.get(), 9);
            assert_eq!(message.payload, inbound_payload);
        }
        other => panic!("RAW carries no source identity, so it must decode as Raw, not {other:?}"),
    }
    // Wire assertions run after recv() so the mock is guaranteed to have consumed the
    // send: a v1 send carries no reply to synchronize on.
    bridge
        .connection(1)
        .assert_wrote(b"RAW SEND ID=rw DESTINATION=peer.b32.i2p PROTOCOL=16 SIZE=5\nbytes");
    bridge.assert_scripts_clean();
    session.close().await;
}

/// G: the v1/v2-compatible control socket is illegal for DATAGRAM2/3 and shared subsessions.
#[tokio::test]
async fn g_control_socket_transport_is_refused_for_datagram2_datagram3_and_shared_children() {
    use i2pr_sam::{DatagramTransport, SamError, SessionDestination, SessionStyle};
    let bridge = support::MockBridge::start().await;
    let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut config = bridge.client_config();
    config.datagram_endpoint = receiver.local_addr().unwrap();
    let client = i2pr_sam::SamClient::connect(config).await.unwrap();
    let opened_after_connect = bridge.accepted_connections();

    for (style, id) in [
        (SessionStyle::Datagram2, "d2"),
        (SessionStyle::Datagram3, "d3"),
        (SessionStyle::Stream, "st"),
    ] {
        let outcome = client
            .create_datagram_session_with(
                &SessionDestination::Transient,
                id,
                style,
                DatagramTransport::ControlSocketV1,
                &[],
            )
            .await;
        assert!(
            matches!(outcome, Err(SamError::Unsupported(_))),
            "{style:?} must refuse the v1/v2-compatible control socket as unsupported"
        );
    }
    assert_eq!(
        bridge.accepted_connections(),
        opened_after_connect,
        "the transport must be refused before any connection reaches the router"
    );
    bridge.assert_lacks(b"SESSION CREATE STYLE=DATAGRAM2");
    bridge.assert_lacks(b"SESSION CREATE STYLE=DATAGRAM3");

    // A shared subsession has no control-socket datagram path at all: it forwards over UDP.
    assert!(
        !SessionStyle::Datagram2.supports_control_socket_datagram(),
        "DATAGRAM2 is excluded by specification"
    );
    assert!(
        !SessionStyle::Datagram3.supports_control_socket_datagram(),
        "DATAGRAM3 is excluded by specification"
    );
    let owner = client
        .create_shared_session(
            &SessionDestination::Transient,
            "owner",
            i2pr_sam_proto::SharedDialect::Master,
            &[],
        )
        .await
        .unwrap();
    let child = owner
        .add_child("udp-child", SessionStyle::Datagram, &[])
        .await
        .unwrap();
    child
        .send_datagram("peer.b32.i2p", b"udp-payload", None, None)
        .await
        .unwrap();

    let mut packet = [0u8; 512];
    let (size, _) = tokio::time::timeout(Duration::from_secs(3), receiver.recv_from(&mut packet))
        .await
        .expect("shared child datagram timed out")
        .unwrap();
    assert_eq!(
        &packet[..size],
        b"3.0 udp-child peer.b32.i2p\nudp-payload",
        "a shared subsession must forward datagrams over UDP, not the control socket"
    );
    assert!(
        !bridge.wrote(b"DATAGRAM SEND"),
        "no connection may carry a v1/v2-compatible DATAGRAM SEND for a shared session"
    );
    assert!(!bridge.wrote(b"RAW SEND"));
    bridge.assert_scripts_clean();
    owner.close().await;
}

/// H: the bounded queue drops whole deliveries and never corrupts the ones it keeps.
#[tokio::test]
async fn h_bounded_control_socket_queue_drops_whole_datagrams_under_flood() {
    const QUEUE_DEPTH: usize = 2;
    const FLOOD: usize = 8;
    let frames: Vec<Vec<u8>> = (0..FLOOD)
        .map(|index| datagram_delivery(THIRD_DESTINATION, 1, 2, format!("{index:04}").as_bytes()))
        .collect();
    let bridge = support::MockBridge::start_with(vec![
        vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=DATAGRAM ID=flood"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
            // The router outruns the reader: nothing is read back while this floods.
            Rule::flood(frames),
        ],
    ])
    .await;
    let mut config = bridge.client_config();
    config.max_datagram_bytes = 64;
    config.max_inbox_bytes = 64;
    config.max_inbox_datagrams = QUEUE_DEPTH;
    let client = i2pr_sam::SamClient::connect(config).await.unwrap();
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "flood",
            i2pr_sam_proto::SessionStyle::Datagram,
            i2pr_sam::DatagramTransport::ControlSocketV1,
            &[],
        )
        .await
        .unwrap();

    // Wait until every delivery is either queued or counted as dropped. The loop only
    // advances on the client's own counters, so there is no wall-clock assumption.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let (queued, dropped) = loop {
        let queued = session.queued_datagrams().await;
        let dropped = session.dropped_datagrams().await;
        if queued + dropped as usize == FLOOD {
            break (queued, dropped);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the flood never finished: {queued} queued, {dropped} dropped"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    };

    assert!(
        dropped > 0,
        "a flood past the queue bound must be counted as dropped"
    );
    assert!(
        queued <= QUEUE_DEPTH,
        "the queue must never exceed its configured depth, saw {queued}"
    );
    assert_eq!(dropped as usize, FLOOD - queued);

    // Whatever survived must still be individually well-formed and in order.
    for expected in 0..queued {
        match session.recv().await.unwrap() {
            i2pr_sam_proto::ReceivedDatagram::Authenticated(message) => {
                assert_eq!(message.source.as_str(), THIRD_DESTINATION);
                assert_eq!(message.from_port.get(), 1);
                assert_eq!(message.to_port.get(), 2);
                assert_eq!(
                    message.payload,
                    format!("{expected:04}").into_bytes(),
                    "a queued datagram must be intact, never interleaved with another"
                );
            }
            other => panic!("flooded DATAGRAM1 must decode as authenticated, not {other:?}"),
        }
    }
    assert_eq!(session.queued_datagrams().await, 0);
    bridge.assert_scripts_clean();
    session.close().await;
}

/// K (header on): a forwarded RAW datagram keeps the metadata block the router sent.
#[tokio::test]
async fn k_forwarded_raw_with_header_true_preserves_ports_and_protocol() {
    let bridge = forwarded_raw_bridge("rwh").await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "rwh",
            i2pr_sam_proto::SessionStyle::Raw,
            i2pr_sam::DatagramTransport::UdpForward,
            &[
                ("HEADER".to_owned(), "true".to_owned()),
                ("PROTOCOL".to_owned(), "16".to_owned()),
            ],
        )
        .await
        .unwrap();

    let forwarding_port = forwarding_port(&bridge, 1);
    UdpSocket::bind("127.0.0.1:0")
        .await
        .unwrap()
        .send_to(
            b"FROM_PORT=5\nTO_PORT=6\nPROTOCOL=16\n\nheadered-payload",
            ("127.0.0.1", forwarding_port),
        )
        .await
        .unwrap();

    match session.recv().await.unwrap() {
        i2pr_sam_proto::ReceivedDatagram::Raw(message) => {
            assert_eq!(message.from_port.get(), 5);
            assert_eq!(message.to_port.get(), 6);
            assert_eq!(message.protocol.get(), 16);
            assert_eq!(message.payload, b"headered-payload");
        }
        other => panic!("forwarded RAW must decode as Raw, not {other:?}"),
    }
    bridge
        .connection(1)
        .assert_wrote(b"sam.udp.port=7655 HEADER=true PROTOCOL=16\n");
    bridge.assert_scripts_clean();
    session.close().await;
}

/// K (header off): a forwarded RAW datagram is bare payload and the ports are not invented.
#[tokio::test]
async fn k_forwarded_raw_without_header_is_bare_payload_with_no_invented_metadata() {
    let bridge = forwarded_raw_bridge("rwnh").await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "rwnh",
            i2pr_sam_proto::SessionStyle::Raw,
            i2pr_sam::DatagramTransport::UdpForward,
            &[
                ("HEADER".to_owned(), "false".to_owned()),
                ("PROTOCOL".to_owned(), "16".to_owned()),
            ],
        )
        .await
        .unwrap();

    let forwarding_port = forwarding_port(&bridge, 1);
    UdpSocket::bind("127.0.0.1:0")
        .await
        .unwrap()
        .send_to(
            b"bare-payload-with-no-header-block",
            ("127.0.0.1", forwarding_port),
        )
        .await
        .unwrap();

    match session.recv().await.unwrap() {
        i2pr_sam_proto::ReceivedDatagram::Raw(message) => {
            assert_eq!(
                message.payload, b"bare-payload-with-no-header-block",
                "without HEADER=true the router sends no metadata block"
            );
            assert_eq!(
                message.from_port.get(),
                0,
                "ports must not be invented for a datagram that carried none"
            );
            assert_eq!(message.to_port.get(), 0);
            assert_eq!(
                message.protocol.get(),
                16,
                "the protocol comes from the session configuration, not the wire"
            );
        }
        other => panic!("forwarded RAW must decode as Raw, not {other:?}"),
    }
    bridge
        .connection(1)
        .assert_wrote(b"HEADER=false PROTOCOL=16\n");
    bridge.assert_scripts_clean();
    session.close().await;
}

/// One `RAW RECEIVED` delivery: header line then exactly `SIZE` raw payload bytes.
fn frame_of_raw_delivery(
    size: usize,
    from_port: u16,
    to_port: u16,
    protocol: u8,
    payload: &[u8],
) -> Vec<u8> {
    let header = format!(
        "RAW RECEIVED SIZE={size} FROM_PORT={from_port} TO_PORT={to_port} PROTOCOL={protocol}\n"
    );
    let mut frame = header.into_bytes();
    frame.extend_from_slice(payload);
    frame
}

/// The UDP forwarding port the client advertised in its `SESSION CREATE` line.
fn forwarding_port(bridge: &support::MockBridge, connection: usize) -> u16 {
    bridge
        .connection(connection)
        .written_text()
        .split_whitespace()
        .find_map(|word| word.strip_prefix("PORT=")?.parse().ok())
        .expect("the client must advertise its UDP forwarding PORT")
}

/// Bridge whose session connection captures the forwarding `PORT` the client advertised.
async fn forwarded_raw_bridge(id: &str) -> support::MockBridge {
    support::MockBridge::start_with(vec![
        vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::capture(
                Match::starts_with(format!("SESSION CREATE STYLE=RAW ID={id}")),
                "PORT",
                support::TextSlot::new(),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
        ],
    ])
    .await
}

/// L: a UDP-forwarded session must not close its control socket at creation time.
#[tokio::test]
async fn l_forwarded_datagram_session_keeps_its_control_socket_open_until_close() {
    let bridge = support::MockBridge::start_with(vec![
        vec![Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY)],
        vec![
            Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=DATAGRAM ID=keep"),
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
    let session = client
        .create_datagram_session_with(
            &i2pr_sam::SessionDestination::Transient,
            "keep",
            i2pr_sam_proto::SessionStyle::Datagram,
            i2pr_sam::DatagramTransport::UdpForward,
            &[],
        )
        .await
        .unwrap();
    bridge
        .connection(1)
        .assert_wrote(b"HOST=127.0.0.1 sam.udp.host=127.0.0.1 sam.udp.port=7655\n");
    // Give a dropped socket time to deliver its FIN: without the retained control
    // socket the bridge observes the close within milliseconds.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        bridge.live_connections(),
        2,
        "the utility and session control sockets must both stay open; \
         closing the session socket kills the router-side session"
    );

    session.close().await;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while bridge.live_connections() != 1 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        bridge.live_connections(),
        1,
        "close() must release the session control socket"
    );
    bridge.assert_scripts_clean();
}
