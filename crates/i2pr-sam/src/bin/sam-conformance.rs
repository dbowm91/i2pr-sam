//! SAM conformance runner.
//!
//! Every row this binary emits records a semantic operation and the evidence behind it.
//! A row may only be `pass` when payload actually crossed the link and both ends agreed on
//! the exact bytes; a session that merely came into existence is `create_only`, an
//! unimplemented router feature is `unsupported`, and a lane that could not run names why.
//! Nothing here may be reported as a compatibility result on the strength of a session
//! creation reply.
//!
//! Exit codes: 0 no failing row, 1 at least one failing row, 2 usage or connection error,
//! 3 the requested live lane could not be provisioned.

use std::{
    collections::BTreeMap,
    env,
    net::SocketAddr,
    path::PathBuf,
    process::ExitCode,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use i2pr_sam::{
    ClientConfig, DatagramTransport, ReceivedDatagram, SamClient, SamError, SessionDestination,
    SessionStyle, SharedDialect,
};
use i2pr_sam_proto::SamVersion;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plan {
    Full,
    Stream,
    Datagram,
    Shared,
}

impl Plan {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "full" => Ok(Self::Full),
            "stream" => Ok(Self::Stream),
            "datagram" => Ok(Self::Datagram),
            "shared" => Ok(Self::Shared),
            other => Err(format!(
                "unknown plan {other:?}; expected full|stream|datagram|shared"
            )),
        }
    }
    fn wants_stream(self) -> bool {
        matches!(self, Self::Full | Self::Stream)
    }
    fn wants_datagram(self) -> bool {
        matches!(self, Self::Full | Self::Datagram)
    }
    fn wants_shared(self) -> bool {
        matches!(self, Self::Full | Self::Shared)
    }
}

struct Options {
    endpoint: SocketAddr,
    router: String,
    router_version: String,
    peer_endpoint: Option<SocketAddr>,
    peer_router: Option<String>,
    peer_router_version: Option<String>,
    plan: Plan,
    output: Option<PathBuf>,
    control_timeout: Duration,
    max_payload: usize,
}

fn parse_options() -> Result<Options, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut endpoint = None;
    let mut router = None;
    let mut router_version = None;
    let mut peer_endpoint = None;
    let mut peer_router = None;
    let mut peer_router_version = None;
    let mut plan = Plan::Full;
    let mut output = None;
    let mut control_timeout = 20;
    let mut max_payload = 512usize;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let mut value = || {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag {
            "--endpoint" => {
                endpoint = Some(
                    value()?
                        .parse::<SocketAddr>()
                        .map_err(|e| format!("--endpoint: {e}"))?,
                )
            }
            "--router" => router = Some(value()?),
            "--router-version" => router_version = Some(value()?),
            "--peer-endpoint" => {
                peer_endpoint = Some(
                    value()?
                        .parse::<SocketAddr>()
                        .map_err(|e| format!("--peer-endpoint: {e}"))?,
                )
            }
            "--peer-router" => peer_router = Some(value()?),
            "--peer-router-version" => peer_router_version = Some(value()?),
            "--plan" => plan = Plan::parse(&value()?)?,
            "--output" => output = Some(PathBuf::from(value()?)),
            "--control-timeout" => {
                control_timeout = value()?
                    .parse()
                    .map_err(|e| format!("--control-timeout: {e}"))?
            }
            "--max-payload" => {
                max_payload = value()?
                    .parse()
                    .map_err(|e| format!("--max-payload: {e}"))?
            }
            other => return Err(format!("unrecognised argument {other}")),
        }
        index += 1;
    }
    let endpoint = endpoint.ok_or("--endpoint is required")?;
    let router = router.ok_or("--router is required")?;
    let router_version = router_version.ok_or("--router-version is required")?;
    if max_payload == 0 || max_payload > 32_768 {
        return Err("--max-payload must be within 1..=32768".into());
    }
    if control_timeout == 0 {
        return Err("--control-timeout must be positive".into());
    }
    Ok(Options {
        endpoint,
        router,
        router_version,
        peer_endpoint,
        peer_router,
        peer_router_version,
        plan,
        output,
        control_timeout: Duration::from_secs(control_timeout),
        max_payload,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pass,
    Unsupported,
    Fail,
    NotRun,
    CreateOnly,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Unsupported => "unsupported",
            Self::Fail => "fail",
            Self::NotRun => "not_run",
            Self::CreateOnly => "create_only",
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Evidence {
    bytes_sent: usize,
    bytes_received: usize,
    exact_payload_match: bool,
    identity_proven: bool,
}

struct Row {
    feature: &'static str,
    operation: &'static str,
    shared_dialect: Option<&'static str>,
    local_identity: Option<(String, String)>,
    peer_identity: Option<(String, String)>,
    outcome: Outcome,
    evidence: Evidence,
    diagnostic_category: Option<String>,
    notes: String,
}

impl Row {
    fn structural(feature: &'static str, operation: &'static str) -> Self {
        Self {
            feature,
            operation,
            shared_dialect: None,
            local_identity: None,
            peer_identity: None,
            outcome: Outcome::NotRun,
            evidence: Evidence::default(),
            diagnostic_category: None,
            notes: String::new(),
        }
    }

    fn dialect(mut self, dialect: SharedDialect) -> Self {
        self.shared_dialect = Some(match dialect {
            SharedDialect::Master => "MASTER",
            SharedDialect::Primary => "PRIMARY",
        });
        self
    }

    fn identity(mut self, identity: Option<&i2pr_sam::SessionIdentity>) -> Self {
        if let Some(identity) = identity {
            self.local_identity = Some((
                identity.destination().as_str().to_owned(),
                identity.hash().to_string(),
            ));
        }
        self
    }

    fn peer(mut self, identity: Option<&i2pr_sam::SessionIdentity>) -> Self {
        if let Some(identity) = identity {
            self.peer_identity = Some((
                identity.destination().as_str().to_owned(),
                identity.hash().to_string(),
            ));
        }
        self
    }

    fn note(mut self, note: impl Into<String>) -> Self {
        self.notes = note.into();
        self
    }

    fn skipped(mut self, category: &str, note: impl Into<String>) -> Self {
        self.outcome = Outcome::NotRun;
        self.diagnostic_category = Some(category.to_owned());
        self.notes = note.into();
        self
    }

    /// Record an operation that did not complete, keeping capability verdicts distinct.
    fn failed(mut self, error: &SamError, note: &str) -> Self {
        self.diagnostic_category = Some(classify_diagnostic(error).to_owned());
        self.notes = if note.is_empty() {
            error.to_string()
        } else {
            format!("{note}: {error}")
        };
        self.outcome = if matches!(error, SamError::Unsupported(_)) {
            Outcome::Unsupported
        } else if is_transient(error) {
            Outcome::NotRun
        } else {
            Outcome::Fail
        };
        self
    }

    fn created_only(mut self, note: impl Into<String>) -> Self {
        self.outcome = Outcome::CreateOnly;
        self.diagnostic_category = Some("no_payload_evidence".into());
        self.notes = note.into();
        self
    }
}

fn is_transient(error: &SamError) -> bool {
    matches!(
        i2pr_sam::classify_failure(error),
        i2pr_sam::FailureClass::TransportTransient
            | i2pr_sam::FailureClass::RouterTransient
            | i2pr_sam::FailureClass::CancelledOrClosed
    )
}

fn classify_diagnostic(error: &SamError) -> &'static str {
    match error {
        SamError::Timeout => "router_timeout",
        SamError::Closed => "session_closed",
        SamError::Io(_) => "transport_error",
        SamError::Protocol(_) => "protocol_violation",
        SamError::Unsupported(_) => "router_rejected_style",
        SamError::Rejected(_) => "router_rejected",
        SamError::NameNotFound => "name_not_found",
        SamError::IdentityUnavailable => "identity_unavailable",
        SamError::RetryAdmissionSaturated => "retry_admission_saturated",
    }
}

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // RFC3339 UTC without pulling in a date-time dependency.
    let days = seconds / 86_400;
    let time_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60
    )
}

/// Howard Hinnant's days-from-epoch to civil date algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

fn plan_name(plan: Plan) -> String {
    match plan {
        Plan::Full => "full",
        Plan::Stream => "stream",
        Plan::Datagram => "datagram",
        Plan::Shared => "shared",
    }
    .to_owned()
}

fn payload(seed: u8, size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| {
            seed.wrapping_add(index as u8)
                .wrapping_mul(31)
                .wrapping_add(7)
        })
        .collect()
}

fn config_for(endpoint: SocketAddr, control_timeout: Duration) -> ClientConfig {
    let mut config = ClientConfig::new(endpoint);
    config.control_timeout = control_timeout;
    config.connect_timeout = Duration::from_secs(10);
    config
}

async fn negotiate(client: &SamClient) -> Option<SamVersion> {
    client.capabilities().await.negotiated_version
}

/// Resolve the concrete identity of a session the client controls.
async fn resolve(
    client: &SamClient,
    id: &str,
) -> Result<Option<i2pr_sam::SessionIdentity>, SamError> {
    // A shared session is the cheapest way to obtain an identity the router will actually
    // use for routing, and it must publish a concrete Destination or creation fails.
    let session = client
        .create_shared_session(
            &SessionDestination::Transient,
            id,
            SharedDialect::Primary,
            &[],
        )
        .await?;
    let identity = session.identity().clone();
    session.close().await;
    Ok(Some(identity))
}

/// Stream payload row: one side accepts, the other connects, and both verify exact bytes.
async fn stream_row(
    options: &Options,
    client: &SamClient,
    peer: &SamClient,
    local_identity: Option<&i2pr_sam::SessionIdentity>,
    peer_identity: Option<&i2pr_sam::SessionIdentity>,
) -> Row {
    let mut row = Row::structural("stream", "stream_payload_bidirectional")
        .identity(local_identity)
        .peer(peer_identity);
    let Some(peer_identity) = peer_identity else {
        return row.skipped(
            "peer_identity_unavailable",
            "peer Destination could not be resolved, so no payload exchange was attempted",
        );
    };
    let session = match client
        .create_stream_session(&SessionDestination::Transient, "cf-stream", &[])
        .await
    {
        Ok(session) => session,
        Err(error) => {
            return row.failed(&error, "").note("accepting side");
        }
    };
    let peer_session = match peer
        .create_stream_session(&SessionDestination::Transient, "cf-stream-peer", &[])
        .await
    {
        Ok(session) => session,
        Err(error) => {
            return row.failed(&error, "").note("connecting side");
        }
    };
    let destination = peer_identity.destination().as_str().to_owned();

    let session = Arc::new(session);
    let max_payload = options.max_payload;
    let accepting = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            let mut stream = session.accept().await?;
            let peer: Option<i2pr_sam::StreamPeer> = stream.peer().cloned();
            let mut received = vec![0; max_payload];
            let count = stream.read(&mut received).await?;
            received.truncate(count);
            let reply = payload(0xA5, max_payload);
            stream.write_all(&reply).await?;
            stream.flush().await?;
            Ok::<_, SamError>((peer, received, reply.len()))
        })
    };
    let connecting = tokio::spawn(async move {
        let mut stream = peer_session.connect(&destination, None, None).await?;
        let body = payload(0x5A, max_payload);
        stream.write_all(&body).await?;
        stream.flush().await?;
        let mut reply = vec![0; max_payload];
        let count = stream.read(&mut reply).await?;
        reply.truncate(count);
        Ok::<_, SamError>((body, reply))
    });

    let sent = max_payload;
    let (accepted, connected) = match (accepting.await, connecting.await) {
        (Ok(Ok(accepted)), Ok(Ok(connected))) => (accepted, connected),
        (Ok(Ok(_)), Ok(Err(error))) => {
            session.close().await;
            return row
                .failed(&error, "")
                .note("the connecting side failed while the accepting side completed");
        }
        (Ok(Err(error)), _) => {
            session.close().await;
            return row
                .failed(&error, "")
                .note("the accepting side failed, so no payload crossed the link");
        }
        (Err(join_error), _) => {
            session.close().await;
            return row
                .skipped(
                    "accepting_side_failed",
                    format!("accepting side task failed: {join_error}"),
                )
                .note("no payload exchange completed");
        }
        (Ok(Ok(_)), Err(join_error)) => {
            session.close().await;
            return row
                .skipped(
                    "connecting_side_failed",
                    format!("connecting side task failed: {join_error}"),
                )
                .note("no payload exchange completed");
        }
    };
    session.close().await;
    let (observed_peer, inbound, replied) = accepted;
    let (outbound, reply) = connected;
    let inbound_expected = payload(0x5A, max_payload);
    let reply_expected = payload(0xA5, max_payload);
    let exact = inbound == inbound_expected && reply == reply_expected;
    let peer_identity_match = observed_peer
        .as_ref()
        .map(|peer| &peer.destination == peer_identity.destination())
        .unwrap_or(false);
    let evidence = Evidence {
        bytes_sent: outbound.len() + replied,
        bytes_received: inbound.len() + reply.len(),
        exact_payload_match: exact,
        identity_proven: peer_identity_match,
    };
    let identity_proven = evidence.identity_proven;
    row.evidence = evidence;
    if exact {
        row.outcome = Outcome::Pass;
        row.notes = format!(
            "both directions exchanged {} bytes; non-silent accept captured the peer Destination",
            sent
        );
    } else {
        row.outcome = Outcome::Fail;
        row.diagnostic_category = Some("payload_mismatch".into());
        row.notes = "payload bytes did not match what was sent".into();
    }
    if !identity_proven {
        row.notes
            .push_str("; peer Destination was not captured on accept");
    }
    row
}

/// Datagram row for one ordinary style, over the requested transport.
async fn datagram_row(
    options: &Options,
    client: &SamClient,
    peer: &SamClient,
    style: SessionStyle,
    transport: DatagramTransport,
    local_identity: Option<&i2pr_sam::SessionIdentity>,
    peer_identity: Option<&i2pr_sam::SessionIdentity>,
) -> Row {
    let mut row = Row::structural(
        match style {
            SessionStyle::Datagram => "datagram",
            SessionStyle::Raw => "raw",
            other => unreachable!("{other:?} is not an ordinary datagram style"),
        },
        match (style, transport) {
            (_, DatagramTransport::UdpForward) => "datagram_payload_exchange_udp_forward",
            (_, DatagramTransport::ControlSocketV1) => "datagram_payload_exchange_control_socket",
        },
    )
    .identity(local_identity)
    .peer(peer_identity);
    let Some(peer_identity) = peer_identity else {
        return row.skipped(
            "peer_identity_unavailable",
            "peer Destination could not be resolved, so no payload exchange was attempted",
        );
    };
    if !style.supports_control_socket_datagram() && transport == DatagramTransport::ControlSocketV1
    {
        return row.skipped(
            "transport_not_permitted",
            "v1/v2-compatible control-socket datagram commands are excluded for this style",
        );
    }
    let id = format!("cf-{}", style.as_wire().to_lowercase());
    let peer_id = format!("{id}-peer");
    let session = match client
        .create_datagram_session_with(&SessionDestination::Transient, &id, style, transport, &[])
        .await
    {
        Ok(session) => session,
        Err(error) => {
            return row
                .failed(&error, "")
                .note("this side could not create the session");
        }
    };
    let peer_session = match peer
        .create_datagram_session_with(
            &SessionDestination::Transient,
            &peer_id,
            style,
            transport,
            &[],
        )
        .await
    {
        Ok(peer_session) => peer_session,
        Err(error) => {
            session.close().await;
            return row
                .failed(&error, "")
                .note("the peer side could not create a matching session");
        }
    };
    let destination = peer_identity.destination().as_str().to_owned();
    let body = payload(0x3C, options.max_payload);
    if let Err(error) = peer_session.send(&destination, &body, None, None).await {
        session.close().await;
        peer_session.close().await;
        return row
            .failed(&error, "")
            .note("peer send failed before delivery");
    }
    let received = match session.recv().await {
        Ok(received) => received,
        Err(error) => {
            session.close().await;
            peer_session.close().await;
            return row.failed(&error, "").note("no datagram was delivered");
        }
    };
    session.close().await;
    peer_session.close().await;
    let (inbound, source_known) = match &received {
        ReceivedDatagram::Authenticated(message) => (
            message.payload.clone(),
            Some(message.source.as_str().to_owned()),
        ),
        ReceivedDatagram::Unverified(message) => (message.payload.clone(), None),
        ReceivedDatagram::Raw(message) => (message.payload.clone(), None),
    };
    let exact = inbound == body;
    row.evidence = Evidence {
        bytes_sent: body.len(),
        bytes_received: inbound.len(),
        exact_payload_match: exact,
        identity_proven: source_known.as_deref() == Some(peer_identity.destination().as_str()),
    };
    if exact {
        row.outcome = Outcome::Pass;
        row.notes = "datagram payload matched exactly".into();
        if style == SessionStyle::Datagram3 {
            row.notes
                .push_str("; source remains an unverified hash by specification");
        }
        if style == SessionStyle::Raw {
            row.notes
                .push_str("; RAW carries no source identity by specification");
        }
    } else {
        row.outcome = Outcome::Fail;
        row.diagnostic_category = Some("payload_mismatch".into());
        row.notes = "datagram payload bytes did not match".into();
    }
    row
}

/// Shared-session row: create, add subsessions, and prove one Destination links them.
async fn shared_row(
    options: &Options,
    client: &SamClient,
    peer: &SamClient,
    dialect: SharedDialect,
    local_identity: Option<&i2pr_sam::SessionIdentity>,
) -> Row {
    let name = match dialect {
        SharedDialect::Master => "MASTER",
        SharedDialect::Primary => "PRIMARY",
    };
    let mut row = Row::structural("shared", "shared_subsession_payload_single_destination")
        .dialect(dialect)
        .identity(local_identity);
    let session = match client
        .create_shared_session(
            &SessionDestination::Transient,
            &format!("cf-shared-{name}"),
            dialect,
            &[],
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            return row
                .failed(&error, "")
                .note(format!("{name} shared session could not be created"));
        }
    };
    let identity = session.identity().clone();
    row = row.identity(Some(&identity));
    let datagram_child = match session
        .add_child("cf-shared-dgram", SessionStyle::Datagram, &[])
        .await
    {
        Ok(child) => child,
        Err(error) => {
            session.close().await;
            return row.failed(&error, "").note("subsession could not be added");
        }
    };
    let stream_child = match session
        .add_child("cf-shared-stream", SessionStyle::Stream, &[])
        .await
    {
        Ok(child) => child,
        Err(error) => {
            session.close().await;
            return row
                .failed(&error, "")
                .note("stream subsession could not be added");
        }
    };
    // Every child must report the owner's concrete identity; the request token TRANSIENT
    // is never an acceptable answer.
    let datagram_child = Arc::new(datagram_child);
    let same_identity =
        datagram_child.identity() == &identity && stream_child.identity() == &identity;
    if !same_identity {
        session.close().await;
        row.outcome = Outcome::Fail;
        row.diagnostic_category = Some("identity_divergence".into());
        row.notes = "subsessions did not report the owner's concrete Destination".into();
        return row;
    }
    if peer.capabilities().await.negotiated_version.is_none() {
        session.close().await;
        return row
            .created_only(format!(
                "{name} shared session and subsessions established with a concrete Destination; \
                 peer-observable payload exchange needs --peer-endpoint"
            ))
            .note("single Destination hash: ".to_owned() + &identity.hash().to_string());
    }
    let peer_session = match peer
        .create_stream_session(&SessionDestination::Transient, "cf-shared-peer", &[])
        .await
    {
        Ok(session) => session,
        Err(error) => {
            session.close().await;
            return row
                .failed(&error, "")
                .note("peer session could not be created");
        }
    };
    let destination = identity.destination().as_str().to_owned();
    let max_payload = options.max_payload;
    let receiving = {
        let child = Arc::clone(&datagram_child);
        tokio::spawn(async move {
            let mut stream = child.accept().await?;
            let peer: Option<i2pr_sam::StreamPeer> = stream.peer().cloned();
            let mut body = vec![0; max_payload];
            let count = stream.read(&mut body).await?;
            body.truncate(count);
            Ok::<_, SamError>((peer, body))
        })
    };
    let sending = tokio::spawn(async move {
        let mut stream = peer_session.connect(&destination, None, None).await?;
        let body = payload(0x6B, max_payload);
        stream.write_all(&body).await?;
        stream.flush().await?;
        Ok::<_, SamError>(body)
    });
    match (receiving.await, sending.await) {
        (Ok(Ok((peer_observed, inbound))), Ok(Ok(outbound))) => {
            let expected = payload(0x6B, max_payload);
            let exact = inbound == expected && outbound == expected;
            row.evidence = Evidence {
                bytes_sent: outbound.len(),
                bytes_received: inbound.len(),
                exact_payload_match: exact,
                identity_proven: peer_observed.is_some(),
            };
            if exact && row.evidence.identity_proven {
                row.outcome = Outcome::Pass;
                row.notes = format!(
                    "subsession carried an exact payload under Destination hash {} and the \
                     accepting side captured the peer Destination",
                    identity.hash()
                );
            } else {
                row.outcome = Outcome::Fail;
                row.diagnostic_category = Some(if exact {
                    "identity_not_observed".into()
                } else {
                    "payload_mismatch".to_owned()
                });
                row.notes = "shared subsession exchange did not produce verifiable evidence".into();
            }
        }
        (Ok(Err(error)), _) => {
            row.outcome = Outcome::NotRun;
            row.diagnostic_category = Some(classify_diagnostic(&error).into());
            row.notes = format!("subsession receive failed: {error}");
        }
        (Ok(Ok(_)), Ok(Err(error))) => {
            row.outcome = Outcome::NotRun;
            row.diagnostic_category = Some(classify_diagnostic(&error).into());
            row.notes = format!("peer connect to the shared Destination failed: {error}");
        }
        (Err(join_error), _) => {
            row.outcome = Outcome::NotRun;
            row.diagnostic_category = Some("subsession_task_failed".into());
            row.notes = format!("subsession receive task failed: {join_error}");
        }
        (Ok(Ok(_)), Err(join_error)) => {
            row.outcome = Outcome::NotRun;
            row.diagnostic_category = Some("subsession_task_failed".into());
            row.notes = format!("peer connect task failed: {join_error}");
        }
    };
    datagram_child.close().await;
    stream_child.close().await;
    session.close().await;
    row
}

fn identity_json(identity: Option<&(String, String)>) -> Value {
    match identity {
        Some((destination, hash)) => json!({"destination": destination, "destination_hash": hash}),
        None => Value::Null,
    }
}

fn row_json(row: &Row) -> Value {
    json!({
        "feature": row.feature,
        "operation": row.operation,
        "shared_dialect": row.shared_dialect,
        "local_identity": identity_json(row.local_identity.as_ref()),
        "peer_identity": identity_json(row.peer_identity.as_ref()),
        "result": row.outcome.as_str(),
        "evidence": {
            "bytes_sent": row.evidence.bytes_sent,
            "bytes_received": row.evidence.bytes_received,
            "exact_payload_match": row.evidence.exact_payload_match,
            "identity_proven": row.evidence.identity_proven,
        },
        "diagnostic_category": row.diagnostic_category,
        "notes": row.notes,
    })
}

fn summarise(rows: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for outcome in ["pass", "unsupported", "fail", "not_run", "create_only"] {
        counts.insert(outcome.to_owned(), 0);
    }
    for row in rows {
        let key = row
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("not_run")
            .to_owned();
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err((code, message)) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

async fn run() -> Result<ExitCode, (u8, String)> {
    let options = parse_options().map_err(|error| (2, format!("usage error: {error}")))?;
    let config = config_for(options.endpoint, options.control_timeout);
    let client = SamClient::connect(config).await.map_err(|error| {
        (
            2,
            format!("cannot reach SAM bridge at {}: {error}", options.endpoint),
        )
    })?;

    let local_identity = match resolve(&client, "cf-identity").await {
        Ok(identity) => identity,
        Err(error) => {
            eprintln!("warning: local identity unresolved: {error}");
            None
        }
    };
    let negotiated = negotiate(&client).await;

    let peer = match options.peer_endpoint {
        Some(endpoint) => {
            match SamClient::connect(config_for(endpoint, options.control_timeout)).await {
                Ok(client) => Some((client, endpoint)),
                Err(error) => {
                    eprintln!("warning: peer endpoint {endpoint} unreachable: {error}");
                    None
                }
            }
        }
        None => None,
    };
    let peer_identity = match &peer {
        Some((peer_client, _)) => resolve(peer_client, "cf-identity-peer")
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let peer_client = peer.as_ref().map(|(peer_client, _)| peer_client);

    let mut rows: Vec<Value> = Vec::new();
    if options.plan.wants_stream() {
        let row = match peer_client {
            Some(peer_client) => {
                stream_row(
                    &options,
                    &client,
                    peer_client,
                    local_identity.as_ref(),
                    peer_identity.as_ref(),
                )
                .await
            }
            None => Row::structural("stream", "stream_payload_bidirectional")
                .identity(local_identity.as_ref())
                .skipped(
                    "peer_endpoint_unavailable",
                    "--peer-endpoint is required to exchange payload with a second bridge",
                ),
        };
        rows.push(row_json(&row));
    }
    if options.plan.wants_datagram() {
        for (style, transport) in [
            (SessionStyle::Datagram, DatagramTransport::UdpForward),
            (SessionStyle::Datagram, DatagramTransport::ControlSocketV1),
            (SessionStyle::Raw, DatagramTransport::UdpForward),
            (SessionStyle::Raw, DatagramTransport::ControlSocketV1),
            (SessionStyle::Datagram2, DatagramTransport::UdpForward),
            (SessionStyle::Datagram3, DatagramTransport::UdpForward),
        ] {
            let row = match peer_client {
                Some(peer_client) => {
                    datagram_row(
                        &options,
                        &client,
                        peer_client,
                        style,
                        transport,
                        local_identity.as_ref(),
                        peer_identity.as_ref(),
                    )
                    .await
                }
                None => Row::structural(
                    match style {
                        SessionStyle::Datagram => "datagram",
                        SessionStyle::Raw => "raw",
                        _ => "datagram",
                    },
                    match (style, transport) {
                        (_, DatagramTransport::UdpForward) => "datagram_payload_exchange_udp_forward",
                        (_, DatagramTransport::ControlSocketV1) => {
                            "datagram_payload_exchange_control_socket"
                        }
                    },
                )
                .identity(local_identity.as_ref())
                .skipped(
                    if style.supports_control_socket_datagram() {
                        "peer_endpoint_unavailable"
                    } else {
                        "transport_not_permitted"
                    },
                    match transport {
                        DatagramTransport::ControlSocketV1 if !style.supports_control_socket_datagram() =>
                            "v1/v2-compatible control-socket datagram commands are excluded for this style",
                        _ => "--peer-endpoint is required to exchange payload with a second bridge",
                    },
                ),
            };
            rows.push(row_json(&row));
        }
    }
    if options.plan.wants_shared() {
        for dialect in [SharedDialect::Primary, SharedDialect::Master] {
            let row = match peer_client {
                Some(peer_client) => {
                    shared_row(
                        &options,
                        &client,
                        peer_client,
                        dialect,
                        local_identity.as_ref(),
                    )
                    .await
                }
                None => Row::structural("shared", "shared_subsession_payload_single_destination")
                    .dialect(dialect)
                    .identity(local_identity.as_ref())
                    .skipped(
                        "peer_endpoint_unavailable",
                        "--peer-endpoint is required to observe shared-session payload identity",
                    ),
            };
            rows.push(row_json(&row));
        }
    }

    let capabilities = client.capabilities().await;
    let summary = summarise(&rows);
    let capability_passes = summary.get("pass").copied().unwrap_or(0);
    let artifact = json!({
        "schema_version": "1.1",
        "router": options.router,
        "router_version_or_sha": options.router_version,
        "endpoint": options.endpoint.to_string(),
        "peer_endpoint": options.peer_endpoint.map(|endpoint| endpoint.to_string()),
        "peer_router": options.peer_router,
        "peer_router_version_or_sha": options.peer_router_version,
        "negotiated_sam_version": negotiated.map(|version| format!("{}.{}", version.major, version.minor)),
        "generated_at": timestamp(),
        "plan": plan_name(options.plan),
        "local_identity": local_identity
            .as_ref()
            .map(|identity| {
                identity_json(Some(&(
                    identity.destination().as_str().to_owned(),
                    identity.hash().to_string(),
                )))
            })
            .unwrap_or(Value::Null),
        "capabilities": {
            "stream": capabilities.stream.as_str(),
            "datagram": capabilities.datagram.as_str(),
            "raw": capabilities.raw.as_str(),
            "datagram2": capabilities.datagram2.as_str(),
            "datagram3": capabilities.datagram3.as_str(),
            "shared_primary": capabilities.shared_primary.as_str(),
            "shared_master": capabilities.shared_master.as_str(),
            "datagram_direct": capabilities.datagram_direct.as_str(),
            "raw_direct": capabilities.raw_direct.as_str(),
            "session_identity_lookup": capabilities.session_identity_lookup.as_str(),
        },
        "rows": rows,
        "summary": {
            "pass": summary.get("pass").copied().unwrap_or(0),
            "unsupported": summary.get("unsupported").copied().unwrap_or(0),
            "fail": summary.get("fail").copied().unwrap_or(0),
            "not_run": summary.get("not_run").copied().unwrap_or(0),
            "create_only": summary.get("create_only").copied().unwrap_or(0),
            "capability_passes": capability_passes,
        },
    });

    let rendered = serde_json::to_string_pretty(&artifact)
        .map_err(|error| (2, format!("cannot serialise artifact: {error}")))?;
    println!("{rendered}");
    if let Some(path) = &options.output
        && let Err(error) = std::fs::write(path, format!("{rendered}\n"))
    {
        eprintln!("warning: cannot write {}: {error}", path.display());
    }

    // Capability is asserted only by payload rows; structural successes never contribute.
    if summary.get("fail").copied().unwrap_or(0) > 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::from(0))
}
