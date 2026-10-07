use i2pr_sam_blocking::BlockingClient;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

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

#[test]
fn blocking_stream_delegates_to_async_client() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut utility, _) = listener.accept().unwrap();
        assert!(read_line(&mut utility).starts_with("HELLO VERSION MIN=3.1"));
        utility
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .unwrap();
        let (mut session, _) = listener.accept().unwrap();
        let _ = read_line(&mut session);
        session
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .unwrap();
        assert!(read_line(&mut session).starts_with("SESSION CREATE STYLE=STREAM"));
        session.write_all(b"SESSION STATUS RESULT=OK\n").unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_line(&mut stream);
        stream
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .unwrap();
        assert!(read_line(&mut stream).starts_with("STREAM CONNECT ID=blocking"));
        stream.write_all(b"STREAM STATUS RESULT=OK\n").unwrap();
        let mut data = [0; 4];
        stream.read_exact(&mut data).unwrap();
        assert_eq!(&data, b"ping");
        stream.write_all(b"pong").unwrap();
    });

    let client = BlockingClient::connect_endpoint(addr).unwrap();
    let session = client
        .create_stream_session("TRANSIENT", "blocking", &[])
        .unwrap();
    let mut stream = session.connect("peer.i2p", None, None).unwrap();
    stream.write_all(b"ping").unwrap();
    let mut response = [0; 4];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"pong");
    session.close().unwrap();
    server.join().unwrap();
}
