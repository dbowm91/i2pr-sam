use i2pr_sam::{ClientConfig, SamClient};
use i2pr_sam_proto::{SessionStyle, SharedDialect};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

async fn line(reader: &mut BufReader<TcpStream>) -> String {
    let mut value = String::new();
    reader.read_line(&mut value).await.unwrap();
    value
}

#[tokio::test]
async fn shared_owner_child_lifecycle_is_transactional_and_linked() {
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

        let (socket, _) = listener.accept().await.unwrap();
        let mut owner = BufReader::new(socket);
        let _ = line(&mut owner).await;
        owner
            .get_mut()
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .await
            .unwrap();
        let create = line(&mut owner).await;
        assert!(create.starts_with("SESSION CREATE STYLE=MASTER ID=owner DESTINATION=TRANSIENT"));
        owner
            .get_mut()
            .write_all(b"SESSION STATUS RESULT=OK\n")
            .await
            .unwrap();
        let add = line(&mut owner).await;
        assert!(
            add.starts_with("SESSION ADD STYLE=DATAGRAM ID=udp LISTEN_PORT=9 HOST=127.0.0.1 PORT=")
        );
        owner
            .get_mut()
            .write_all(b"SESSION STATUS RESULT=OK\n")
            .await
            .unwrap();
        let remove = line(&mut owner).await;
        assert_eq!(remove, "SESSION REMOVE ID=udp\n");
        owner
            .get_mut()
            .write_all(b"SESSION STATUS RESULT=OK\n")
            .await
            .unwrap();
    });

    let client = SamClient::connect(ClientConfig::new(addr)).await.unwrap();
    let owner = client
        .create_shared_session("TRANSIENT", "owner", SharedDialect::Master, &[])
        .await
        .unwrap();
    assert_eq!(owner.destination(), "TRANSIENT");
    let child = owner
        .add_child(
            "udp",
            SessionStyle::Datagram,
            &[("LISTEN_PORT".into(), "9".into())],
        )
        .await
        .unwrap();
    assert_eq!(child.destination(), owner.destination());
    assert!(
        owner
            .add_child(
                "other",
                SessionStyle::Datagram,
                &[("LISTEN_PORT".into(), "9".into())]
            )
            .await
            .is_err()
    );
    assert!(child.is_open());
    owner.remove_child("udp").await.unwrap();
    assert!(!child.is_open());
    owner.close().await;
    assert!(!child.is_open());
    bridge.await.unwrap();
}
