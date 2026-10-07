//! Async SAM client. Protocol framing and capability types are re-exported from the
//! runtime-neutral `i2pr-sam-proto` crate.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use i2pr_sam_proto::{Line, SamCapabilities, SamVersion, Support, parse_line};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    net::{TcpStream, UdpSocket},
    sync::{Mutex, Notify, OwnedSemaphorePermit, RwLock, Semaphore},
    time::timeout,
};

pub use i2pr_sam_proto as proto;
use i2pr_sam_proto::{
    AuthenticatedDatagram, Destination, I2pProtocol, Port, RawDatagram, ReceivedDatagram,
    SessionId, SessionStyle, SharedDialect, UnverifiedDatagram3, UnverifiedSourceHash,
};

#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &"[REDACTED]")
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub endpoint: SocketAddr,
    pub datagram_endpoint: SocketAddr,
    pub datagram_forward: DatagramForwardConfig,
    pub min_version: SamVersion,
    pub max_version: SamVersion,
    pub connect_timeout: Duration,
    pub control_timeout: Duration,
    pub max_frame_bytes: usize,
    pub max_datagram_bytes: usize,
    pub credentials: Option<Credentials>,
}

#[derive(Clone, Debug)]
pub struct DatagramForwardConfig {
    /// Local interface that receives SAM forwarded UDP packets.
    pub bind_ip: IpAddr,
    /// Address advertised to the SAM bridge as the packet destination.
    pub advertised_host: IpAddr,
    /// Local UDP port, or zero to allocate an ephemeral port for loopback forwarding.
    pub port: u16,
}
impl ClientConfig {
    pub fn new(endpoint: SocketAddr) -> Self {
        Self {
            endpoint,
            datagram_endpoint: SocketAddr::new(endpoint.ip(), 7655),
            datagram_forward: DatagramForwardConfig {
                bind_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                advertised_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 0,
            },
            min_version: SamVersion::V3_1,
            max_version: SamVersion::V3_3,
            connect_timeout: Duration::from_secs(10),
            control_timeout: Duration::from_secs(30),
            max_frame_bytes: i2pr_sam_proto::MAX_LINE_BYTES,
            max_datagram_bytes: 32_768,
            credentials: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SamError {
    #[error("SAM transport failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SAM operation timed out")]
    Timeout,
    #[error("invalid SAM protocol response: {0}")]
    Protocol(#[from] i2pr_sam_proto::ParseError),
    #[error("SAM bridge rejected the operation: {0}")]
    Rejected(String),
    #[error("name was not found by the SAM bridge")]
    NameNotFound,
    #[error("client or session is closed")]
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    TransportTransient,
    ProtocolPermanent,
    CapabilityUnsupported,
    ConfigurationPermanent,
    RouterTransient,
    CancelledOrClosed,
}

#[derive(Clone, Debug)]
pub struct ReconnectPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_elapsed: Duration,
}
impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            max_elapsed: Duration::from_secs(30),
        }
    }
}
impl ReconnectPolicy {
    pub fn bounded(
        max_attempts: u32,
        initial_backoff: Duration,
        max_backoff: Duration,
        max_elapsed: Duration,
    ) -> Result<Self, SamError> {
        if max_attempts == 0 || initial_backoff > max_backoff || max_elapsed.is_zero() {
            return Err(SamError::Rejected("invalid reconnect policy bounds".into()));
        }
        Ok(Self {
            max_attempts,
            initial_backoff,
            max_backoff,
            max_elapsed,
        })
    }
}

pub fn classify_failure(error: &SamError) -> FailureClass {
    match error {
        SamError::Io(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::WouldBlock
            ) =>
        {
            FailureClass::TransportTransient
        }
        SamError::Timeout => FailureClass::TransportTransient,
        SamError::Closed => FailureClass::CancelledOrClosed,
        SamError::Rejected(_) => FailureClass::ConfigurationPermanent,
        SamError::Protocol(_) => FailureClass::ProtocolPermanent,
        SamError::Io(_) => FailureClass::ConfigurationPermanent,
        SamError::NameNotFound => FailureClass::RouterTransient,
    }
}

pub struct SamClient {
    config: ClientConfig,
    capabilities: Arc<RwLock<SamCapabilities>>,
    utility: Mutex<Control<TcpStream>>,
}

struct Control<S> {
    reader: BufReader<S>,
}
impl<S> Control<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    async fn command(
        &mut self,
        command: &str,
        deadline: Duration,
        max_frame: usize,
    ) -> Result<Line, SamError> {
        if command.len() > max_frame
            || !command.ends_with('\n')
            || command.bytes().filter(|b| *b == b'\n').count() != 1
            || command.contains('\r')
        {
            return Err(SamError::Protocol(i2pr_sam_proto::ParseError::TooLong));
        }
        self.reader.get_mut().write_all(command.as_bytes()).await?;
        self.reader.get_mut().flush().await?;
        let line = timeout(deadline, read_line_bounded(&mut self.reader, max_frame))
            .await
            .map_err(|_| SamError::Timeout)??;
        if line.is_empty() {
            return Err(SamError::Closed);
        }
        Ok(parse_line(&line)?)
    }
}

async fn read_line_bounded<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    ceiling: usize,
) -> Result<Vec<u8>, SamError> {
    let mut line = Vec::with_capacity(256.min(ceiling));
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(line)
            } else {
                Err(SamError::Protocol(
                    i2pr_sam_proto::ParseError::InvalidLineEnding,
                ))
            };
        }
        let newline = available.iter().position(|b| *b == b'\n');
        let count = newline.map_or(available.len(), |i| i + 1);
        if line.len().saturating_add(count) > ceiling {
            return Err(SamError::Protocol(i2pr_sam_proto::ParseError::TooLong));
        }
        line.extend_from_slice(&available[..count]);
        let done = newline.is_some();
        reader.consume(count);
        if done {
            return Ok(line);
        }
    }
}

impl SamClient {
    pub async fn connect_with_policy(
        config: ClientConfig,
        policy: ReconnectPolicy,
    ) -> Result<Self, SamError> {
        if policy.max_attempts == 0
            || policy.initial_backoff > policy.max_backoff
            || policy.max_elapsed.is_zero()
        {
            return Err(SamError::Rejected("invalid reconnect policy bounds".into()));
        }
        let started = tokio::time::Instant::now();
        let mut backoff = policy.initial_backoff;
        let mut last_error = None;
        for attempt in 0..policy.max_attempts {
            let remaining = policy.max_elapsed.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            let result = timeout(remaining, Self::connect(config.clone()))
                .await
                .map_err(|_| SamError::Timeout)
                .and_then(|x| x);
            match result {
                Ok(client) => return Ok(client),
                Err(error) => {
                    let retryable = classify_failure(&error) == FailureClass::TransportTransient;
                    last_error = Some(error);
                    if !retryable || attempt + 1 >= policy.max_attempts {
                        break;
                    }
                    let remaining = policy.max_elapsed.saturating_sub(started.elapsed());
                    if remaining.is_zero() {
                        break;
                    }
                    tokio::time::sleep(backoff.min(remaining)).await;
                    backoff = backoff.saturating_mul(2).min(policy.max_backoff);
                }
            }
        }
        Err(last_error.unwrap_or(SamError::Timeout))
    }

    pub async fn connect(config: ClientConfig) -> Result<Self, SamError> {
        if config.min_version > config.max_version
            || config.max_frame_bytes == 0
            || config.max_frame_bytes > i2pr_sam_proto::MAX_LINE_BYTES
            || config.max_datagram_bytes == 0
            || config.max_datagram_bytes > 65_507
            || (!config.datagram_forward.advertised_host.is_loopback()
                && config.datagram_forward.port == 0)
        {
            return Err(SamError::Rejected(
                "invalid client configuration bounds".into(),
            ));
        }
        let (control, version) = open_control(&config).await?;
        let capabilities = SamCapabilities {
            negotiated_version: Some(version),
            stream: Support::Supported,
            ..SamCapabilities::default()
        };
        Ok(Self {
            config,
            capabilities: Arc::new(RwLock::new(capabilities)),
            utility: Mutex::new(control),
        })
    }

    pub async fn generate_destination(
        &self,
        signature_type: Option<u16>,
    ) -> Result<(String, SecretDestination), SamError> {
        let mut cmd = String::from("DEST GENERATE");
        cmd.push_str(&format!(" SIGNATURE_TYPE={}", signature_type.unwrap_or(7)));
        cmd.push('\n');
        let reply = self
            .utility
            .lock()
            .await
            .command(
                &cmd,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.words.first().map(String::as_str) != Some("DEST")
            || reply.words.get(1).map(String::as_str) != Some("REPLY")
        {
            return Err(SamError::Rejected("unexpected DEST response".into()));
        }
        Ok((
            reply
                .field("PUB")
                .ok_or_else(|| SamError::Rejected("missing PUB".into()))?
                .to_owned(),
            SecretDestination(
                reply
                    .field("PRIV")
                    .ok_or_else(|| SamError::Rejected("missing PRIV".into()))?
                    .to_owned(),
            ),
        ))
    }

    pub async fn lookup(&self, name: &str) -> Result<String, SamError> {
        let reply = self
            .utility
            .lock()
            .await
            .command(
                &format!("NAMING LOOKUP NAME={}\n", quote(name)),
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") == Some("KEY_NOT_FOUND")
            || reply.field("RESULT") == Some("NAME_NOT_FOUND")
        {
            return Err(SamError::NameNotFound);
        }
        ensure_ok(&reply)?;
        reply
            .field("VALUE")
            .map(str::to_owned)
            .ok_or_else(|| SamError::Rejected("missing VALUE".into()))
    }

    pub async fn capabilities(&self) -> SamCapabilities {
        self.capabilities.read().await.clone()
    }

    pub async fn create_stream_session(
        &self,
        destination: &str,
        id: &str,
        options: &[(String, String)],
    ) -> Result<StreamSession, SamError> {
        validate_session_inputs(destination, id)?;
        let (mut control, _) = open_control(&self.config).await?;
        let mut command = format!(
            "SESSION CREATE STYLE=STREAM ID={} DESTINATION={}",
            quote(id),
            quote(destination)
        );
        let transient = destination == "TRANSIENT";
        if transient {
            command.push_str(" SIGNATURE_TYPE=7");
        }
        let mut reserved = vec!["STYLE", "ID", "DESTINATION"];
        if transient {
            reserved.push("SIGNATURE_TYPE");
        }
        append_options(&mut command, options, &reserved)?;
        command.push('\n');
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&reply)?;
        self.capabilities.write().await.stream = Support::Supported;
        Ok(StreamSession {
            config: self.config.clone(),
            destination: destination.to_owned(),
            id: id.to_owned(),
            operations: Arc::new(Semaphore::new(64)),
            control: Mutex::new(Some(control)),
        })
    }

    pub async fn create_shared_session(
        &self,
        destination: &str,
        id: &str,
        dialect: SharedDialect,
        options: &[(String, String)],
    ) -> Result<SharedSession, SamError> {
        validate_session_inputs(destination, id)?;
        let (mut control, _) = open_control(&self.config).await?;
        let style = match dialect {
            SharedDialect::Master => "MASTER",
            SharedDialect::Primary => "PRIMARY",
        };
        let mut command = format!(
            "SESSION CREATE STYLE={style} ID={} DESTINATION={}",
            quote(id),
            quote(destination)
        );
        if destination == "TRANSIENT" {
            command.push_str(" SIGNATURE_TYPE=7");
        }
        append_options(
            &mut command,
            options,
            &[
                "STYLE",
                "ID",
                "DESTINATION",
                "SIGNATURE_TYPE",
                "PORT",
                "HOST",
                "FROM_PORT",
                "TO_PORT",
                "PROTOCOL",
                "LISTEN_PORT",
                "LISTEN_PROTOCOL",
                "HEADER",
            ],
        )?;
        command.push('\n');
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&reply)?;
        let capabilities = self.capabilities.clone();
        {
            let mut observed = capabilities.write().await;
            match dialect {
                SharedDialect::Master => observed.shared_master = Support::Supported,
                SharedDialect::Primary => observed.shared_primary = Support::Supported,
            }
        }
        Ok(SharedSession {
            id: id.to_owned(),
            destination: destination.to_owned(),
            dialect,
            config: self.config.clone(),
            control: Mutex::new(Some(control)),
            children: Mutex::new(HashMap::new()),
            capabilities,
            live: Arc::new(AtomicBool::new(true)),
            closed_notify: Arc::new(Notify::new()),
            max_children: 64,
        })
    }

    pub async fn create_datagram_session(
        &self,
        destination: &str,
        id: &str,
        style: SessionStyle,
        options: &[(String, String)],
    ) -> Result<DatagramSession, SamError> {
        validate_session_inputs(destination, id)?;
        if !matches!(
            style,
            SessionStyle::Datagram
                | SessionStyle::Raw
                | SessionStyle::Datagram2
                | SessionStyle::Datagram3
        ) {
            return Err(SamError::Rejected(
                "datagram session requires DATAGRAM, RAW, DATAGRAM2, or DATAGRAM3".into(),
            ));
        }
        let socket = UdpSocket::bind(SocketAddr::new(
            self.config.datagram_forward.bind_ip,
            self.config.datagram_forward.port,
        ))
        .await?;
        let local_port = socket.local_addr()?.port();
        let (mut control, _) = open_control(&self.config).await?;
        let style_text = match style {
            SessionStyle::Datagram => "DATAGRAM",
            SessionStyle::Raw => "RAW",
            SessionStyle::Datagram2 => "DATAGRAM2",
            SessionStyle::Datagram3 => "DATAGRAM3",
            SessionStyle::Stream => unreachable!(),
        };
        let mut command = format!(
            "SESSION CREATE STYLE={style_text} ID={} DESTINATION={} PORT={local_port} HOST={}",
            quote(id),
            quote(destination),
            self.config.datagram_forward.advertised_host
        );
        if destination == "TRANSIENT" {
            command.push_str(" SIGNATURE_TYPE=7");
        }
        if style == SessionStyle::Raw {
            command.push_str(" HEADER=true");
        }
        let mut reserved = vec!["STYLE", "ID", "DESTINATION", "PORT", "HOST"];
        if destination == "TRANSIENT" {
            reserved.push("SIGNATURE_TYPE");
        }
        if style == SessionStyle::Raw {
            reserved.push("HEADER");
        }
        append_options(&mut command, options, &reserved)?;
        command.push('\n');
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&reply)?;
        match style {
            SessionStyle::Datagram => self.capabilities.write().await.datagram = Support::Supported,
            SessionStyle::Raw => self.capabilities.write().await.raw = Support::Supported,
            SessionStyle::Datagram2 => {
                self.capabilities.write().await.datagram2 = Support::Supported
            }
            SessionStyle::Datagram3 => {
                self.capabilities.write().await.datagram3 = Support::Supported
            }
            SessionStyle::Stream => {}
        }
        Ok(DatagramSession {
            id: id.to_owned(),
            style,
            config: self.config.clone(),
            control: Mutex::new(Some(control)),
            socket,
            closed: AtomicBool::new(false),
            closed_notify: Arc::new(Notify::new()),
        })
    }
}

pub struct DatagramSession {
    id: String,
    style: SessionStyle,
    config: ClientConfig,
    control: Mutex<Option<Control<TcpStream>>>,
    socket: UdpSocket,
    closed: AtomicBool,
    closed_notify: Arc<Notify>,
}

impl DatagramSession {
    pub fn style(&self) -> SessionStyle {
        self.style
    }

    pub async fn send(
        &self,
        destination: &str,
        payload: &[u8],
        from_port: Option<u16>,
        to_port: Option<u16>,
    ) -> Result<(), SamError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(SamError::Closed);
        }
        if destination.is_empty()
            || destination
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        {
            return Err(SamError::Rejected(
                "invalid datagram destination token".into(),
            ));
        }
        if payload.is_empty() || payload.len() > self.config.max_datagram_bytes {
            return Err(SamError::Rejected(
                "datagram payload outside configured bounds".into(),
            ));
        }
        let mut header = format!("3.0 {} {}", self.id, destination);
        if let Some(port) = from_port {
            header.push_str(&format!(" FROM_PORT={port}"));
        }
        if let Some(port) = to_port {
            header.push_str(&format!(" TO_PORT={port}"));
        }
        header.push('\n');
        let mut packet = Vec::with_capacity(header.len() + payload.len());
        packet.extend_from_slice(header.as_bytes());
        packet.extend_from_slice(payload);
        if packet.len() > 65_507 {
            return Err(SamError::Rejected(
                "SAM UDP frame exceeds IPv4 datagram limit".into(),
            ));
        }
        self.socket
            .send_to(&packet, self.config.datagram_endpoint)
            .await?;
        Ok(())
    }

    pub async fn recv(&self) -> Result<ReceivedDatagram, SamError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(SamError::Closed);
        }
        let mut packet = vec![0; 65_507];
        let closed = self.closed_notify.notified();
        if self.closed.load(Ordering::Acquire) {
            return Err(SamError::Closed);
        }
        let (size, _) = tokio::select! {
            _ = closed => return Err(SamError::Closed),
            result = self.socket.recv_from(&mut packet) => result?,
        };
        packet.truncate(size);
        if size > self.config.max_datagram_bytes + 1024 {
            return Err(SamError::Rejected(
                "received datagram exceeds configured bounds".into(),
            ));
        }
        decode_forwarded_datagram(self.style, &packet)
    }

    pub async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.closed_notify.notify_waiters();
        self.control.lock().await.take();
    }
}

fn decode_forwarded_datagram(
    style: SessionStyle,
    bytes: &[u8],
) -> Result<ReceivedDatagram, SamError> {
    if style == SessionStyle::Raw {
        let split = bytes
            .windows(2)
            .position(|w| w == b"\n\n")
            .ok_or_else(|| SamError::Rejected("invalid RAW forwarding header".into()))?;
        let header = std::str::from_utf8(&bytes[..split])
            .map_err(|_| SamError::Rejected("invalid RAW forwarding header".into()))?;
        let mut from_port = Port::new(0);
        let mut to_port = Port::new(0);
        let mut protocol = I2pProtocol::new(18).expect("default RAW protocol is valid");
        for word in header.split_ascii_whitespace() {
            if let Some(v) = word.strip_prefix("FROM_PORT=") {
                from_port = Port::new(
                    v.parse()
                        .map_err(|_| SamError::Rejected("invalid source port".into()))?,
                );
            } else if let Some(v) = word.strip_prefix("TO_PORT=") {
                to_port = Port::new(
                    v.parse()
                        .map_err(|_| SamError::Rejected("invalid destination port".into()))?,
                );
            } else if let Some(v) = word.strip_prefix("PROTOCOL=") {
                protocol = I2pProtocol::new(
                    v.parse()
                        .map_err(|_| SamError::Rejected("invalid I2P protocol".into()))?,
                )
                .map_err(SamError::Protocol)?;
            }
        }
        return Ok(ReceivedDatagram::Raw(RawDatagram {
            from_port,
            to_port,
            protocol,
            payload: bytes[split + 2..].to_vec(),
        }));
    }
    let split = bytes
        .iter()
        .position(|b| *b == b'\n')
        .ok_or_else(|| SamError::Rejected("invalid datagram forwarding header".into()))?;
    let text = std::str::from_utf8(&bytes[..split])
        .map_err(|_| SamError::Rejected("invalid datagram forwarding header".into()))?;
    let mut words = text.split_ascii_whitespace();
    let source = words
        .next()
        .ok_or_else(|| SamError::Rejected("missing datagram source".into()))?;
    let mut from_port = Port::new(0);
    let mut to_port = Port::new(0);
    for word in words {
        if let Some(v) = word.strip_prefix("FROM_PORT=") {
            from_port = Port::new(
                v.parse()
                    .map_err(|_| SamError::Rejected("invalid source port".into()))?,
            );
        } else if let Some(v) = word.strip_prefix("TO_PORT=") {
            to_port = Port::new(
                v.parse()
                    .map_err(|_| SamError::Rejected("invalid destination port".into()))?,
            );
        }
    }
    let payload = bytes[split + 1..].to_vec();
    if style == SessionStyle::Datagram3 {
        let hash: [u8; 32] = BASE64
            .decode(source)
            .map_err(|_| SamError::Rejected("invalid DATAGRAM3 source hash".into()))?
            .try_into()
            .map_err(|_| SamError::Rejected("invalid DATAGRAM3 source hash length".into()))?;
        Ok(ReceivedDatagram::Unverified(UnverifiedDatagram3 {
            source_hash: UnverifiedSourceHash::new(hash),
            from_port,
            to_port,
            payload,
        }))
    } else {
        Ok(ReceivedDatagram::Authenticated(AuthenticatedDatagram {
            source: Destination::new(source).map_err(SamError::Protocol)?,
            from_port,
            to_port,
            payload,
        }))
    }
}

fn append_options(
    command: &mut String,
    options: &[(String, String)],
    reserved: &[&str],
) -> Result<(), SamError> {
    if options.len() > 64 {
        return Err(SamError::Rejected("too many session options".into()));
    }
    let mut seen = HashSet::new();
    for (key, value) in options {
        if key.is_empty()
            || key
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && b != b'.' && b != b'_')
            || reserved.contains(&key.as_str())
            || !seen.insert(key)
        {
            return Err(SamError::Rejected(
                "invalid, duplicate, or reserved session option".into(),
            ));
        }
        command.push_str(&format!(" {key}={}", quote(value)));
    }
    Ok(())
}

fn validate_session_inputs(destination: &str, id: &str) -> Result<(), SamError> {
    if destination.is_empty() || destination.len() > i2pr_sam_proto::MAX_DESTINATION_BYTES {
        return Err(SamError::Rejected(
            "Destination text outside configured bounds".into(),
        ));
    }
    SessionId::new(id).map_err(SamError::Protocol)?;
    Ok(())
}

fn validate_child_options(
    style: SessionStyle,
    options: &[(String, String)],
) -> Result<(), SamError> {
    let has = |key: &str| options.iter().any(|(name, _)| name == key);
    if style == SessionStyle::Stream && has("PORT") {
        return Err(SamError::Rejected(
            "STREAM child cannot configure a datagram PORT".into(),
        ));
    }
    if style != SessionStyle::Raw && (has("PROTOCOL") || has("LISTEN_PROTOCOL") || has("HEADER")) {
        return Err(SamError::Rejected(
            "RAW-only child option used with another style".into(),
        ));
    }
    if style == SessionStyle::Raw {
        for key in ["PROTOCOL", "LISTEN_PROTOCOL"] {
            if let Some(value) = options
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
            {
                let protocol: u8 = value
                    .parse()
                    .map_err(|_| SamError::Rejected("invalid RAW protocol".into()))?;
                I2pProtocol::new(protocol).map_err(SamError::Protocol)?;
            }
        }
    }
    Ok(())
}

fn listener_tuple(
    style: SessionStyle,
    options: &[(String, String)],
) -> Result<(u16, u8), SamError> {
    let value = |key: &str| {
        options
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, v)| v.as_str())
    };
    let port = value("LISTEN_PORT")
        .or_else(|| value("FROM_PORT"))
        .unwrap_or("0")
        .parse()
        .map_err(|_| SamError::Rejected("invalid listener port".into()))?;
    let protocol = if style == SessionStyle::Stream {
        6
    } else if style == SessionStyle::Raw {
        value("LISTEN_PROTOCOL")
            .or_else(|| value("PROTOCOL"))
            .unwrap_or("18")
            .parse()
            .map_err(|_| SamError::Rejected("invalid listener protocol".into()))?
    } else {
        17
    };
    if style == SessionStyle::Stream
        && value("LISTEN_PORT").is_some()
        && value("FROM_PORT") != value("LISTEN_PORT")
        && value("LISTEN_PORT") != Some("0")
    {
        return Err(SamError::Rejected(
            "STREAM LISTEN_PORT must match FROM_PORT or be zero".into(),
        ));
    }
    Ok((port, protocol))
}

pub struct SharedSession {
    id: String,
    destination: String,
    dialect: SharedDialect,
    config: ClientConfig,
    control: Mutex<Option<Control<TcpStream>>>,
    children: Mutex<HashMap<String, ChildEntry>>,
    capabilities: Arc<RwLock<SamCapabilities>>,
    live: Arc<AtomicBool>,
    closed_notify: Arc<Notify>,
    max_children: usize,
}

struct ChildEntry {
    listener: (u16, u8),
    live: Arc<AtomicBool>,
    closed_notify: Arc<Notify>,
}

impl SharedSession {
    pub fn destination(&self) -> &str {
        &self.destination
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn dialect(&self) -> SharedDialect {
        self.dialect
    }

    pub async fn add_child(
        &self,
        id: &str,
        style: SessionStyle,
        options: &[(String, String)],
    ) -> Result<SharedChild, SamError> {
        SessionId::new(id).map_err(SamError::Protocol)?;
        if !self.live.load(Ordering::Acquire) {
            return Err(SamError::Closed);
        }
        validate_child_options(style, options)?;
        let mut children = self.children.lock().await;
        if children.len() >= self.max_children || children.contains_key(id) {
            return Err(SamError::Rejected(
                "duplicate child ID or child limit reached".into(),
            ));
        }
        let style_text = match style {
            SessionStyle::Stream => "STREAM",
            SessionStyle::Datagram => "DATAGRAM",
            SessionStyle::Raw => "RAW",
            SessionStyle::Datagram2 => "DATAGRAM2",
            SessionStyle::Datagram3 => "DATAGRAM3",
        };
        let mut wire_options = options.to_vec();
        let mut datagram_socket = None;
        let mut local_forwarding = false;
        if matches!(
            style,
            SessionStyle::Datagram
                | SessionStyle::Raw
                | SessionStyle::Datagram2
                | SessionStyle::Datagram3
        ) {
            let host = wire_options
                .iter()
                .find(|(key, _)| key == "HOST")
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| "127.0.0.1".into());
            if !wire_options.iter().any(|(key, _)| key == "HOST") {
                wire_options.push(("HOST".into(), host.clone()));
            }
            if host == "127.0.0.1" || host == "localhost" {
                let requested_port = wire_options
                    .iter()
                    .find(|(key, _)| key == "PORT")
                    .map(|(_, value)| value.parse::<u16>())
                    .transpose()
                    .map_err(|_| SamError::Rejected("invalid shared UDP forwarding port".into()))?;
                let bind =
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), requested_port.unwrap_or(0));
                let socket = UdpSocket::bind(bind).await?;
                let actual_port = socket.local_addr()?.port();
                if !wire_options.iter().any(|(key, _)| key == "PORT") {
                    wire_options.push(("PORT".into(), actual_port.to_string()));
                }
                datagram_socket = Some(socket);
                local_forwarding = true;
            } else if !wire_options.iter().any(|(key, _)| key == "PORT") {
                return Err(SamError::Rejected(
                    "remote UDP forwarding requires explicit PORT".into(),
                ));
            } else {
                datagram_socket = Some(
                    UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).await?,
                );
            }
            if style == SessionStyle::Raw && !wire_options.iter().any(|(key, _)| key == "HEADER") {
                wire_options.push(("HEADER".into(), "true".into()));
            }
        }
        let listener = listener_tuple(style, &wire_options)?;
        if children
            .values()
            .any(|existing| existing.listener == listener)
        {
            return Err(SamError::Rejected(
                "duplicate shared-session listener tuple".into(),
            ));
        }
        let mut command = format!("SESSION ADD STYLE={style_text} ID={}", quote(id));
        append_options(&mut command, &wire_options, &["STYLE", "ID", "DESTINATION"])?;
        command.push('\n');
        let mut guard = self.control.lock().await;
        let control = guard.as_mut().ok_or(SamError::Closed)?;
        let result = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&result)?;
        let child_live = Arc::new(AtomicBool::new(true));
        let child_closed_notify = Arc::new(Notify::new());
        children.insert(
            id.to_owned(),
            ChildEntry {
                listener,
                live: child_live.clone(),
                closed_notify: child_closed_notify.clone(),
            },
        );
        {
            let mut observed = self.capabilities.write().await;
            match style_text {
                "STREAM" => observed.stream = Support::Supported,
                "DATAGRAM" => observed.datagram = Support::Supported,
                "RAW" => observed.raw = Support::Supported,
                "DATAGRAM2" => observed.datagram2 = Support::Supported,
                "DATAGRAM3" => observed.datagram3 = Support::Supported,
                _ => {}
            }
        }
        Ok(SharedChild {
            id: id.to_owned(),
            style,
            style_text: style_text.to_owned(),
            destination: self.destination.clone(),
            live: Arc::downgrade(&self.live),
            child_live,
            config: self.config.clone(),
            datagram_socket,
            local_forwarding,
            owner_closed: self.closed_notify.clone(),
            closed_notify: child_closed_notify,
            closed: AtomicBool::new(false),
            operations: Arc::new(Semaphore::new(32)),
        })
    }

    pub async fn remove_child(&self, id: &str) -> Result<(), SamError> {
        if !self.live.load(Ordering::Acquire) {
            return Err(SamError::Closed);
        }
        let mut children = self.children.lock().await;
        if !children.contains_key(id) {
            return Ok(());
        }
        let mut guard = self.control.lock().await;
        let control = guard.as_mut().ok_or(SamError::Closed)?;
        let result = control
            .command(
                &format!("SESSION REMOVE ID={}\n", quote(id)),
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&result)?;
        if let Some(child) = children.remove(id) {
            child.live.store(false, Ordering::Release);
            child.closed_notify.notify_waiters();
        }
        Ok(())
    }

    pub async fn close(&self) {
        self.live.store(false, Ordering::Release);
        self.closed_notify.notify_waiters();
        let mut children = self.children.lock().await;
        for child in children.values() {
            child.live.store(false, Ordering::Release);
            child.closed_notify.notify_waiters();
        }
        children.clear();
        self.control.lock().await.take();
    }
}

pub struct SharedChild {
    id: String,
    style: SessionStyle,
    style_text: String,
    destination: String,
    live: std::sync::Weak<AtomicBool>,
    child_live: Arc<AtomicBool>,
    config: ClientConfig,
    datagram_socket: Option<UdpSocket>,
    local_forwarding: bool,
    owner_closed: Arc<Notify>,
    closed_notify: Arc<Notify>,
    closed: AtomicBool,
    operations: Arc<Semaphore>,
}
impl SharedChild {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn style(&self) -> &str {
        &self.style_text
    }
    pub fn destination(&self) -> &str {
        &self.destination
    }
    pub fn is_open(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
            && self.child_live.load(Ordering::Acquire)
            && self
                .live
                .upgrade()
                .is_some_and(|live| live.load(Ordering::Acquire))
    }

    pub async fn connect(
        &self,
        destination: &str,
        from_port: Option<u16>,
        to_port: Option<u16>,
    ) -> Result<SamStream, SamError> {
        if self.style != SessionStyle::Stream || !self.is_open() {
            return Err(SamError::Closed);
        }
        let permit = self
            .operations
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SamError::Closed)?;
        let (mut control, _) = open_control(&self.config).await?;
        let mut command = format!(
            "STREAM CONNECT ID={} DESTINATION={}",
            quote(&self.id),
            quote(destination)
        );
        if let Some(port) = from_port {
            command.push_str(&format!(" FROM_PORT={port}"));
        }
        if let Some(port) = to_port {
            command.push_str(&format!(" TO_PORT={port}"));
        }
        command.push('\n');
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&reply)?;
        Ok(SamStream {
            inner: control.reader,
            remote_destination: None,
            owner_live: Some(self.live.clone()),
            _permit: permit,
        })
    }

    pub async fn accept(&self) -> Result<SamStream, SamError> {
        if self.style != SessionStyle::Stream || !self.is_open() {
            return Err(SamError::Closed);
        }
        let permit = self
            .operations
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SamError::Closed)?;
        let (mut control, _) = open_control(&self.config).await?;
        let reply = control
            .command(
                &format!("STREAM ACCEPT ID={}\n", quote(&self.id)),
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&reply)?;
        Ok(SamStream {
            inner: control.reader,
            remote_destination: reply.field("DESTINATION").map(str::to_owned),
            owner_live: Some(self.live.clone()),
            _permit: permit,
        })
    }

    pub async fn send_datagram(
        &self,
        destination: &str,
        payload: &[u8],
        from_port: Option<u16>,
        to_port: Option<u16>,
    ) -> Result<(), SamError> {
        if !matches!(
            self.style,
            SessionStyle::Datagram
                | SessionStyle::Raw
                | SessionStyle::Datagram2
                | SessionStyle::Datagram3
        ) || !self.is_open()
        {
            return Err(SamError::Closed);
        }
        if destination.is_empty()
            || destination
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
            || payload.is_empty()
            || payload.len() > self.config.max_datagram_bytes
        {
            return Err(SamError::Rejected(
                "datagram arguments outside configured bounds".into(),
            ));
        }
        let mut header = format!("3.0 {} {}", self.id, destination);
        if let Some(port) = from_port {
            header.push_str(&format!(" FROM_PORT={port}"));
        }
        if let Some(port) = to_port {
            header.push_str(&format!(" TO_PORT={port}"));
        }
        header.push('\n');
        let mut packet = Vec::with_capacity(header.len() + payload.len());
        packet.extend_from_slice(header.as_bytes());
        packet.extend_from_slice(payload);
        self.datagram_socket
            .as_ref()
            .ok_or_else(|| {
                SamError::Rejected("remote forwarding child has no local send socket".into())
            })?
            .send_to(&packet, self.config.datagram_endpoint)
            .await?;
        Ok(())
    }

    pub async fn recv_datagram(&self) -> Result<ReceivedDatagram, SamError> {
        if !self.is_open() {
            return Err(SamError::Closed);
        }
        if !self.local_forwarding {
            return Err(SamError::Rejected(
                "remote bridge forwarding is received by the configured host".into(),
            ));
        }
        let socket = self.datagram_socket.as_ref().ok_or_else(|| {
            SamError::Rejected("remote forwarding child has no local receive socket".into())
        })?;
        let mut packet = vec![0; 65_507];
        let owner_closed = self.owner_closed.notified();
        let local_closed = self.closed_notify.notified();
        if !self.is_open() {
            return Err(SamError::Closed);
        }
        let (size, _) = tokio::select! { _ = owner_closed => return Err(SamError::Closed), _ = local_closed => return Err(SamError::Closed), result = socket.recv_from(&mut packet) => result? };
        packet.truncate(size);
        decode_forwarded_datagram(self.style, &packet)
    }

    pub async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.closed_notify.notify_waiters();
        self.operations.close();
    }
}

async fn open_control(config: &ClientConfig) -> Result<(Control<TcpStream>, SamVersion), SamError> {
    let stream = timeout(config.connect_timeout, TcpStream::connect(config.endpoint))
        .await
        .map_err(|_| SamError::Timeout)??;
    let mut control = Control {
        reader: BufReader::new(stream),
    };
    let auth = config
        .credentials
        .as_ref()
        .map(|c| {
            format!(
                " USER={} PASSWORD={}",
                quote(&c.username),
                quote(&c.password)
            )
        })
        .unwrap_or_default();
    let hello = format!(
        "HELLO VERSION MIN={} MAX={}{}\n",
        version_text(config.min_version),
        version_text(config.max_version),
        auth
    );
    let reply = control
        .command(&hello, config.control_timeout, config.max_frame_bytes)
        .await?;
    if reply.words.first().map(String::as_str) != Some("HELLO")
        || reply.words.get(1).map(String::as_str) != Some("REPLY")
    {
        return Err(SamError::Rejected("unexpected HELLO response".into()));
    }
    if reply.field("RESULT") != Some("OK") {
        return Err(SamError::Rejected(
            reply.field("RESULT").unwrap_or("unknown").to_owned(),
        ));
    }
    let (major, minor) = reply
        .field("VERSION")
        .and_then(|s| s.split_once('.'))
        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        .or_else(|| {
            Some((
                reply.field("MAJOR")?.parse().ok()?,
                reply.field("MINOR")?.parse().ok()?,
            ))
        })
        .ok_or_else(|| SamError::Rejected("HELLO omitted a valid version".into()))?;
    Ok((control, SamVersion { major, minor }))
}

fn version_text(version: SamVersion) -> String {
    format!("{}.{}", version.major, version.minor)
}

fn ensure_ok(reply: &Line) -> Result<(), SamError> {
    if reply.field("RESULT") == Some("OK") {
        Ok(())
    } else {
        Err(SamError::Rejected(
            reply.field("RESULT").unwrap_or("unknown").to_owned(),
        ))
    }
}
fn quote(value: &str) -> String {
    if value
        .bytes()
        .any(|b| b.is_ascii_whitespace() || b == b'"' || b == b'\\')
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

pub struct SecretDestination(String);
impl SecretDestination {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretDestination([REDACTED])")
    }
}

pub struct StreamSession {
    config: ClientConfig,
    destination: String,
    id: String,
    operations: Arc<Semaphore>,
    control: Mutex<Option<Control<TcpStream>>>,
}
impl StreamSession {
    pub fn destination(&self) -> &str {
        &self.destination
    }
    pub async fn connect(
        &self,
        destination: &str,
        from_port: Option<u16>,
        to_port: Option<u16>,
    ) -> Result<SamStream, SamError> {
        let permit = self
            .operations
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SamError::Closed)?;
        let (mut control, _) = open_control(&self.config).await?;
        let mut cmd = format!(
            "STREAM CONNECT ID={} DESTINATION={}",
            quote(&self.id),
            quote(destination)
        );
        if let Some(p) = from_port {
            cmd.push_str(&format!(" FROM_PORT={p}"));
        }
        if let Some(p) = to_port {
            cmd.push_str(&format!(" TO_PORT={p}"));
        }
        cmd.push('\n');
        let reply = control
            .command(
                &cmd,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&reply)?;
        Ok(SamStream {
            inner: control.reader,
            remote_destination: None,
            owner_live: None,
            _permit: permit,
        })
    }
    pub async fn accept(&self) -> Result<SamStream, SamError> {
        let permit = self
            .operations
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SamError::Closed)?;
        let (mut control, _) = open_control(&self.config).await?;
        let reply = control
            .command(
                &format!("STREAM ACCEPT ID={}\n", quote(&self.id)),
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        ensure_ok(&reply)?;
        let remote_destination = reply.field("DESTINATION").map(str::to_owned);
        Ok(SamStream {
            inner: control.reader,
            remote_destination,
            owner_live: None,
            _permit: permit,
        })
    }
    pub async fn close(&self) {
        self.operations.close();
        self.control.lock().await.take();
    }
}

pub struct SamStream {
    inner: BufReader<TcpStream>,
    remote_destination: Option<String>,
    owner_live: Option<std::sync::Weak<AtomicBool>>,
    _permit: OwnedSemaphorePermit,
}
impl SamStream {
    pub fn remote_destination(&self) -> Option<&str> {
        self.remote_destination.as_deref()
    }
}
impl AsyncRead for SamStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.owner_live.as_ref().is_some_and(|live| {
            !live
                .upgrade()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
        }) {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "shared session owner closed",
            )));
        }
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl AsyncWrite for SamStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if self.owner_live.as_ref().is_some_and(|live| {
            !live
                .upgrade()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
        }) {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "shared session owner closed",
            )));
        }
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod datagram_tests {
    use super::*;

    #[test]
    fn source_identity_is_style_specific() {
        let authenticated = decode_forwarded_datagram(
            SessionStyle::Datagram,
            b"base64-destination FROM_PORT=3 TO_PORT=4\npayload",
        )
        .unwrap();
        assert!(matches!(authenticated, ReceivedDatagram::Authenticated(_)));
        let unverified = decode_forwarded_datagram(
            SessionStyle::Datagram3,
            b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\npayload",
        )
        .unwrap();
        assert!(matches!(unverified, ReceivedDatagram::Unverified(_)));
        let raw = decode_forwarded_datagram(
            SessionStyle::Raw,
            b"FROM_PORT=1\nTO_PORT=2\nPROTOCOL=18\n\npayload",
        )
        .unwrap();
        assert!(matches!(raw, ReceivedDatagram::Raw(_)));
        assert!(
            decode_forwarded_datagram(SessionStyle::Datagram3, b"not-a-hash\npayload").is_err()
        );
    }

    #[test]
    fn credentials_debug_is_redacted() {
        let credentials = Credentials {
            username: "private-user".into(),
            password: "private-password".into(),
        };
        let display = format!("{credentials:?}");
        assert!(!display.contains("private-user"));
        assert!(!display.contains("private-password"));
    }
}
