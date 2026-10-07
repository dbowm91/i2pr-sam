use i2pr_sam::{
    ClientConfig, FailureClass, ReconnectPolicy, SamClient, SamError, classify_failure,
};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn bind_listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    (listener, addr)
}

#[tokio::test]
async fn malformed_and_oversized_hello_responses_fail_closed() {
    let (listener, addr) = bind_listener().await;
    let malformed = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 128];
        let _ = socket.read(&mut request).await.unwrap();
        socket.write_all(b"NOT A SAM REPLY\n").await.unwrap();
    });
    let error = match SamClient::connect(ClientConfig::new(addr)).await {
        Ok(_) => panic!("malformed bridge reply unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(matches!(error, SamError::Rejected(_)));
    malformed.await.unwrap();

    let (listener, addr) = bind_listener().await;
    let oversized = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 128];
        let _ = socket.read(&mut request).await.unwrap();
        socket.write_all(&vec![b'x'; 2048]).await.unwrap();
    });
    let mut config = ClientConfig::new(addr);
    config.max_frame_bytes = 1024;
    assert!(matches!(
        SamClient::connect(config).await,
        Err(SamError::Protocol(_))
    ));
    oversized.await.unwrap();
}

#[tokio::test]
async fn timeout_and_retry_classifier_are_bounded_and_explicit() {
    let (listener, addr) = bind_listener().await;
    let delayed = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 128];
        let _ = socket.read(&mut request).await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        let _ = socket
            .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
            .await;
    });
    let mut config = ClientConfig::new(addr);
    config.control_timeout = Duration::from_millis(10);
    let policy = ReconnectPolicy::bounded(
        3,
        Duration::from_millis(1),
        Duration::from_millis(2),
        Duration::from_millis(30),
    )
    .unwrap();
    assert!(matches!(
        SamClient::connect_with_policy(config, policy).await,
        Err(SamError::Timeout)
    ));
    assert_eq!(
        classify_failure(&SamError::Timeout),
        FailureClass::TransportTransient
    );
    assert_eq!(
        classify_failure(&SamError::Protocol(
            i2pr_sam_proto::ParseError::InvalidToken
        )),
        FailureClass::ProtocolPermanent
    );
    assert_eq!(
        classify_failure(&SamError::Rejected("AUTH".into())),
        FailureClass::ConfigurationPermanent
    );
    assert_eq!(
        classify_failure(&SamError::Closed),
        FailureClass::CancelledOrClosed
    );
    delayed.abort();
}
