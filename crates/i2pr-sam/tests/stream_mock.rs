use i2pr_sam::{ClientConfig, SamClient};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

async fn read_line(stream: &mut BufReader<TcpStream>) -> String {
    let mut line = String::new();
    stream.read_line(&mut line).await.unwrap();
    line
}

async fn hello(stream: &mut BufReader<TcpStream>) {
    assert!(
        read_line(stream)
            .await
            .starts_with("HELLO VERSION MIN=3.1 MAX=3.3")
    );
    stream
        .get_mut()
        .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
        .await
        .unwrap();
}

#[tokio::test]
async fn ordinary_stream_exchange_and_destination_lookup() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let bridge = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut control = BufReader::new(socket);
        hello(&mut control).await;
        assert_eq!(
            read_line(&mut control).await,
            "DEST GENERATE SIGNATURE_TYPE=7\n"
        );
        control
            .get_mut()
            .write_all(b"DEST REPLY PUB=public PRIV=private\n")
            .await
            .unwrap();
        assert_eq!(
            read_line(&mut control).await,
            "NAMING LOOKUP NAME=example.i2p\n"
        );
        control
            .get_mut()
            .write_all(b"NAMING REPLY RESULT=OK NAME=example.i2p VALUE=resolved\n")
            .await
            .unwrap();

        let (socket, _) = listener.accept().await.unwrap();
        let mut session = BufReader::new(socket);
        hello(&mut session).await;
        assert!(
            read_line(&mut session)
                .await
                .starts_with("SESSION CREATE STYLE=STREAM ID=test DESTINATION=TRANSIENT")
        );
        session
            .get_mut()
            .write_all(b"SESSION STATUS RESULT=OK\n")
            .await
            .unwrap();

        let (socket, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(socket);
        hello(&mut stream).await;
        assert_eq!(
            read_line(&mut stream).await,
            "STREAM CONNECT ID=test DESTINATION=peer.i2p\n"
        );
        stream
            .get_mut()
            .write_all(b"STREAM STATUS RESULT=OK\nserver-data")
            .await
            .unwrap();
        let mut received = [0; 11];
        stream.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"client-data");
        stream.get_mut().write_all(b"reply").await.unwrap();
    });

    let client = SamClient::connect(ClientConfig::new(addr)).await.unwrap();
    let (_, secret) = client.generate_destination(Some(7)).await.unwrap();
    assert_eq!(format!("{secret:?}"), "SecretDestination([REDACTED])");
    assert_eq!(client.lookup("example.i2p").await.unwrap(), "resolved");
    let session = client
        .create_stream_session("TRANSIENT", "test", &[])
        .await
        .unwrap();
    let mut stream = session.connect("peer.i2p", None, None).await.unwrap();
    let mut received = [0; 11];
    stream.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, b"server-data");
    stream.write_all(b"client-data").await.unwrap();
    stream.flush().await.unwrap();
    let mut response = [0; 5];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(&response, b"reply");
    session.close().await;
    bridge.await.unwrap();
}

#[tokio::test]
async fn accept_preserves_authenticated_peer_and_data_after_status() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let bridge = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut utility = BufReader::new(socket);
        hello(&mut utility).await;

        let (socket, _) = listener.accept().await.unwrap();
        let mut session = BufReader::new(socket);
        hello(&mut session).await;
        let _ = read_line(&mut session).await;
        session
            .get_mut()
            .write_all(b"SESSION STATUS RESULT=OK\n")
            .await
            .unwrap();

        let (socket, _) = listener.accept().await.unwrap();
        let mut incoming = BufReader::new(socket);
        hello(&mut incoming).await;
        assert_eq!(
            read_line(&mut incoming).await,
            "STREAM ACCEPT ID=listener\n"
        );
        incoming
            .get_mut()
            .write_all(b"STREAM STATUS RESULT=OK DESTINATION=authenticated-peer\nhello")
            .await
            .unwrap();
    });
    let client = SamClient::connect(ClientConfig::new(addr)).await.unwrap();
    let session = client
        .create_stream_session("TRANSIENT", "listener", &[])
        .await
        .unwrap();
    let mut stream = session.accept().await.unwrap();
    assert_eq!(stream.remote_destination(), Some("authenticated-peer"));
    let mut bytes = [0; 5];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"hello");
    session.close().await;
    bridge.await.unwrap();
}
