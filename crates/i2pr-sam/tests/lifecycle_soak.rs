//! Lifecycle soak regression test.
//!
//! Defect closed here (**O**): creating and destroying sessions and streams must release every
//! socket and task it opened, and `close()` must be idempotent so a double close in an error
//! path cannot hang or panic.
//!
//! Leak detection deliberately avoids `tokio::runtime::Handle::metrics()`, which needs an
//! unstable cfg. Instead the mock bridge counts the connections it has accepted and not yet
//! seen close: if a session leaked its control socket or a stream leaked its data socket, the
//! live count cannot return to its baseline.

mod support;

use i2pr_sam::{DatagramTransport, SamError, SessionDestination, SessionStyle};
use support::{Match, OWNER_DESTINATION, Rule, SESSION_OK, Script, naming_me_ok};
use tokio::io::AsyncWriteExt;

fn utility_script() -> Script {
    vec![Rule::line(
        Match::starts_with("HELLO VERSION"),
        support::HELLO_REPLY,
    )]
}

fn owner_script(adds: usize) -> Script {
    let mut script = vec![
        Rule::line(Match::starts_with("HELLO VERSION"), support::HELLO_REPLY),
        Rule::line(
            Match::starts_with("SESSION CREATE STYLE=MASTER"),
            SESSION_OK,
        ),
        Rule::line(
            Match::exact("NAMING LOOKUP NAME=ME"),
            &naming_me_ok(OWNER_DESTINATION),
        ),
    ];
    script.extend((0..adds).map(|_| Rule::line(Match::starts_with("SESSION ADD"), SESSION_OK)));
    script
}

/// O: many session/stream cycles leave no socket behind and `close()` is idempotent.
#[tokio::test]
async fn o_session_and_stream_cycles_release_every_socket_and_close_is_idempotent() {
    const CYCLES: usize = 40;
    // Every session opens its own control socket and every accept its own data socket, so
    // each cycle contributes two scripted connections.
    let mut scripts = vec![utility_script()];
    for _ in 0..CYCLES {
        scripts.push(vec![
            Rule::line(Match::starts_with("HELLO VERSION"), support::HELLO_REPLY),
            Rule::line(
                Match::starts_with("SESSION CREATE STYLE=STREAM"),
                SESSION_OK,
            ),
            Rule::line(
                Match::exact("NAMING LOOKUP NAME=ME"),
                &naming_me_ok(OWNER_DESTINATION),
            ),
        ]);
        scripts.push(vec![
            Rule::line(Match::starts_with("HELLO VERSION"), support::HELLO_REPLY),
            Rule::line(
                Match::starts_with("STREAM ACCEPT"),
                &support::stream_accept_with_peer(),
            ),
        ]);
    }
    let bridge = support::MockBridge::start_with(scripts).await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();
    assert_eq!(
        bridge.live_connections(),
        1,
        "the utility socket is the baseline"
    );

    for index in 0..CYCLES {
        let session = client
            .create_stream_session(
                &SessionDestination::Transient,
                &format!("soak-{index}"),
                &[],
            )
            .await
            .unwrap();
        let mut stream = session.accept().await.unwrap();
        stream.write_all(b"ping").await.unwrap();
        assert!(
            stream.peer().is_some(),
            "a non-silent accept announces the peer"
        );
        drop(stream);
        session.close().await;
        // A second close must be a no-op rather than a hang or a panic.
        session.close().await;
    }

    assert_eq!(
        bridge.accepted_connections(),
        1 + CYCLES * 2,
        "each cycle must open exactly one session socket and one stream socket"
    );
    bridge
        .wait_for("every session and stream socket to close", || {
            bridge.live_connections() == 1
        })
        .await;
    let mut pinged = 0;
    for index in 0..CYCLES {
        pinged += bridge.connection(2 + index * 2).occurrences(b"ping");
    }
    assert_eq!(
        pinged, CYCLES,
        "every accepted stream must have carried its payload"
    );
    bridge.assert_scripts_clean();
}

/// O: a datagram session and a shared session both close idempotently and stop accepting work.
#[tokio::test]
async fn o_datagram_and_shared_sessions_close_idempotently_and_release_their_children() {
    let bridge = support::MockBridge::start_with_fallback(
        vec![
            utility_script(),
            vec![
                Rule::line(Match::starts_with("HELLO VERSION"), support::HELLO_REPLY),
                Rule::line(
                    Match::starts_with("SESSION CREATE STYLE=DATAGRAM"),
                    SESSION_OK,
                ),
                Rule::line(
                    Match::exact("NAMING LOOKUP NAME=ME"),
                    &naming_me_ok(OWNER_DESTINATION),
                ),
            ],
            owner_script(1),
        ],
        support::default_script(),
    )
    .await;
    let client = i2pr_sam::SamClient::connect(bridge.client_config())
        .await
        .unwrap();

    let datagram = client
        .create_datagram_session_with(
            &SessionDestination::Transient,
            "soak-dgram",
            SessionStyle::Datagram,
            DatagramTransport::ControlSocketV1,
            &[],
        )
        .await
        .unwrap();
    assert_eq!(datagram.id(), "soak-dgram");
    datagram.close().await;
    datagram.close().await;
    assert!(
        matches!(
            datagram
                .send("peer.b32.i2p", b"after-close", None, None)
                .await,
            Err(SamError::Closed)
        ),
        "a closed datagram session must refuse further work"
    );

    let owner = client
        .create_shared_session(
            &SessionDestination::Transient,
            "soak-owner",
            i2pr_sam_proto::SharedDialect::Master,
            &[],
        )
        .await
        .unwrap();
    let child = owner
        .add_child(
            "soak-child",
            SessionStyle::Datagram,
            &[("LISTEN_PORT".to_owned(), "31".to_owned())],
        )
        .await
        .unwrap();
    assert!(child.is_open());
    child.close().await;
    child.close().await;

    owner.close().await;
    owner.close().await;
    assert!(!child.is_open(), "closing the owner must close every child");
    assert!(
        matches!(
            owner.add_child("late", SessionStyle::Stream, &[]).await,
            Err(SamError::Closed)
        ),
        "a closed shared session must refuse new subsessions"
    );
    assert!(matches!(
        owner.remove_child("soak-child").await,
        Err(SamError::Closed)
    ));

    bridge
        .wait_for("every session socket to close", || {
            bridge.live_connections() == 1
        })
        .await;
    bridge.assert_scripts_clean();
}
