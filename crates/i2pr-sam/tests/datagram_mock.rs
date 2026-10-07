use i2pr_sam::{ClientConfig, SamClient};
use i2pr_sam_proto::{ReceivedDatagram, SessionStyle};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, UdpSocket},
};

#[tokio::test]
async fn datagram_udp_send_and_forwarded_receive() {
    let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tcp_addr = tcp_listener.local_addr().unwrap();
    let udp_listener = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let udp_addr = udp_listener.local_addr().unwrap();
    let bridge = tokio::spawn(async move {
        let (stream, _) = tcp_listener.accept().await.unwrap();
        let mut utility = BufReader::new(stream);
        let mut line = String::new();
        utility.read_line(&mut line).await.unwrap();
        utility
            .get_mut()
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .await
            .unwrap();
        let (stream, _) = tcp_listener.accept().await.unwrap();
        let mut control = BufReader::new(stream);
        line.clear();
        control.read_line(&mut line).await.unwrap();
        control
            .get_mut()
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .await
            .unwrap();
        line.clear();
        control.read_line(&mut line).await.unwrap();
        assert!(line.starts_with("SESSION CREATE STYLE=DATAGRAM "));
        let port: u16 = line
            .split_whitespace()
            .find_map(|part| part.strip_prefix("PORT=")?.parse().ok())
            .unwrap();
        control
            .get_mut()
            .write_all(b"SESSION STATUS RESULT=OK\n")
            .await
            .unwrap();
        let mut packet = [0; 4096];
        let (size, peer) = udp_listener.recv_from(&mut packet).await.unwrap();
        assert!(
            packet[..size].starts_with(b"3.0 dgram destination FROM_PORT=2 TO_PORT=3\npayload")
        );
        udp_listener
            .send_to(
                b"source-destination FROM_PORT=5 TO_PORT=6\nresponse",
                ("127.0.0.1", port),
            )
            .await
            .unwrap();
        let _ = peer;
    });
    let mut config = ClientConfig::new(tcp_addr);
    config.datagram_endpoint = udp_addr;
    let client = SamClient::connect(config).await.unwrap();
    let session = client
        .create_datagram_session("TRANSIENT", "dgram", SessionStyle::Datagram, &[])
        .await
        .unwrap();
    session
        .send("destination", b"payload", Some(2), Some(3))
        .await
        .unwrap();
    match session.recv().await.unwrap() {
        ReceivedDatagram::Authenticated(message) => {
            assert_eq!(message.source.as_str(), "source-destination");
            assert_eq!(message.from_port.get(), 5);
            assert_eq!(message.to_port.get(), 6);
            assert_eq!(message.payload, b"response");
        }
        other => panic!("unexpected datagram trust shape: {other:?}"),
    }
    session.close().await;
    bridge.await.unwrap();
}
