//! The blocking facade must reproduce the async client's stream framing exactly.
//!
//! This is the parity check for the same defect the async suite covers in
//! `stream_mock.rs`: a non-silent accept consumes the peer identity block before payload, and
//! a connect consumes nothing. The mock here is a plain `std::net` server on a thread because
//! the facade refuses to run inside a Tokio runtime.

use i2pr_sam::SessionDestination;
use i2pr_sam_blocking::BlockingClient;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

/// The fixture peer Destination. Padding-free so the framing assertions here do not depend on
/// how the peer-block reader distinguishes a Destination from a `KEY=VALUE` line.
const PEER_DESTINATION: &str = "cGVlci1pZGVudGl0eS1maXh0dXJlLTMzYnl0ZXMhIS4u";
const OWNER_DESTINATION: &str = "b3duZXItaWRlbnRpdHktZml4dHVyZS0zMmJ5dGVzIQA=";

fn read_line(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut one = [0];
    loop {
        stream.read_exact(&mut one).unwrap();
        bytes.push(one[0]);
        if one[0] == b'\n' {
            break;
        }
    }
    String::from_utf8(bytes).unwrap()
}

fn hello(stream: &mut TcpStream) {
    assert!(
        read_line(stream).starts_with("HELLO VERSION MIN=3.1"),
        "the facade must send the same HELLO the async client sends"
    );
    stream
        .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3 MAJOR=3 MINOR=3\n")
        .unwrap();
}

/// `STREAM CONNECT` has no identity block: payload starts at the byte after the status line.
#[test]
fn blocking_stream_connect_starts_payload_immediately_after_the_status_line() {
    const PAYLOAD: &[u8] = b"blocking-payload";
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut utility, _) = listener.accept().unwrap();
        hello(&mut utility);

        let (mut session, _) = listener.accept().unwrap();
        hello(&mut session);
        assert!(read_line(&mut session).starts_with("SESSION CREATE STYLE=STREAM ID=blocking"));
        session
            .write_all(b"SESSION STATUS RESULT=OK EXPIRES=3600 DESTINATION=YmxvYg==\n")
            .unwrap();
        assert_eq!(read_line(&mut session), "NAMING LOOKUP NAME=ME\n");
        session
            .write_all(
                format!("NAMING REPLY RESULT=OK NAME=ME VALUE={OWNER_DESTINATION}\n").as_bytes(),
            )
            .unwrap();

        let (mut stream, _) = listener.accept().unwrap();
        hello(&mut stream);
        assert!(read_line(&mut stream).starts_with("STREAM CONNECT ID=blocking"));
        stream.write_all(b"STREAM STATUS RESULT=OK\n").unwrap();
        let mut data = [0; 4];
        stream.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"ping");
        stream.write_all(PAYLOAD).unwrap();
    });

    let client = BlockingClient::connect_endpoint(addr).unwrap();
    let session = client
        .create_stream_session(&SessionDestination::Transient, "blocking", &[])
        .unwrap();
    let mut stream = session.connect("peer.b32.i2p", None, None).unwrap();
    assert!(
        stream.remote_destination().is_none(),
        "STREAM CONNECT announces no peer, so the facade must not invent one"
    );
    stream.write_all(b"ping").unwrap();
    let mut payload = vec![0; PAYLOAD.len()];
    stream.read_exact(&mut payload).unwrap();
    assert_eq!(
        payload, PAYLOAD,
        "the status line must not be treated as payload"
    );
    session.close().unwrap();
    drop(stream);
    server.join().unwrap();
}

/// A non-silent accept consumes the peer identity block, so application data starts at byte
/// zero and the peer is reported.
#[test]
fn blocking_stream_accept_consumes_the_peer_identity_block_before_payload() {
    const PAYLOAD: &[u8] = b"accepted-blocking-payload";
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut utility, _) = listener.accept().unwrap();
        hello(&mut utility);

        let (mut session, _) = listener.accept().unwrap();
        hello(&mut session);
        let _ = read_line(&mut session);
        session
            .write_all(b"SESSION STATUS RESULT=OK EXPIRES=3600 DESTINATION=YmxvYg==\n")
            .unwrap();
        let _ = read_line(&mut session);
        session
            .write_all(
                format!("NAMING REPLY RESULT=OK NAME=ME VALUE={OWNER_DESTINATION}\n").as_bytes(),
            )
            .unwrap();

        let (mut incoming, _) = listener.accept().unwrap();
        hello(&mut incoming);
        assert_eq!(read_line(&mut incoming), "STREAM ACCEPT ID=blocking\n");
        incoming
            .write_all(
                format!("STREAM STATUS RESULT=OK\n{PEER_DESTINATION}\nFROM_PORT=5\nTO_PORT=6\n\n")
                    .as_bytes(),
            )
            .unwrap();
        incoming.write_all(PAYLOAD).unwrap();
    });

    let client = BlockingClient::connect_endpoint(addr).unwrap();
    let session = client
        .create_stream_session(&SessionDestination::Transient, "blocking", &[])
        .unwrap();
    let mut stream = session.accept().unwrap();
    let peer = stream
        .peer()
        .expect("a non-silent accept must report the announced peer");
    assert_eq!(peer.destination.as_str(), PEER_DESTINATION);
    assert_eq!(peer.from_port.map(|port| port.get()), Some(5));
    assert_eq!(peer.to_port.map(|port| port.get()), Some(6));
    assert_eq!(
        stream.remote_destination().map(|d| d.as_str()),
        Some(PEER_DESTINATION)
    );
    let mut payload = vec![0; PAYLOAD.len()];
    stream.read_exact(&mut payload).unwrap();
    assert_eq!(
        payload, PAYLOAD,
        "the peer identity block leaked into the facade's application data"
    );
    session.close().unwrap();
    drop(stream);
    server.join().unwrap();
}
