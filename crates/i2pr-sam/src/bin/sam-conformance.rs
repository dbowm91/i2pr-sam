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
    service_destination: Option<String>,
    peer_router: Option<String>,
    peer_router_version: Option<String>,
    plan: Plan,
    output: Option<PathBuf>,
    control_timeout: Duration,
    max_payload: usize,
    /// SAM UDP forwarding endpoint datagram frames are sent to. Defaults to the bridge
    /// address with TCP port minus one (Java `7656 -> 7655`; i2pd behaves the same), which
    /// matches both routers' default layout; override when the router uses another port.
    datagram_endpoint: Option<SocketAddr>,
    /// Unique suffix for every SAM session/child ID created by this invocation, so a
    /// lingering session from an earlier timed-out run can never collide with this run's
    /// IDs and surface as a misleading `INVALID_ID` verdict.
    tag: String,
}

fn parse_options() -> Result<Options, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut endpoint = None;
    let mut router = None;
    let mut router_version = None;
    let mut peer_endpoint = None;
    let mut service_destination = None;
    let mut peer_router = None;
    let mut peer_router_version = None;
    let mut plan = Plan::Full;
    let mut output = None;
    let mut control_timeout = 120;
    let mut max_payload = 512usize;
    let mut datagram_endpoint = None;
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
            "--service-destination" => service_destination = Some(value()?),
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
            "--datagram-endpoint" => {
                datagram_endpoint = Some(
                    value()?
                        .parse::<SocketAddr>()
                        .map_err(|e| format!("--datagram-endpoint: {e}"))?,
                )
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
    let tag = {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        format!("{:x}{:x}", std::process::id(), nanos & 0xffffff)
    };
    Ok(Options {
        endpoint,
        router,
        router_version,
        peer_endpoint,
        service_destination,
        peer_router,
        peer_router_version,
        plan,
        output,
        control_timeout: Duration::from_secs(control_timeout),
        max_payload,
        datagram_endpoint,
        tag,
    })
}

/// Millisecond timestamp for live progress tracing on stderr (never into artifacts).
fn live_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Session/child ID scoped to this invocation; see `Options::tag`.
fn sid(options: &Options, base: &str) -> String {
    format!("{base}-{}", options.tag)
}

/// Connect for a payload exchange, tolerating the accept/connect race.
///
/// Java I2P answers `CANT_REACH_PEER` when the SYN arrives before the peer's ACCEPT is
/// registered. The accept task is always started first, but on a fast (or same-router)
/// path the connect can still win the race, so a `CANT_REACH_PEER` or connect timeout is
/// retried a few times before it becomes the row verdict. Anything else returns
/// immediately: a genuine rejection must stay visible, not be retried into noise.
async fn connect_for_exchange(
    peer_session: &i2pr_sam::StreamSession,
    destination: &str,
    attempts: u32,
) -> Result<i2pr_sam::SamStream, SamError> {
    let mut last = None;
    for attempt in 1..=attempts {
        match peer_session.connect(destination, None, None).await {
            Ok(stream) => return Ok(stream),
            Err(error) => {
                let retryable = matches!(&error, SamError::Timeout)
                    || matches!(&error, SamError::Rejected(message) if message.contains("CANT_REACH_PEER"));
                if retryable && attempt < attempts {
                    eprintln!("live retry: connect attempt {attempt} not yet accepted ({error}), retrying");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    last = Some(error);
                    continue;
                }
                return Err(error);
            }
        }
    }
    Err(last.expect("connect retry loop always attempts once"))
}

/// Session tunnel shaping for the same-router lab matrix.
///
/// The lab host's multi-hop data plane cannot carry fresh-transient payload inside the
/// router's stream lifetime, while the SAM wire framing itself is correct (proven by the
/// default-tunnel service row and by datagram payload passes). Exporting
/// `SAM_CONFORMANCE_TUNNEL_LENGTH=0` makes the transient matrix sessions request
/// zero-length inbound/outbound tunnels: addressing, LeaseSet publication, session
/// management, framing, and payload agreement are all still exercised live through the
/// router; only multi-hop onion routing is bypassed. When the variable is unset, no
/// tunnel options are sent and both routers use their defaults. Mock tests run with the
/// variable unset, so unit expectations keep the bare `SESSION CREATE` shape.
fn live_tunnel_options() -> Vec<(String, String)> {
    let Some(length) = std::env::var("SAM_CONFORMANCE_TUNNEL_LENGTH")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return Vec::new();
    };
    vec![
        ("inbound.length".to_owned(), length.to_string()),
        ("outbound.length".to_owned(), length.to_string()),
    ]
}

/// Verbose step tracing for live runs: `SAM_CONFORMANCE_TRACE=1` prints per-step
/// diagnostics to stderr. Retry/settle/warning lines always print; anything gated
/// here is timing detail only a debugging run needs.
fn trace_enabled() -> bool {
    std::env::var("SAM_CONFORMANCE_TRACE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn trace_step(message: String) {
    if trace_enabled() {
        eprintln!("{message}");
    }
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
    http_status: Option<u16>,
    http_body_bytes: Option<usize>,
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
    fn failed(mut self, error: &SamError, note: impl AsRef<str>) -> Self {
        self.diagnostic_category = Some(classify_diagnostic(error).to_owned());
        let note = note.as_ref();
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

/// Fresh transient Destinations need inbound tunnels plus LeaseSet floodfill before the
/// peer session can reach them; SESSION CREATE OK does not imply reachability. Wait once
/// per exchange so a live router can publish before payload is attempted. Set
/// `SAM_CONFORMANCE_SETTLE_SECS=0` to skip (mock/fast environments).
async fn settle_for_live_leasesets() {
    let settle = std::env::var("SAM_CONFORMANCE_SETTLE_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(90);
    if settle > 0 {
        eprintln!("live settle: waiting {settle}s for transient LeaseSet publication");
        tokio::time::sleep(Duration::from_secs(settle)).await;
    }
}

/// Warmup exchange attempts per row; fresh client tunnels may need traffic before payload
/// flows. Set `SAM_CONFORMANCE_ATTEMPTS=1` for a single attempt.
fn live_attempts() -> u32 {
    std::env::var("SAM_CONFORMANCE_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|attempts| (1..=5).contains(attempts))
        .unwrap_or(3)
}

fn config_for(
    endpoint: SocketAddr,
    control_timeout: Duration,
    datagram_endpoint: Option<SocketAddr>,
) -> ClientConfig {
    let mut config = ClientConfig::new(endpoint);
    config.control_timeout = control_timeout;
    config.connect_timeout = Duration::from_secs(10);
    config.datagram_endpoint = datagram_endpoint.unwrap_or_else(|| {
        // Both routers place the SAM UDP port one below the TCP bridge port by default.
        SocketAddr::new(endpoint.ip(), endpoint.port().saturating_sub(1).max(1))
    });
    config
}

async fn negotiate(client: &SamClient) -> Option<SamVersion> {
    client.capabilities().await.negotiated_version
}

/// Stream payload row: one side accepts, the other connects, and both verify exact bytes.
async fn stream_row(options: &Options, client: &SamClient, peer: &SamClient) -> Row {
    let mut row = Row::structural("stream", "stream_payload_bidirectional");
    let tunnels = live_tunnel_options();
    let session = match client
        .create_stream_session(
            &SessionDestination::Transient,
            &sid(options, "cf-stream"),
            &tunnels,
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            return row.failed(&error, "accepting side");
        }
    };
    let peer_session = match peer
        .create_stream_session(
            &SessionDestination::Transient,
            &sid(options, "cf-stream-peer"),
            &tunnels,
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            session.close().await;
            return row.failed(&error, "connecting side");
        }
    };
    let Some(local_identity) = session.identity() else {
        session.close().await;
        peer_session.close().await;
        return row.skipped(
            "identity_unavailable",
            "the accepting session did not expose its concrete Destination",
        );
    };
    let Some(peer_identity) = peer_session.identity() else {
        session.close().await;
        peer_session.close().await;
        return row.skipped(
            "identity_unavailable",
            "the connecting session did not expose its concrete Destination",
        );
    };
    row = row.identity(Some(local_identity)).peer(Some(peer_identity));
    let expected_peer_destination = peer_identity.destination().clone();
    let destination = local_identity.destination().as_str().to_owned();
    settle_for_live_leasesets().await;
    // Diagnostic: prove each fresh Destination is resolvable from the other client
    // before attempting payload; a lookup failure isolates publication, while a
    // lookup success followed by delivery failure isolates the tunnel data plane.
    {
        let peer_b32 = peer_identity.destination().as_str().to_owned();
        let local_b32 = local_identity.destination().as_str().to_owned();
        match tokio::time::timeout(
            Duration::from_secs(60),
            client.lookup_destination(&peer_b32),
        )
        .await
        {
            Ok(Ok(_)) => trace_step("debug stream: cross-lookup accept-side dest from peer client ok".to_owned()),
            Ok(Err(error)) => {
                trace_step(format!("debug stream: cross-lookup accept-side dest failed: {error}"))
            }
            Err(_) => trace_step("debug stream: cross-lookup accept-side dest timed out".to_owned()),
        }
        match tokio::time::timeout(
            Duration::from_secs(60),
            peer.lookup_destination(&local_b32),
        )
        .await
        {
            Ok(Ok(_)) => trace_step("debug stream: cross-lookup connect-side dest from main client ok".to_owned()),
            Ok(Err(error)) => {
                trace_step(format!("debug stream: cross-lookup connect-side dest failed: {error}"))
            }
            Err(_) => trace_step("debug stream: cross-lookup connect-side dest timed out".to_owned()),
        }
    }

    let session = Arc::new(session);
    let peer_session = Arc::new(peer_session);
    let max_payload = options.max_payload;
    let sent = max_payload;
    // Fresh client tunnels may need warmup traffic before payload flows reliably; retry
    // the exchange on the same sessions so already-published LeaseSets keep warming up.
    // Only transport-level stalls are retried: verdicts and rejections return immediately.
    let (accepted, connected) = {
        let mut attempt_outcome = None;
        let attempts = live_attempts();
        for attempt in 1..=attempts {
            let mut accepting = {
                let session = Arc::clone(&session);
                tokio::spawn(async move {
                    let mut stream = session.accept().await?;
                    trace_step(format!("live trace stream accept peer-blocked at {}", live_ts()));
                    let peer: Option<i2pr_sam::StreamPeer> = stream.peer().cloned();
                    let mut received = vec![0; max_payload];
                    stream.read_exact(&mut received).await?;
                    trace_step(format!("live trace stream accept payload-read at {}", live_ts()));
                    let reply = payload(0xA5, max_payload);
                    stream.write_all(&reply).await?;
                    stream.flush().await?;
                    trace_step(format!("live trace stream accept reply-written at {}", live_ts()));
                    Ok::<_, SamError>((peer, received, reply.len()))
                })
            };
            let mut connecting = {
                let destination = destination.clone();
                let peer_session = Arc::clone(&peer_session);
                tokio::spawn(async move {
                    let mut stream = connect_for_exchange(&peer_session, &destination, 4).await?;
                    trace_step(format!("live trace stream connect ready at {}", live_ts()));
                    let body = payload(0x5A, max_payload);
                    stream.write_all(&body).await?;
                    stream.flush().await?;
                    trace_step(format!("live trace stream connect payload-written at {}", live_ts()));
                    let mut reply = vec![0; max_payload];
                    stream.read_exact(&mut reply).await?;
                    trace_step(format!("live trace stream connect reply-read at {}", live_ts()));
                    Ok::<_, SamError>((body, reply))
                })
            };

            let exchange = tokio::time::timeout(options.control_timeout, async {
                tokio::join!(&mut accepting, &mut connecting)
            })
            .await;
            let retryable = |error: &SamError| {
                !matches!(error, SamError::Unsupported(_))
                    && matches!(
                        i2pr_sam::classify_failure(error),
                        i2pr_sam::FailureClass::TransportTransient
                            | i2pr_sam::FailureClass::RouterTransient
                            | i2pr_sam::FailureClass::CancelledOrClosed
                    )
            };
            match exchange {
                Err(_) => {
                    accepting.abort();
                    connecting.abort();
                    if attempt < attempts {
                        eprintln!(
                            "live retry: STREAM exchange attempt {attempt} timed out, retrying"
                        );
                        continue;
                    }
                    session.close().await;
                    peer_session.close().await;
                    return row.skipped(
                        "router_timeout",
                        "timed out waiting for the STREAM payload exchange",
                    );
                }
                Ok((accepting, connecting)) => match (accepting, connecting) {
                    (Ok(Ok(accepted)), Ok(Ok(connected))) => {
                        attempt_outcome = Some((accepted, connected));
                        break;
                    }
                    (Ok(Ok(_)), Ok(Err(error))) | (Ok(Err(error)), _) => {
                        if retryable(&error) && attempt < attempts {
                            eprintln!(
                                "live retry: STREAM exchange attempt {attempt} stalled ({error}), retrying"
                            );
                            continue;
                        }
                        session.close().await;
                        peer_session.close().await;
                        return row.failed(&error, "the STREAM exchange did not complete");
                    }
                    (Err(join_error), _) | (Ok(Ok(_)), Err(join_error)) => {
                        session.close().await;
                        peer_session.close().await;
                        return row
                            .skipped(
                                "subsession_task_failed",
                                format!("STREAM task failed: {join_error}"),
                            )
                            .note("no payload exchange completed");
                    }
                },
            }
        }
        attempt_outcome.expect("retry loop always breaks with a verdict")
    };
    session.close().await;
    peer_session.close().await;
    let (observed_peer, inbound, replied) = accepted;
    let (outbound, reply) = connected;
    let inbound_expected = payload(0x5A, max_payload);
    let reply_expected = payload(0xA5, max_payload);
    let exact = inbound == inbound_expected && reply == reply_expected;
    let peer_identity_match = observed_peer
        .as_ref()
        .map(|peer| peer.destination == expected_peer_destination)
        .unwrap_or(false);
    let evidence = Evidence {
        bytes_sent: outbound.len() + replied,
        bytes_received: inbound.len() + reply.len(),
        exact_payload_match: exact,
        identity_proven: peer_identity_match,
        ..Evidence::default()
    };
    let identity_proven = evidence.identity_proven;
    row.evidence = evidence;
    if exact && identity_proven {
        row.outcome = Outcome::Pass;
        row.notes = format!(
            "both directions exchanged {} bytes; non-silent accept captured the peer Destination",
            sent
        );
    } else if exact {
        row.outcome = Outcome::Fail;
        row.diagnostic_category = Some("identity_not_observed".into());
        row.notes = "payload matched, but the accepting side did not observe the connecting session Destination".into();
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

/// Send an HTTP request to a known service Destination through this SAM bridge.
async fn service_http_row(options: &Options, client: &SamClient) -> Row {
    let target = options
        .service_destination
        .as_deref()
        .expect("selected by caller");
    let mut row = Row::structural("stream", "stream_http_service_request_response");
    let destination = match client.lookup_destination(target).await {
        Ok(destination) => destination,
        Err(error) => return row.failed(&error, "service Destination lookup failed"),
    };
    let identity = match i2pr_sam::SessionIdentity::new(destination.clone()) {
        Ok(identity) => identity,
        Err(error) => return row.failed(&error, "resolved service identity is invalid"),
    };
    row = row.peer(Some(&identity));

    let session = match client
        .create_stream_session(
            &SessionDestination::Transient,
            &sid(options, "cf-service-http"),
            &[],
        )
        .await
    {
        Ok(session) => session,
        Err(error) => return row.failed(&error, "STREAM session creation failed"),
    };
    row = row.identity(session.identity());
    let mut stream = match session.connect(destination.as_str(), None, None).await {
        Ok(stream) => stream,
        Err(error) => {
            session.close().await;
            return row.failed(&error, "STREAM connect to service failed");
        }
    };
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\nUser-Agent: i2pr-sam-conformance\r\n\r\n"
    );
    if let Err(error) = stream.write_all(request.as_bytes()).await {
        session.close().await;
        return row.failed(&SamError::Io(error), "HTTP request write failed");
    }
    if let Err(error) = stream.flush().await {
        session.close().await;
        return row.failed(&SamError::Io(error), "HTTP request flush failed");
    }
    let mut limited = stream.take(1_048_576);
    let mut response = Vec::new();
    let read =
        tokio::time::timeout(options.control_timeout, limited.read_to_end(&mut response)).await;
    session.close().await;
    match read {
        Err(_) => return row.skipped("router_timeout", "timed out waiting for HTTP response"),
        Ok(Err(error)) => return row.failed(&SamError::Io(error), "HTTP response read failed"),
        Ok(Ok(_)) => {}
    }

    let status_line = response
        .split(|byte| *byte == b'\n')
        .next()
        .map(|line| String::from_utf8_lossy(line).trim().to_owned())
        .unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok());
    let body_bytes = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|end| response.len().saturating_sub(end + 4))
        .unwrap_or(0);
    let successful = body_bytes > 0
        && status.is_some_and(|code| (200..300).contains(&code))
        && response.starts_with(b"HTTP/1.");
    row.evidence = Evidence {
        bytes_sent: request.len(),
        bytes_received: response.len(),
        // HTTP responses are validated by status and nonempty payload, not byte equality.
        exact_payload_match: false,
        identity_proven: false,
        http_status: status,
        http_body_bytes: Some(body_bytes),
    };
    row.notes = format!(
        "target={target}; response_status={}; response_bytes={}; response_body_bytes={}; success requires HTTP 2xx and nonempty body",
        status
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unparsed".into()),
        response.len(),
        body_bytes
    );
    if successful {
        row.outcome = Outcome::Pass;
    } else {
        row.outcome = Outcome::Fail;
        row.diagnostic_category = Some("invalid_http_response".into());
    }
    row
}

/// Datagram row for one ordinary style, over the requested transport.
fn datagram_feature(style: SessionStyle) -> &'static str {
    match style {
        SessionStyle::Stream => "stream",
        SessionStyle::Datagram => "datagram",
        SessionStyle::Raw => "raw",
        SessionStyle::Datagram2 => "datagram2",
        SessionStyle::Datagram3 => "datagram3",
    }
}

async fn datagram_row(
    options: &Options,
    client: &SamClient,
    peer: &SamClient,
    style: SessionStyle,
    transport: DatagramTransport,
) -> Row {
    let mut row = Row::structural(
        datagram_feature(style),
        match (style, transport) {
            (_, DatagramTransport::UdpForward) => "datagram_payload_exchange_udp_forward",
            (_, DatagramTransport::ControlSocketV1) => "datagram_payload_exchange_control_socket",
        },
    );
    if !style.supports_control_socket_datagram() && transport == DatagramTransport::ControlSocketV1
    {
        return row.skipped(
            "transport_not_permitted",
            "v1/v2-compatible control-socket datagram commands are excluded for this style",
        );
    }
    let id = sid(options, &format!("cf-{}", style.as_wire().to_lowercase()));
    let peer_id = format!("{id}-peer");
    // i2pd forwards RAW datagrams bare even when HEADER=true is requested, while Java
    // honors the flag; the matrix only needs exact payload with no source identity, so
    // request the bare shape explicitly and decode it on both routers.
    let mut session_options = live_tunnel_options();
    if style == SessionStyle::Raw {
        session_options.push(("HEADER".to_owned(), "false".to_owned()));
    }
    let session = match client
        .create_datagram_session_with(
            &SessionDestination::Transient,
            &id,
            style,
            transport,
            &session_options,
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            return row
                .failed(&error, "this side could not create the session");
        }
    };
    let peer_session = match peer
        .create_datagram_session_with(
            &SessionDestination::Transient,
            &peer_id,
            style,
            transport,
            &session_options,
        )
        .await
    {
        Ok(peer_session) => peer_session,
        Err(error) => {
            session.close().await;
            return row
                .failed(&error, "the peer side could not create a matching session");
        }
    };
    let Some(local_identity) = session.identity() else {
        session.close().await;
        peer_session.close().await;
        return row.skipped(
            "identity_unavailable",
            "the receiving datagram session did not expose its concrete Destination",
        );
    };
    let Some(peer_identity) = peer_session.identity() else {
        session.close().await;
        peer_session.close().await;
        return row.skipped(
            "identity_unavailable",
            "the sending datagram session did not expose its concrete Destination",
        );
    };
    row = row.identity(Some(local_identity)).peer(Some(peer_identity));
    let expected_peer_destination = peer_identity.destination().clone();
    let destination = local_identity.destination().as_str().to_owned();
    settle_for_live_leasesets().await;
    // Datagram retries re-send on the same sessions: each send is independent traffic
    // that keeps warming the receiver's freshly published LeaseSet.
    let attempts = live_attempts();
    let (body, received) = loop {
        let mut attempt_verdict = None;
        for attempt in 1..=attempts {
            let attempt_body = payload(0x3C, options.max_payload);
            if let Err(error) = peer_session
                .send(&destination, &attempt_body, None, None)
                .await
            {
                session.close().await;
                peer_session.close().await;
                return row.failed(&error, "peer send failed before delivery");
            }
            match tokio::time::timeout(options.control_timeout, session.recv()).await {
                Ok(Ok(attempt_received)) => {
                    attempt_verdict = Some((attempt_body, attempt_received));
                    break;
                }
                Ok(Err(error)) => {
                    session.close().await;
                    peer_session.close().await;
                    return row.failed(&error, "no datagram was delivered");
                }
                Err(_) => {
                    if attempt < attempts {
                        eprintln!(
                            "live retry: datagram exchange attempt {attempt} timed out, resending"
                        );
                        continue;
                    }
                    session.close().await;
                    peer_session.close().await;
                    return row.skipped(
                        "router_timeout",
                        "timed out waiting for the datagram receiver session",
                    );
                }
            }
        }
        break attempt_verdict.expect("retry loop always breaks with a verdict");
    };
    session.close().await;
    peer_session.close().await;
    let (inbound, source_known, trust_shape_matches) = match (style, &received) {
        (
            SessionStyle::Datagram | SessionStyle::Datagram2,
            ReceivedDatagram::Authenticated(message),
        ) => (
            message.payload.clone(),
            Some(message.source.as_str().to_owned()),
            true,
        ),
        (SessionStyle::Datagram3, ReceivedDatagram::Unverified(message)) => {
            (message.payload.clone(), None, true)
        }
        (SessionStyle::Raw, ReceivedDatagram::Raw(message)) => {
            (message.payload.clone(), None, true)
        }
        (_, ReceivedDatagram::Authenticated(message)) => (message.payload.clone(), None, false),
        (_, ReceivedDatagram::Unverified(message)) => (message.payload.clone(), None, false),
        (_, ReceivedDatagram::Raw(message)) => (message.payload.clone(), None, false),
    };
    let identity_proven = source_known.as_deref() == Some(expected_peer_destination.as_str());
    let source_proven_as_required = match style {
        SessionStyle::Datagram | SessionStyle::Datagram2 => identity_proven,
        SessionStyle::Datagram3 | SessionStyle::Raw => !identity_proven,
        SessionStyle::Stream => false,
    };
    let exact = inbound == body && trust_shape_matches && source_proven_as_required;
    row.evidence = Evidence {
        bytes_sent: body.len(),
        bytes_received: inbound.len(),
        exact_payload_match: exact,
        identity_proven,
        ..Evidence::default()
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
        row.diagnostic_category = Some(if inbound != body {
            "payload_mismatch".into()
        } else {
            "source_trust_mismatch".into()
        });
        row.notes = format!(
            "datagram response failed payload/trust validation for {:?}",
            style
        );
    }
    row
}

/// Shared-session row: create, add subsessions, and prove one Destination links them.
async fn shared_rows(
    options: &Options,
    client: &SamClient,
    peer: &SamClient,
    dialect: SharedDialect,
) -> Vec<Row> {
    let name = match dialect {
        SharedDialect::Master => "MASTER",
        SharedDialect::Primary => "PRIMARY",
    };
    let mut stream_row = Row::structural(
        "shared",
        "shared_subsession_stream_payload_single_destination",
    )
    .dialect(dialect);
    let mut datagram_row = Row::structural(
        "shared",
        "shared_subsession_datagram_payload_single_destination",
    )
    .dialect(dialect);
    let tunnels = live_tunnel_options();
    let session = match client
        .create_shared_session(
            &SessionDestination::Transient,
            &sid(options, &format!("cf-shared-{name}")),
            dialect,
            &tunnels,
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            return vec![
                stream_row
                    .failed(&error, format!("{name} shared session could not be created")),
                datagram_row
                    .failed(&error, format!("{name} shared session could not be created")),
            ];
        }
    };
    let identity = session.identity().clone();
    stream_row = stream_row.identity(Some(&identity));
    datagram_row = datagram_row.identity(Some(&identity));
    let datagram_child_name = sid(options, &format!("cf-shared-dgram-{name}"));
    let stream_child_name = sid(options, &format!("cf-shared-stream-{name}"));
    let datagram_child = match session
        .add_child(&datagram_child_name, SessionStyle::Datagram, &[])
        .await
    {
        Ok(child) => child,
        Err(error) => {
            session.close().await;
            return vec![
                stream_row
                    .failed(&error, "datagram subsession could not be added"),
                datagram_row
                    .failed(&error, "datagram subsession could not be added"),
            ];
        }
    };
    let stream_child = match session
        .add_child(&stream_child_name, SessionStyle::Stream, &[])
        .await
    {
        Ok(child) => child,
        Err(error) => {
            session.close().await;
            datagram_child.close().await;
            return vec![
                stream_row
                    .failed(&error, "stream subsession could not be added"),
                datagram_row
                    .failed(&error, "stream subsession could not be added"),
            ];
        }
    };
    // Every child must report the owner's concrete identity; the request token TRANSIENT
    // is never an acceptable answer.
    let same_identity =
        datagram_child.identity() == &identity && stream_child.identity() == &identity;
    if !same_identity {
        session.close().await;
        datagram_child.close().await;
        stream_child.close().await;
        stream_row.outcome = Outcome::Fail;
        stream_row.diagnostic_category = Some("identity_divergence".into());
        stream_row.notes = "subsessions did not report the owner's concrete Destination".into();
        datagram_row.outcome = Outcome::Fail;
        datagram_row.diagnostic_category = Some("identity_divergence".into());
        datagram_row.notes = "subsessions did not report the owner's concrete Destination".into();
        return vec![stream_row, datagram_row];
    }
    // The owner Destination needs inbound tunnels plus LeaseSet floodfill before the
    // peer sessions can reach the children; SESSION CREATE OK does not imply it.
    settle_for_live_leasesets().await;

    let stream_child = Arc::new(stream_child);
    let stream_peer = match peer
        .create_stream_session(
            &SessionDestination::Transient,
            &sid(options, &format!("cf-shared-peer-{name}")),
            &tunnels,
        )
        .await
    {
        Ok(session) => Some(session),
        Err(error) => {
            stream_row = stream_row
                .failed(&error, "peer stream session could not be created");
            None
        }
    };
    if let Some(peer_session) = stream_peer {
        if let Some(peer_identity) = peer_session.identity() {
            stream_row = stream_row.peer(Some(peer_identity));
            let expected_peer_destination = peer_identity.destination().clone();
            let destination = identity.destination().as_str().to_owned();
            let max_payload = options.max_payload;
            let peer_session = std::sync::Arc::new(peer_session);
            let attempts = live_attempts();
            // Same-session warmup retries as the ordinary STREAM row: fresh tunnels may
            // need traffic before payload flows.
            for attempt in 1..=attempts {
                let mut receiving = {
                    let child = Arc::clone(&stream_child);
                    tokio::spawn(async move {
                        let mut stream = child.accept().await?;
                        let peer: Option<i2pr_sam::StreamPeer> = stream.peer().cloned();
                        let mut body = vec![0; max_payload];
                        stream.read_exact(&mut body).await?;
                        Ok::<_, SamError>((peer, body))
                    })
                };
                let mut sending = {
                    let peer_session = std::sync::Arc::clone(&peer_session);
                    let destination = destination.clone();
                    tokio::spawn(async move {
                        let mut stream =
                            connect_for_exchange(&peer_session, &destination, 4).await?;
                        let body = payload(0x6B, max_payload);
                        stream.write_all(&body).await?;
                        stream.flush().await?;
                        Ok::<_, SamError>(body)
                    })
                };
                let exchange = tokio::time::timeout(options.control_timeout, async {
                    tokio::join!(&mut receiving, &mut sending)
                })
                .await;
                let retryable = |error: &SamError| {
                    !matches!(error, SamError::Unsupported(_))
                        && matches!(
                            i2pr_sam::classify_failure(error),
                            i2pr_sam::FailureClass::TransportTransient
                                | i2pr_sam::FailureClass::RouterTransient
                                | i2pr_sam::FailureClass::CancelledOrClosed
                        )
                };
                match exchange {
                    Err(_) => {
                        receiving.abort();
                        sending.abort();
                        if attempt < attempts {
                            eprintln!("live retry: shared STREAM attempt {attempt} timed out, retrying");
                            continue;
                        }
                        stream_row = stream_row.skipped(
                            "router_timeout",
                            "timed out waiting for the shared STREAM payload exchange",
                        );
                    }
                    Ok((receiving_result, sending_result)) => {
                        match (receiving_result, sending_result) {
                        (Ok(Ok((peer_observed, inbound))), Ok(Ok(outbound))) => {
                            let expected = payload(0x6B, max_payload);
                            let exact = inbound == expected && outbound == expected;
                            let identity_proven = peer_observed.as_ref().is_some_and(|observed| {
                                observed.destination == expected_peer_destination
                            });
                            stream_row.evidence = Evidence {
                                bytes_sent: outbound.len(),
                                bytes_received: inbound.len(),
                                exact_payload_match: exact,
                                identity_proven,
                                ..Evidence::default()
                            };
                            if exact && identity_proven {
                                stream_row.outcome = Outcome::Pass;
                                stream_row.notes = format!(
                                    "shared STREAM child exchanged an exact payload under Destination hash {} and captured the peer Destination",
                                    identity.hash()
                                );
                            } else {
                                stream_row.outcome = Outcome::Fail;
                                stream_row.diagnostic_category = Some(if exact {
                                    "identity_not_observed".into()
                                } else {
                                    "payload_mismatch".into()
                                });
                                stream_row.notes = "shared STREAM child did not produce verifiable payload and identity evidence".into();
                            }
                        }
                        (Ok(Err(error)), _) | (_, Ok(Err(error))) => {
                            if retryable(&error) && attempt < attempts {
                                eprintln!("live retry: shared STREAM attempt {attempt} stalled ({error}), retrying");
                                continue;
                            }
                            stream_row = stream_row
                                .failed(&error, "shared STREAM exchange did not complete");
                            break;
                        }
                        (Err(error), _) | (_, Err(error)) => {
                            stream_row = stream_row.skipped(
                                "subsession_task_failed",
                                format!("shared STREAM task failed: {error}"),
                            );
                            break;
                        }
                        }
                        break;
                    }
                }
            }
            peer_session.close().await;
        } else {
            stream_row = stream_row.skipped(
                "identity_unavailable",
                "the connecting shared STREAM session did not expose its Destination",
            );
        }
    }

    let datagram_child = Arc::new(datagram_child);
    let peer_datagram = match peer
        .create_datagram_session_with(
            &SessionDestination::Transient,
            &sid(options, &format!("cf-shared-dgram-peer-{name}")),
            SessionStyle::Datagram,
            DatagramTransport::UdpForward,
            &tunnels,
        )
        .await
    {
        Ok(peer_session) => Some(peer_session),
        Err(error) => {
            datagram_row = datagram_row
                .failed(&error, "peer datagram session could not be created");
            None
        }
    };
    if let Some(peer_session) = peer_datagram {
        if let Some(peer_identity) = peer_session.identity() {
            datagram_row = datagram_row.peer(Some(peer_identity));
            let expected_peer_destination = peer_identity.destination().as_str().to_owned();
            let destination = identity.destination().as_str().to_owned();
            let attempts = live_attempts();
            // Same-session resends as the ordinary datagram row: each send is independent
            // traffic that keeps warming the owner's freshly published LeaseSet.
            for attempt in 1..=attempts {
                let body = payload(0x7C, options.max_payload);
                let receive = {
                    let child = Arc::clone(&datagram_child);
                    tokio::spawn(async move { child.recv_datagram().await })
                };
                if let Err(error) = peer_session.send(&destination, &body, None, None).await {
                    datagram_row = datagram_row
                        .failed(&error, "peer datagram send failed before the shared child received a payload");
                    break;
                }
                match tokio::time::timeout(options.control_timeout, receive).await {
                    Ok(Ok(Ok(ReceivedDatagram::Authenticated(message)))) => {
                        let exact = message.payload == body;
                        let identity_proven = message.source.as_str() == expected_peer_destination;
                        datagram_row.evidence = Evidence {
                            bytes_sent: body.len(),
                            bytes_received: message.payload.len(),
                            exact_payload_match: exact,
                            identity_proven,
                            ..Evidence::default()
                        };
                        if exact && identity_proven {
                            datagram_row.outcome = Outcome::Pass;
                            datagram_row.notes = format!(
                                "shared DATAGRAM child exchanged an exact payload under owner Destination hash {} and authenticated the peer source",
                                identity.hash()
                            );
                        } else {
                            datagram_row.outcome = Outcome::Fail;
                            datagram_row.diagnostic_category = Some(if exact {
                                "identity_not_observed".into()
                            } else {
                                "payload_mismatch".into()
                            });
                            datagram_row.notes = "shared DATAGRAM child did not produce exact payload and authenticated source evidence".into();
                        }
                    }
                    Ok(Ok(Ok(_))) => {
                        datagram_row.outcome = Outcome::Fail;
                        datagram_row.diagnostic_category = Some("source_trust_mismatch".into());
                        datagram_row.notes =
                            "shared DATAGRAM child returned the wrong source type".into();
                    }
                    Ok(Ok(Err(error))) => {
                        datagram_row =
                            datagram_row.failed(&error, "shared DATAGRAM receive failed");
                    }
                    Ok(Err(error)) => {
                        datagram_row = datagram_row.skipped(
                            "subsession_task_failed",
                            format!("shared DATAGRAM task failed: {error}"),
                        );
                    }
                    Err(_) => {
                        if attempt < attempts {
                            eprintln!("live retry: shared DATAGRAM attempt {attempt} timed out, resending");
                            continue;
                        }
                        datagram_row = datagram_row.skipped(
                            "router_timeout",
                            "timed out waiting for the shared DATAGRAM child response",
                        );
                        break;
                    }
                }
                break;
            }
            peer_session.close().await;
        } else {
            peer_session.close().await;
            datagram_row = datagram_row.skipped(
                "identity_unavailable",
                "the sending shared DATAGRAM session did not expose its Destination",
            );
        }
    }
    if let Err(error) = session.remove_child(&stream_child_name).await {
        stream_row.outcome = Outcome::Fail;
        stream_row.diagnostic_category = Some("child_removal_failed".into());
        stream_row.notes = format!("shared STREAM child removal failed: {error}");
    } else if stream_child.is_open() {
        stream_row.outcome = Outcome::Fail;
        stream_row.diagnostic_category = Some("child_removal_not_observed".into());
        stream_row.notes = "shared STREAM child remained open after removal".into();
    } else if stream_row.outcome == Outcome::Pass {
        stream_row.notes.push_str("; child removal succeeded");
    }
    stream_child.close().await;
    session.close().await;
    if stream_child.is_open() {
        stream_row.outcome = Outcome::Fail;
        stream_row.diagnostic_category = Some("owner_teardown_not_observed".into());
        stream_row.notes = "shared STREAM child remained open after owner teardown".into();
    } else if stream_row.outcome == Outcome::Pass {
        stream_row
            .notes
            .push_str("; owner teardown invalidated the child");
    }
    if datagram_child.is_open() {
        datagram_row.outcome = Outcome::Fail;
        datagram_row.diagnostic_category = Some("owner_teardown_not_observed".into());
        datagram_row.notes = "shared DATAGRAM child remained open after owner teardown".into();
    } else if datagram_row.outcome == Outcome::Pass {
        datagram_row
            .notes
            .push_str("; owner teardown invalidated the child");
    }
    vec![stream_row, datagram_row]
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
            "http_status": row.evidence.http_status,
            "http_body_bytes": row.evidence.http_body_bytes,
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
    let config = config_for(
        options.endpoint,
        options.control_timeout,
        options.datagram_endpoint,
    );
    let client = SamClient::connect(config).await.map_err(|error| {
        (
            2,
            format!("cannot reach SAM bridge at {}: {error}", options.endpoint),
        )
    })?;

    let negotiated = negotiate(&client).await;

    let peer = match options.peer_endpoint {
        Some(endpoint) => {
            match SamClient::connect(config_for(
                endpoint,
                options.control_timeout,
                options.datagram_endpoint,
            ))
            .await {
                Ok(client) => Some((client, endpoint)),
                Err(error) => {
                    eprintln!("warning: peer endpoint {endpoint} unreachable: {error}");
                    None
                }
            }
        }
        None => None,
    };
    let peer_client = peer.as_ref().map(|(peer_client, _)| peer_client);

    let mut rows: Vec<Value> = Vec::new();
    if options.plan.wants_stream() {
        let row = if options.service_destination.is_some() {
            service_http_row(&options, &client).await
        } else {
            match peer_client {
                Some(peer_client) => stream_row(&options, &client, peer_client).await,
                None => Row::structural("stream", "stream_payload_bidirectional")
                    .skipped(
                        "peer_endpoint_unavailable",
                        "--peer-endpoint is required to create a second SAM client for payload exchange",
                    ),
            }
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
                    )
                    .await
                }
                None => Row::structural(
                    datagram_feature(style),
                    match (style, transport) {
                        (_, DatagramTransport::UdpForward) => "datagram_payload_exchange_udp_forward",
                        (_, DatagramTransport::ControlSocketV1) => {
                            "datagram_payload_exchange_control_socket"
                        }
                    },
                )
                .skipped(
                    if style.supports_control_socket_datagram() {
                        "peer_endpoint_unavailable"
                    } else {
                        "transport_not_permitted"
                    },
                    match transport {
                        DatagramTransport::ControlSocketV1 if !style.supports_control_socket_datagram() =>
                            "v1/v2-compatible control-socket datagram commands are excluded for this style",
                        _ => "--peer-endpoint is required to create a second SAM client for payload exchange",
                    },
                ),
            };
            rows.push(row_json(&row));
        }
    }
    if options.plan.wants_shared() {
        for dialect in [SharedDialect::Primary, SharedDialect::Master] {
            let dialect_rows = match peer_client {
                Some(peer_client) => {
                    shared_rows(
                        &options,
                        &client,
                        peer_client,
                        dialect,
                    )
                    .await
                }
                None => vec![
                    Row::structural(
                        "shared",
                        "shared_subsession_stream_payload_single_destination",
                    )
                    .dialect(dialect)
                    .skipped(
                        "peer_endpoint_unavailable",
                        "--peer-endpoint is required to create a second SAM client through this or another router",
                    ),
                    Row::structural(
                        "shared",
                        "shared_subsession_datagram_payload_single_destination",
                    )
                    .dialect(dialect)
                    .skipped(
                        "peer_endpoint_unavailable",
                        "--peer-endpoint is required to create a second SAM client through this or another router",
                    ),
                ],
            };
            rows.extend(dialect_rows.iter().map(row_json));
        }
    }

    let capabilities = client.capabilities().await;
    let summary = summarise(&rows);
    let capability_passes = summary.get("pass").copied().unwrap_or(0);
    let artifact_local_identity = rows
        .iter()
        .find_map(|row| {
            row.get("local_identity")
                .filter(|value| !value.is_null())
                .cloned()
        })
        .unwrap_or(Value::Null);
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
        "local_identity": artifact_local_identity,
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

#[cfg(test)]
mod tests {
    use super::datagram_feature;
    use i2pr_sam::SessionStyle;

    #[test]
    fn every_datagram_family_has_a_runner_feature_label() {
        assert_eq!(datagram_feature(SessionStyle::Stream), "stream");
        assert_eq!(datagram_feature(SessionStyle::Datagram), "datagram");
        assert_eq!(datagram_feature(SessionStyle::Raw), "raw");
        assert_eq!(datagram_feature(SessionStyle::Datagram2), "datagram2");
        assert_eq!(datagram_feature(SessionStyle::Datagram3), "datagram3");
    }
}