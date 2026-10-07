use i2pr_sam::{ClientConfig, SamClient};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

async fn line(reader: &mut BufReader<TcpStream>) -> String {
    let mut value = String::new();
    reader.read_line(&mut value).await.unwrap();
    value
}

#[tokio::test]
async fn one_hundred_stream_session_create_close_cycles_release_control_sockets() {
    const CYCLES: usize = 100;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let bridge = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut utility = BufReader::new(socket);
        let _ = line(&mut utility).await;
        utility
            .get_mut()
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .await
            .unwrap();
        for index in 0..CYCLES {
            let (socket, _) = listener.accept().await.unwrap();
            let mut session = BufReader::new(socket);
            let _ = line(&mut session).await;
            session
                .get_mut()
                .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
                .await
                .unwrap();
            assert!(
                line(&mut session)
                    .await
                    .starts_with("SESSION CREATE STYLE=STREAM")
            );
            session
                .get_mut()
                .write_all(b"SESSION STATUS RESULT=OK\n")
                .await
                .unwrap();
            let mut eof = [0; 1];
            assert_eq!(
                session.read(&mut eof).await.unwrap(),
                0,
                "session {index} control socket remained open"
            );
        }
    });
    let client = SamClient::connect(ClientConfig::new(addr)).await.unwrap();
    for index in 0..CYCLES {
        let session = client
            .create_stream_session("TRANSIENT", &format!("soak-{index}"), &[])
            .await
            .unwrap();
        session.close().await;
    }
    bridge.await.unwrap();
}
