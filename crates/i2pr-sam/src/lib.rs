//! Async SAM client. Protocol framing and capability types are re-exported from the
//! runtime-neutral `i2pr-sam-proto` crate.
//!
//! Three protocol facts drive this module's shape:
//!
//! * capability is not version. A negotiated `3.3` never implies an optional feature works;
//! * identity is concrete. A `TRANSIENT` request token is not proof of a shared linkability
//!   domain, so sessions resolve a real Destination;
//! * transports are not interchangeable. Ordinary DATAGRAM1/RAW may use UDP forwarding or
//!   the v1/v2-compatible control socket, and DATAGRAM2/3 may use neither v1/v2 path.

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
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    net::{TcpStream, UdpSocket},
    sync::{Mutex, Notify, OwnedSemaphorePermit, RwLock, Semaphore},
    time::timeout,
};

mod control_socket;
mod datagram;
mod identity;
mod resource;

use resource::ReleaseOnDrop;
pub use resource::{ResourceUsage, resource_usage};

pub use control_socket::{ControlDatagramLink, DatagramInbox, StreamPeer};
pub use datagram::{DatagramTransport, ForwardedMetadata};
pub use i2pr_sam_proto as proto;
pub use i2pr_sam_proto::{
    AuthenticatedDatagram, Destination, DestinationHash, ReceivedDatagram, SamCapabilities,
    SecretDestination, SessionId, SessionStyle, SharedDialect, UnverifiedDatagram3,
    UnverifiedSourceHash,
};
use i2pr_sam_proto::{I2pProtocol, Line, Port, SamVersion, Support, parse_line};
pub use identity::{
    GeneratedDestination, IdentityFailure, SessionDestination, SessionIdentity, destination_hash,
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
    /// Bounded queue depth for control-socket datagram deliveries.
    pub max_inbox_datagrams: usize,
    /// Bounded queue size in bytes for control-socket datagram deliveries.
    pub max_inbox_bytes: usize,
    /// Datagram port offered to the bridge in the forwarded datagram header.
    pub datagram_frame_version: SamVersion,
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
            // SAM SESSION CREATE may wait for tunnel construction, which can take a
            // minute or longer during startup or network congestion.
            control_timeout: Duration::from_secs(120),
            max_frame_bytes: i2pr_sam_proto::MAX_LINE_BYTES,
            max_datagram_bytes: 32_768,
            max_inbox_datagrams: 64,
            max_inbox_bytes: 256 * 1024,
            datagram_frame_version: SamVersion::V3_0,
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
    #[error("SAM operation is not supported for this session style: {0}")]
    Unsupported(String),
    #[error("name was not found by the SAM bridge")]
    NameNotFound,
    #[error("session identity could not be resolved to a concrete Destination")]
    IdentityUnavailable,
    #[error("client or session is closed")]
    Closed,
    #[error("connect retry admission budget is saturated")]
    RetryAdmissionSaturated,
}

/// Typed session option set.
///
/// Options used to be an untyped `&[(String, String)]`, which let a caller smuggle a
/// reserved key such as `DESTINATION` or `STYLE` into a session command. This type keeps
/// the same entries but validates key shape at construction and deduplicates on insert.
/// `Deref` to a slice of pairs keeps existing call sites compiling while callers migrate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionOptions {
    entries: Vec<(String, String)>,
}

impl SessionOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builder() -> SessionOptionsBuilder {
        SessionOptionsBuilder::default()
    }

    /// Entries in insertion order; duplicates are removed, last write wins.
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn with(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, SamError> {
        self.set(key.into(), value.into())?;
        Ok(self)
    }

    pub fn set(&mut self, key: String, value: String) -> Result<(), SamError> {
        if !valid_option_key(&key) {
            return Err(SamError::Rejected(format!(
                "invalid session option key {key:?}"
            )));
        }
        if value.bytes().any(|b| b.is_ascii_control()) {
            return Err(SamError::Rejected(format!(
                "session option {key:?} value contains a control character"
            )));
        }
        match self.entries.iter_mut().find(|(name, _)| *name == key) {
            Some(entry) => entry.1 = value,
            None => {
                if self.entries.len() >= MAX_SESSION_OPTIONS {
                    return Err(SamError::Rejected("too many session options".into()));
                }
                self.entries.push((key, value));
            }
        }
        Ok(())
    }
}

impl std::ops::Deref for SessionOptions {
    type Target = [(String, String)];

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl From<&[(String, String)]> for SessionOptions {
    fn from(entries: &[(String, String)]) -> Self {
        let mut options = Self {
            entries: Vec::with_capacity(entries.len()),
        };
        for (key, value) in entries {
            let _ = options.set(key.clone(), value.clone());
        }
        options
    }
}

impl From<Vec<(String, String)>> for SessionOptions {
    fn from(entries: Vec<(String, String)>) -> Self {
        Self::from(entries.as_slice())
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionOptionsBuilder {
    entries: Vec<(String, String)>,
}

impl SessionOptionsBuilder {
    pub fn option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries.push((key.into(), value.into()));
        self
    }

    /// Build the option set, rejecting keys that could collide with reserved session fields.
    pub fn build(self, reserved: &[&str]) -> Result<SessionOptions, SamError> {
        let mut options = SessionOptions::default();
        for (key, value) in self.entries {
            if reserved.contains(&key.as_str()) {
                return Err(SamError::Rejected(format!(
                    "session option {key:?} collides with a reserved session field"
                )));
            }
            options.set(key, value)?;
        }
        Ok(options)
    }
}

const MAX_SESSION_OPTIONS: usize = 64;

fn valid_option_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
}

/// What a resolved name actually denotes.
///
/// A SAM lookup can legitimately return either a full Destination or a base32 hash
/// address. Collapsing both into an opaque string is how a client ends up sending a hash
/// where a Destination is required, so the two cases are modelled separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerTarget {
    /// A full public Destination, usable as a session destination.
    Destination(Destination),
    /// A base32 hash address, which is a valid peer name but not a Destination.
    Base32Hash { name: String, value: String },
}

impl PeerTarget {
    pub fn classify(name: &str, value: &str) -> Result<Self, SamError> {
        let looks_like_base32_hash = value.ends_with(".b32.i2p")
            || (!value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.'));
        if looks_like_base32_hash {
            Ok(Self::Base32Hash {
                name: name.to_owned(),
                value: value.to_owned(),
            })
        } else {
            validate_destination_token(value)?;
            Ok(Self::Destination(
                Destination::new(value).map_err(SamError::Protocol)?,
            ))
        }
    }

    pub fn destination(&self) -> Option<&Destination> {
        match self {
            Self::Destination(destination) => Some(destination),
            Self::Base32Hash { .. } => None,
        }
    }

    /// The Destination, or a typed refusal explaining why a hash is not one.
    pub fn require_destination(&self) -> Result<&Destination, SamError> {
        match self {
            Self::Destination(destination) => Ok(destination),
            Self::Base32Hash { value, .. } => Err(SamError::Rejected(format!(
                "{value} is a base32 hash address, not a Destination; it can be used as a \\
                 lookup name or `NAMING LOOKUP NAME=ME` value but not as session key material"
            ))),
        }
    }

    pub fn wire_value(&self) -> &str {
        match self {
            Self::Destination(destination) => destination.as_str(),
            Self::Base32Hash { value, .. } => value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    TransportTransient,
    ProtocolPermanent,
    CapabilityUnsupported,
    ConfigurationPermanent,
    RouterTransient,
    CancelledOrClosed,
    ResourceExhausted,
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
        SamError::RetryAdmissionSaturated => FailureClass::ResourceExhausted,
        SamError::Unsupported(_) => FailureClass::CapabilityUnsupported,
        SamError::Rejected(_) => FailureClass::ConfigurationPermanent,
        SamError::Protocol(_) => FailureClass::ProtocolPermanent,
        SamError::Io(_) => FailureClass::ConfigurationPermanent,
        SamError::NameNotFound | SamError::IdentityUnavailable => FailureClass::RouterTransient,
    }
}

/// Reject SAM destination tokens that cannot appear safely in a command line.
///
/// A destination is a single protocol token. Any ASCII control character - including a
/// newline - would let a caller-supplied value terminate one command and begin another on
/// the same control socket, so such tokens are refused before framing rather than quoted.
fn validate_destination_token(value: &str) -> Result<&str, SamError> {
    // A command line must still fit inside the frame ceiling once quoting is added, so the
    // usable token length is a fraction of the raw destination field limit.
    if value.is_empty() || value.len() > i2pr_sam_proto::MAX_DESTINATION_BYTES / 4 {
        return Err(SamError::Rejected(
            "destination outside configured bounds".into(),
        ));
    }
    if value
        .bytes()
        .any(|b| b.is_ascii_control() || b == b'"' || b == b'\\')
    {
        return Err(SamError::Rejected(
            "destination contains characters that cannot be framed safely".into(),
        ));
    }
    Ok(value)
}

pub struct SamClient {
    config: ClientConfig,
    capabilities: Arc<RwLock<SamCapabilities>>,
    utility: Mutex<Control<TcpStream>>,
}

struct Control<S> {
    reader: BufReader<S>,
    /// Keeps the live-socket counter honest for every exit path, including cancellation.
    _lifetime: ReleaseOnDrop,
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
        let line = timeout(
            deadline,
            control_socket::read_line_bounded(&mut self.reader, max_frame),
        )
        .await
        .map_err(|_| SamError::Timeout)??;
        if line.is_empty() {
            return Err(SamError::Closed);
        }
        Ok(parse_line(&line)?)
    }
}

/// Extract a concrete session identity using the specification's own lookup path.
///
/// `NAMING LOOKUP NAME=ME` returns the Destination the router bound to this socket's
/// session, which is the only documented way to name a router-generated identity without
/// keeping private key material.
async fn resolve_identity(
    control: &mut Control<TcpStream>,
    config: &ClientConfig,
) -> Result<SessionIdentity, IdentityFailure> {
    let reply = control
        .command(
            "NAMING LOOKUP NAME=ME\n",
            config.control_timeout,
            config.max_frame_bytes,
        )
        .await
        .map_err(|_| IdentityFailure::LookupFailed)?;
    let value = reply.field("VALUE").ok_or(IdentityFailure::LookupFailed)?;
    SessionIdentity::parse(value).map_err(|_| IdentityFailure::Malformed)
}

impl SamClient {
    /// Connect with the default wall-clock retry time source.
    pub async fn connect_with_policy(
        config: ClientConfig,
        policy: ConnectRetryPolicy,
    ) -> Result<Self, SamError> {
        Self::connect_with_clock(config, policy, &TokioRetryClock::start()).await
    }

    /// Connect with an injected time source, so retry budgets are deterministic in tests.
    pub async fn connect_with_clock(
        config: ClientConfig,
        policy: ConnectRetryPolicy,
        clock: &dyn RetryClock,
    ) -> Result<Self, SamError> {
        if policy.max_attempts == 0
            || policy.initial_backoff > policy.max_backoff
            || policy.max_elapsed.is_zero()
            || policy.max_concurrent_admissions == 0
        {
            return Err(SamError::Rejected(
                "invalid connect retry policy bounds".into(),
            ));
        }
        // A single attempt never retries, so it never competes for the shared budget.
        let _admission = match policy.max_attempts {
            1 => None,
            _ => Some(acquire_retry_admission(&policy, clock).await?),
        };
        let mut last_error = None;
        for attempt in 0..policy.max_attempts {
            let remaining = remaining_budget(&policy, clock);
            if remaining.is_zero() {
                break;
            }
            let result = match timeout(remaining, Self::connect(config.clone())).await {
                Ok(result) => result,
                Err(_) => Err(SamError::Timeout),
            };
            match result {
                Ok(client) => return Ok(client),
                Err(error) => {
                    let retryable = classify_failure(&error) == FailureClass::TransportTransient;
                    last_error = Some(error);
                    if !retryable || attempt + 1 >= policy.max_attempts {
                        break;
                    }
                    let remaining = remaining_budget(&policy, clock);
                    if remaining.is_zero() {
                        break;
                    }
                    clock
                        .sleep(policy.backoff_before(attempt + 1).min(remaining))
                        .await;
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
            || config.max_inbox_datagrams == 0
            || config.max_inbox_bytes < config.max_datagram_bytes
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
            // Negotiation records the version and nothing else. Every capability stays
            // Unknown until an operation actually succeeds, so a router advertising 3.3
            // cannot make this client claim a feature the router does not implement.
            ..SamCapabilities::default()
        };
        Ok(Self {
            config,
            capabilities: Arc::new(RwLock::new(capabilities)),
            utility: Mutex::new(control),
        })
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Create an ordinary datagram session that sends through the SAM datagram port and
    /// receives through a forwarded UDP socket.
    pub async fn create_datagram_session(
        &self,
        destination: &SessionDestination,
        id: &str,
        style: SessionStyle,
        options: &[(String, String)],
    ) -> Result<DatagramSession, SamError> {
        self.create_datagram_session_with(
            destination,
            id,
            style,
            DatagramTransport::UdpForward,
            options,
        )
        .await
    }

    pub async fn generate_destination(
        &self,
        signature_type: Option<u16>,
    ) -> Result<GeneratedDestination, SamError> {
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
        let public = Destination::new(
            reply
                .field("PUB")
                .ok_or_else(|| SamError::Rejected("missing PUB".into()))?,
        )
        .map_err(SamError::Protocol)?;
        let secret = SecretDestination::new(
            reply
                .field("PRIV")
                .ok_or_else(|| SamError::Rejected("missing PRIV".into()))?,
        )
        .map_err(SamError::Protocol)?;
        Ok(GeneratedDestination::new(public, secret))
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
            || reply.field("RESULT") == Some("LEASESET_NOT_FOUND")
        {
            return Err(SamError::NameNotFound);
        }
        ensure_ok(&reply)?;
        reply
            .field("VALUE")
            .map(str::to_owned)
            .ok_or_else(|| SamError::Rejected("missing VALUE".into()))
    }

    /// Typed lookup: the resolved value is a Destination, never an opaque string.
    pub async fn lookup_destination(&self, name: &str) -> Result<Destination, SamError> {
        let value = self.lookup(name).await?;
        validate_destination_token(&value)?;
        Destination::new(value).map_err(SamError::Protocol)
    }

    /// Resolve a name to a typed peer target, distinguishing Destination from hash address.
    pub async fn resolve_peer(&self, name: &str) -> Result<PeerTarget, SamError> {
        let value = self.lookup(name).await?;
        PeerTarget::classify(name, &value)
    }

    /// Resolve the Destination this utility connection's own session would use.
    pub async fn session_identity(&self) -> Result<SessionIdentity, SamError> {
        let mut utility = self.utility.lock().await;
        resolve_identity(&mut utility, &self.config)
            .await
            .map_err(|_| SamError::IdentityUnavailable)
    }

    pub async fn capabilities(&self) -> SamCapabilities {
        self.capabilities.read().await.clone()
    }

    async fn mark<T: Copy>(&self, slot: impl Fn(&mut SamCapabilities) -> &mut T, value: T) {
        let mut observed = self.capabilities.write().await;
        *slot(&mut observed) = value;
    }

    /// Record an unsupported verdict only when the router itself rejected the style.
    async fn observe_rejection(
        &self,
        reply: &Line,
        slot: impl Fn(&mut SamCapabilities) -> &mut Support,
    ) {
        if unsupported_style_reply(reply) {
            self.mark(slot, Support::Unsupported).await;
        }
    }

    pub async fn create_stream_session(
        &self,
        destination: &SessionDestination,
        id: &str,
        options: &[(String, String)],
    ) -> Result<StreamSession, SamError> {
        validate_session_inputs(destination, id)?;
        let (mut control, _) = open_control(&self.config).await?;
        let command = self.session_create_command(destination, None, id, "STREAM", &[], options)?;
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") != Some("OK") {
            self.observe_rejection(&reply, |c| &mut c.stream).await;
            return Err(rejection(&reply));
        }
        let identity = resolve_identity(&mut control, &self.config).await.ok();
        if identity.is_some() {
            self.mark(|c| &mut c.session_identity_lookup, Support::Supported)
                .await;
        }
        self.mark(|c| &mut c.stream, Support::Supported).await;
        resource::session_opened();
        Ok(StreamSession {
            config: self.config.clone(),
            destination: destination.clone(),
            identity,
            id: id.to_owned(),
            operations: Arc::new(Semaphore::new(64)),
            control: Mutex::new(Some(control)),
        })
    }

    /// Assemble a `SESSION CREATE` line.
    ///
    /// `structural` holds fields this client owns (forwarding `PORT`/`HOST`, RAW `HEADER`)
    /// so they cannot collide with, or be duplicated by, caller-supplied options.
    fn session_create_command(
        &self,
        destination: &SessionDestination,
        generated_secret: Option<&SecretDestination>,
        id: &str,
        style: &str,
        structural: &[(String, String)],
        options: &[(String, String)],
    ) -> Result<String, SamError> {
        let mut reserved: Vec<&str> = vec!["STYLE", "ID", "DESTINATION"];
        let generate = destination.is_transient() || generated_secret.is_some();
        if generate {
            reserved.push("SIGNATURE_TYPE");
        }
        let wire = match destination {
            SessionDestination::Transient => "TRANSIENT".to_owned(),
            SessionDestination::Generated => generated_secret
                .ok_or_else(|| {
                    SamError::Rejected(
                        "generated destination was not produced before session create".into(),
                    )
                })?
                .expose()
                .to_owned(),
            SessionDestination::Imported(public) => public.as_str().to_owned(),
            SessionDestination::WithKey { secret, .. } => secret.expose().to_owned(),
        };
        let mut command = format!(
            "SESSION CREATE STYLE={style} ID={} DESTINATION={}",
            quote(id),
            quote(&wire)
        );
        if generate {
            command.push_str(" SIGNATURE_TYPE=7");
        }
        for (key, value) in structural {
            reserved.push(key.as_str());
            command.push_str(&format!(" {key}={value}"));
        }
        append_options(&mut command, options, &reserved)?;
        command.push('\n');
        Ok(command)
    }

    pub async fn create_shared_session(
        &self,
        destination: &SessionDestination,
        id: &str,
        dialect: SharedDialect,
        options: &[(String, String)],
    ) -> Result<SharedSession, SamError> {
        validate_session_inputs(destination, id)?;
        destination.validate()?;
        let mut generated = Vec::new();
        if matches!(destination, SessionDestination::Generated) {
            let pair = self.generate_destination(Some(7)).await?;
            generated.push(pair.secret().clone());
        }
        let (mut control, _) = open_control(&self.config).await?;
        let style = match dialect {
            SharedDialect::Master => "MASTER",
            SharedDialect::Primary => "PRIMARY",
        };
        // A primary session must not carry datagram routing options: they belong to
        // subsessions, and accepting them here would silently misroute traffic.
        for reserved in [
            "PORT",
            "HOST",
            "FROM_PORT",
            "TO_PORT",
            "PROTOCOL",
            "LISTEN_PORT",
            "LISTEN_PROTOCOL",
            "HEADER",
        ] {
            if options.iter().any(|(key, _)| key == reserved) {
                return Err(SamError::Rejected(format!(
                    "{reserved} is not allowed on a shared session; configure it on a subsession"
                )));
            }
        }
        let command =
            self.session_create_command(destination, generated.first(), id, style, &[], options)?;
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") != Some("OK") {
            match dialect {
                SharedDialect::Master => {
                    self.observe_rejection(&reply, |c| &mut c.shared_master)
                        .await
                }
                SharedDialect::Primary => {
                    self.observe_rejection(&reply, |c| &mut c.shared_primary)
                        .await
                }
            }
            return Err(rejection(&reply));
        }
        // The router echoes the session's private Destination. It is retained only in memory
        // and never rendered by Debug, logged, or written to an artifact.
        let _router_reported_key = reply
            .field("DESTINATION")
            .map(|value| SecretDestination::new(value).ok());
        let identity = match resolve_identity(&mut control, &self.config).await {
            Ok(identity) => identity,
            Err(_) => {
                // A shared session without a concrete identity cannot prove one Destination
                // links every child, so this is a failure rather than a silent fallback.
                return Err(SamError::IdentityUnavailable);
            }
        };
        self.mark(|c| &mut c.session_identity_lookup, Support::Supported)
            .await;
        match dialect {
            SharedDialect::Master => {
                self.mark(|c| &mut c.shared_master, Support::Supported)
                    .await
            }
            SharedDialect::Primary => {
                self.mark(|c| &mut c.shared_primary, Support::Supported)
                    .await
            }
        }
        let version = self
            .capabilities
            .read()
            .await
            .negotiated_version
            .unwrap_or(SamVersion::V3_0);
        Ok(SharedSession {
            id: id.to_owned(),
            identity,
            dialect,
            config: self.config.clone(),
            control: Mutex::new(Some(control)),
            children: Mutex::new(HashMap::new()),
            capabilities: self.capabilities.clone(),
            live: Arc::new(AtomicBool::new(true)),
            closed_notify: Arc::new(Notify::new()),
            max_children: 64,
            negotiated_version: version,
            _lifetime: ReleaseOnDrop::new(resource::session_closed),
        })
    }

    pub async fn create_datagram_session_with(
        &self,
        destination: &SessionDestination,
        id: &str,
        style: SessionStyle,
        transport: DatagramTransport,
        options: &[(String, String)],
    ) -> Result<DatagramSession, SamError> {
        validate_session_inputs(destination, id)?;
        destination.validate()?;
        datagram::ensure_transport_supported(style, transport)?;
        if style == SessionStyle::Stream {
            return Err(SamError::Rejected(
                "datagram session requires DATAGRAM, RAW, DATAGRAM2, or DATAGRAM3".into(),
            ));
        }
        if option_bool(options, "HEADER")?.is_some() && style != SessionStyle::Raw {
            return Err(SamError::Rejected(
                "HEADER is only valid for STYLE=RAW".into(),
            ));
        }
        let negotiated = self
            .capabilities
            .read()
            .await
            .negotiated_version
            .unwrap_or(SamVersion::V3_0);
        // HEADER is a SAM 3.2 option. Requesting it below that version asks the router for
        // something it will ignore, and the decode side must then not expect the metadata.
        let header = option_bool(options, "HEADER")?.unwrap_or(major_minor(&negotiated) >= (3, 2));
        let protocol = match option_u8(options, "PROTOCOL")? {
            Some(value) => I2pProtocol::new(value).map_err(SamError::Protocol)?,
            None => I2pProtocol::new(18).expect("default RAW protocol is valid"),
        };
        if style != SessionStyle::Raw && option_u8(options, "PROTOCOL")?.is_some() {
            return Err(SamError::Rejected(
                "PROTOCOL is only valid for STYLE=RAW".into(),
            ));
        }

        let mut generated = Vec::new();
        if matches!(destination, SessionDestination::Generated) {
            let pair = self.generate_destination(Some(7)).await?;
            generated.push(pair.secret().clone());
        }

        let mut structural: Vec<(String, String)> = Vec::new();
        let mut udp_socket = None;
        if transport == DatagramTransport::UdpForward {
            let socket = UdpSocket::bind(SocketAddr::new(
                self.config.datagram_forward.bind_ip,
                self.config.datagram_forward.port,
            ))
            .await?;
            let local_port = socket.local_addr()?.port();
            udp_socket = Some(socket);
            structural.push(("PORT".into(), local_port.to_string()));
            structural.push((
                "HOST".into(),
                self.config.datagram_forward.advertised_host.to_string(),
            ));
        } else if style == SessionStyle::Raw && header {
            structural.push(("HEADER".into(), "true".into()));
        }

        let (mut control, _) = open_control(&self.config).await?;
        let command = self.session_create_command(
            destination,
            generated.first(),
            id,
            style.as_wire(),
            &structural,
            options,
        )?;
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") != Some("OK") {
            let mut observed = self.capabilities.write().await;
            match style {
                SessionStyle::Datagram => observed.datagram = Support::Unsupported,
                SessionStyle::Raw => observed.raw = Support::Unsupported,
                SessionStyle::Datagram2 => observed.datagram2 = Support::Unsupported,
                SessionStyle::Datagram3 => observed.datagram3 = Support::Unsupported,
                SessionStyle::Stream => {}
            }
            return Err(rejection(&reply));
        }
        {
            let mut observed = self.capabilities.write().await;
            match style {
                SessionStyle::Datagram => observed.datagram = Support::Supported,
                SessionStyle::Raw => observed.raw = Support::Supported,
                SessionStyle::Datagram2 => observed.datagram2 = Support::Supported,
                SessionStyle::Datagram3 => observed.datagram3 = Support::Supported,
                SessionStyle::Stream => {}
            }
            if transport == DatagramTransport::ControlSocketV1 {
                match style {
                    SessionStyle::Raw => observed.raw_direct = Support::Supported,
                    _ => observed.datagram_direct = Support::Supported,
                }
            }
        }
        let identity = resolve_identity(&mut control, &self.config).await.ok();
        if identity.is_some() {
            self.mark(|c| &mut c.session_identity_lookup, Support::Supported)
                .await;
        }
        // Anything the request/response phase already buffered must survive the switch to
        // demultiplexed mode, or a delivery that arrived early would be silently dropped.
        let leftover = control.reader.buffer().to_vec();
        let link = match transport {
            DatagramTransport::ControlSocketV1 => Some(Arc::new(ControlDatagramLink::new(
                id.to_owned(),
                style,
                control.reader.into_inner(),
                leftover,
                self.config.control_timeout,
                self.config.max_frame_bytes,
                self.config.max_datagram_bytes,
                self.config.max_inbox_datagrams,
                self.config.max_inbox_bytes,
            ))),
            DatagramTransport::UdpForward => None,
        };
        Ok(DatagramSession {
            id: id.to_owned(),
            style,
            transport,
            protocol,
            metadata: ForwardedMetadata { header, protocol },
            config: self.config.clone(),
            identity,
            udp_socket,
            link,
            closed: AtomicBool::new(false),
            closed_notify: Arc::new(Notify::new()),
            _lifetime: ReleaseOnDrop::new(resource::session_closed),
        })
    }
}

fn major_minor(version: &SamVersion) -> (u8, u8) {
    (version.major, version.minor)
}

fn option_bool(options: &[(String, String)], key: &str) -> Result<Option<bool>, SamError> {
    match options
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
    {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(_) => Err(SamError::Rejected(format!("invalid {key} option"))),
    }
}

fn option_u8(options: &[(String, String)], key: &str) -> Result<Option<u8>, SamError> {
    match options
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
    {
        None => Ok(None),
        Some(value) => value
            .parse()
            .map(Some)
            .map_err(|_| SamError::Rejected(format!("invalid {key} option"))),
    }
}

pub struct DatagramSession {
    id: String,
    style: SessionStyle,
    transport: DatagramTransport,
    protocol: I2pProtocol,
    metadata: ForwardedMetadata,
    config: ClientConfig,
    identity: Option<SessionIdentity>,
    udp_socket: Option<UdpSocket>,
    link: Option<Arc<ControlDatagramLink>>,
    closed: AtomicBool,
    closed_notify: Arc<Notify>,
    _lifetime: ReleaseOnDrop,
}

impl DatagramSession {
    pub fn style(&self) -> SessionStyle {
        self.style
    }

    pub fn transport(&self) -> DatagramTransport {
        self.transport
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The session's concrete identity, when the router exposed one.
    pub fn identity(&self) -> Option<&SessionIdentity> {
        self.identity.as_ref()
    }

    pub async fn send(
        &self,
        destination: &str,
        payload: &[u8],
        from_port: Option<Port>,
        to_port: Option<Port>,
    ) -> Result<(), SamError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(SamError::Closed);
        }
        if payload.is_empty() || payload.len() > self.config.max_datagram_bytes {
            return Err(SamError::Rejected(
                "datagram payload outside configured bounds".into(),
            ));
        }
        validate_destination_token(destination)?;
        match (&self.link, self.transport) {
            (Some(link), DatagramTransport::ControlSocketV1) => {
                let protocol = (self.style == SessionStyle::Raw).then_some(self.protocol);
                let command = datagram::build_direct_send_command(
                    self.style,
                    &self.id,
                    destination,
                    payload.len(),
                    from_port,
                    to_port,
                    protocol,
                )?;
                let reply = link.send_payload(&command, payload).await?;
                if reply.field("RESULT") != Some("OK") {
                    return Err(rejection(&reply));
                }
                Ok(())
            }
            (Some(_), DatagramTransport::UdpForward) => Err(SamError::Rejected(
                "control-socket transport was not initialised".into(),
            )),
            (None, DatagramTransport::UdpForward) => {
                let socket = self
                    .udp_socket
                    .as_ref()
                    .ok_or_else(|| SamError::Rejected("UDP forwarding socket is absent".into()))?;
                let version = version_text(self.config.datagram_frame_version);
                let protocol = (self.style == SessionStyle::Raw).then_some(self.protocol);
                let frame = datagram::build_forwarded_frame(
                    &version,
                    &self.id,
                    destination,
                    payload,
                    from_port,
                    to_port,
                    protocol,
                )?;
                socket
                    .send_to(&frame, self.config.datagram_endpoint)
                    .await?;
                Ok(())
            }
            (None, DatagramTransport::ControlSocketV1) => Err(SamError::Closed),
        }
    }

    pub async fn recv(&self) -> Result<ReceivedDatagram, SamError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(SamError::Closed);
        }
        match (&self.link, self.transport) {
            (Some(link), DatagramTransport::ControlSocketV1) => link.recv_datagram().await,
            (Some(_), DatagramTransport::UdpForward) => Err(SamError::Rejected(
                "control-socket transport was not initialised".into(),
            )),
            (None, DatagramTransport::UdpForward) => {
                let socket = self
                    .udp_socket
                    .as_ref()
                    .ok_or_else(|| SamError::Rejected("UDP forwarding socket is absent".into()))?;
                let mut packet = vec![0; 65_507];
                let closed = self.closed_notify.notified();
                if self.closed.load(Ordering::Acquire) {
                    return Err(SamError::Closed);
                }
                let (size, _) = tokio::select! {
                    _ = closed => return Err(SamError::Closed),
                    result = socket.recv_from(&mut packet) => result?,
                };
                packet.truncate(size);
                if size > self.config.max_datagram_bytes + 1024 {
                    return Err(SamError::Rejected(
                        "received datagram exceeds configured bounds".into(),
                    ));
                }
                datagram::decode_forwarded_datagram(self.style, self.metadata, &packet)
            }
            (None, DatagramTransport::ControlSocketV1) => Err(SamError::Closed),
        }
    }

    /// Deliveries dropped because the bounded control-socket queue was full.
    pub async fn dropped_datagrams(&self) -> u64 {
        match &self.link {
            Some(link) => link.dropped_datagrams().await,
            None => 0,
        }
    }

    pub async fn queued_datagrams(&self) -> usize {
        match &self.link {
            Some(link) => link.queued_datagrams().await,
            None => 0,
        }
    }

    pub async fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.closed_notify.notify_waiters();
        if let Some(link) = &self.link {
            link.close().await;
        }
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
            || value.bytes().any(|b| b.is_ascii_control())
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

fn validate_session_inputs(destination: &SessionDestination, id: &str) -> Result<(), SamError> {
    SessionId::new(id).map_err(SamError::Protocol)?;
    if let Some(value) = destination.wire_value() {
        validate_destination_token(&value)?;
    }
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
        let listen = value("LISTEN_PROTOCOL")
            .or_else(|| value("PROTOCOL"))
            .unwrap_or("18")
            .parse::<u8>()
            .map_err(|_| SamError::Rejected("invalid listener protocol".into()))?;
        // Streaming traffic is never routed to a RAW subsession, so a RAW subsession may
        // not advertise protocol 6 even as a wildcard value.
        if style == SessionStyle::Raw && value("LISTEN_PROTOCOL").is_some() && listen == 6 {
            return Err(SamError::Rejected(
                "RAW subsession may not set LISTEN_PROTOCOL=6".into(),
            ));
        }
        I2pProtocol::new(listen).map_err(SamError::Protocol)?.get()
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
    identity: SessionIdentity,
    dialect: SharedDialect,
    config: ClientConfig,
    control: Mutex<Option<Control<TcpStream>>>,
    children: Mutex<HashMap<String, ChildEntry>>,
    capabilities: Arc<RwLock<SamCapabilities>>,
    live: Arc<AtomicBool>,
    closed_notify: Arc<Notify>,
    max_children: usize,
    negotiated_version: SamVersion,
    _lifetime: ReleaseOnDrop,
}

struct ChildEntry {
    listener: (u16, u8),
    live: Arc<AtomicBool>,
    closed_notify: Arc<Notify>,
}

impl SharedSession {
    /// The concrete Destination every child of this session shares.
    pub fn identity(&self) -> &SessionIdentity {
        &self.identity
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
        let style_text = style.as_wire();
        // `effective` is what the session will actually be configured with; `structural`
        // holds the fields this client owns so a caller cannot collide with them.
        let mut effective = options.to_vec();
        let mut structural: Vec<(String, String)> = Vec::new();
        let mut datagram_socket = None;
        let mut local_forwarding = false;
        let mut metadata = ForwardedMetadata {
            header: false,
            protocol: I2pProtocol::new(18).expect("default RAW protocol is valid"),
        };
        if matches!(
            style,
            SessionStyle::Datagram
                | SessionStyle::Raw
                | SessionStyle::Datagram2
                | SessionStyle::Datagram3
        ) {
            let host = effective
                .iter()
                .find(|(key, _)| key == "HOST")
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| "127.0.0.1".into());
            if !effective.iter().any(|(key, _)| key == "HOST") {
                effective.push(("HOST".into(), host.clone()));
                structural.push(("HOST".into(), host.clone()));
            }
            if host == "127.0.0.1" || host == "localhost" {
                let requested_port = effective
                    .iter()
                    .find(|(key, _)| key == "PORT")
                    .map(|(_, value)| value.parse::<u16>())
                    .transpose()
                    .map_err(|_| SamError::Rejected("invalid shared UDP forwarding port".into()))?;
                let bind =
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), requested_port.unwrap_or(0));
                let socket = UdpSocket::bind(bind).await?;
                let actual_port = socket.local_addr()?.port();
                if !effective.iter().any(|(key, _)| key == "PORT") {
                    // The bridge must forward to the port we actually bound.
                    effective.push(("PORT".into(), actual_port.to_string()));
                    structural.push(("PORT".into(), actual_port.to_string()));
                }
                datagram_socket = Some(socket);
                local_forwarding = true;
            } else {
                if !effective.iter().any(|(key, _)| key == "PORT") {
                    return Err(SamError::Rejected(
                        "remote UDP forwarding requires explicit PORT".into(),
                    ));
                }
                // Nothing is delivered locally for a remote host; the socket only exists so
                // the outbound path has somewhere to send from.
                datagram_socket = Some(
                    UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).await?,
                );
            }
            if style == SessionStyle::Raw {
                // Subsessions use the datagram forwarding path; the v1/v2-compatible
                // control-socket modes are not available on a shared session.
                let header = option_bool(&effective, "HEADER")?
                    .unwrap_or(major_minor(&self.negotiated_version) >= (3, 2));
                if header && !effective.iter().any(|(key, _)| key == "HEADER") {
                    effective.push(("HEADER".into(), "true".into()));
                    structural.push(("HEADER".into(), "true".into()));
                }
                metadata.header = header;
                metadata.protocol = option_u8(&effective, "PROTOCOL")?
                    .map(I2pProtocol::new)
                    .transpose()
                    .map_err(SamError::Protocol)?
                    .unwrap_or_else(|| I2pProtocol::new(18).expect("valid default protocol"));
            }
        }
        let listener = listener_tuple(style, &effective)?;
        if children
            .values()
            .any(|existing| existing.listener == listener)
        {
            return Err(SamError::Rejected(
                "duplicate shared-session listener tuple".into(),
            ));
        }
        let mut command = format!("SESSION ADD STYLE={style_text} ID={}", quote(id));
        let mut reserved: Vec<&str> = vec!["STYLE", "ID", "DESTINATION"];
        for (key, value) in &structural {
            reserved.push(key.as_str());
            command.push_str(&format!(" {key}={value}"));
        }
        append_options(&mut command, options, &reserved)?;
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
        if result.field("RESULT") != Some("OK") {
            if unsupported_style_reply(&result) {
                let mut capabilities = self.capabilities.write().await;
                match style {
                    SessionStyle::Datagram2 => capabilities.datagram2 = Support::Unsupported,
                    SessionStyle::Datagram3 => capabilities.datagram3 = Support::Unsupported,
                    _ => {}
                }
            }
            return Err(rejection(&result));
        }
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
            match style {
                SessionStyle::Stream => observed.stream = Support::Supported,
                SessionStyle::Datagram => observed.datagram = Support::Supported,
                SessionStyle::Raw => observed.raw = Support::Supported,
                SessionStyle::Datagram2 => observed.datagram2 = Support::Supported,
                SessionStyle::Datagram3 => observed.datagram3 = Support::Supported,
            }
            observed.session_add_remove = Support::Supported;
        }
        Ok(SharedChild {
            id: id.to_owned(),
            style,
            style_text: style_text.to_owned(),
            identity: self.identity.clone(),
            live: Arc::downgrade(&self.live),
            child_live,
            config: self.config.clone(),
            datagram_socket,
            local_forwarding,
            metadata,
            datagram_frame_version: self.config.datagram_frame_version,
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
        if result.field("RESULT") != Some("OK") {
            return Err(rejection(&result));
        }
        if let Some(child) = children.remove(id) {
            child.live.store(false, Ordering::Release);
            child.closed_notify.notify_waiters();
        }
        Ok(())
    }

    pub async fn close(&self) {
        if !self.live.swap(false, Ordering::AcqRel) {
            return;
        }
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
    identity: SessionIdentity,
    live: std::sync::Weak<AtomicBool>,
    child_live: Arc<AtomicBool>,
    config: ClientConfig,
    datagram_socket: Option<UdpSocket>,
    local_forwarding: bool,
    metadata: ForwardedMetadata,
    datagram_frame_version: SamVersion,
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
    pub fn style_kind(&self) -> SessionStyle {
        self.style
    }
    /// The owner's concrete Destination; a child never invents its own identity.
    pub fn identity(&self) -> &SessionIdentity {
        &self.identity
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
        from_port: Option<Port>,
        to_port: Option<Port>,
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
            command.push_str(&format!(" FROM_PORT={}", port.get()));
        }
        if let Some(port) = to_port {
            command.push_str(&format!(" TO_PORT={}", port.get()));
        }
        command.push('\n');
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") != Some("OK") {
            return Err(rejection(&reply));
        }
        resource::stream_opened();
        Ok(SamStream {
            inner: control_socket::Prefixed::new(Vec::new(), control.reader),
            peer: None,
            owner_live: Some(self.live.clone()),
            _permit: permit,
            _lifetime: ReleaseOnDrop::new(resource::stream_closed),
        })
    }

    pub async fn accept(&self) -> Result<SamStream, SamError> {
        self.accept_with(false).await
    }

    /// Accept with an explicit `SILENT` choice; non-silent accepts announce the peer.
    pub async fn accept_with(&self, silent: bool) -> Result<SamStream, SamError> {
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
        let mut command = format!("STREAM ACCEPT ID={}", quote(&self.id));
        if silent {
            command.push_str(" SILENT=true");
        }
        command.push('\n');
        let reply = control
            .command(
                &command,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") != Some("OK") {
            return Err(rejection(&reply));
        }
        let (peer, pushed_back) = if silent {
            (None, Vec::new())
        } else {
            let (peer, pushed_back) = control_socket::read_stream_peer(
                &mut control.reader,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
            (Some(peer), pushed_back)
        };
        resource::stream_opened();
        Ok(SamStream {
            inner: control_socket::Prefixed::new(pushed_back, control.reader),
            peer,
            owner_live: Some(self.live.clone()),
            _permit: permit,
            _lifetime: ReleaseOnDrop::new(resource::stream_closed),
        })
    }

    pub async fn send_datagram(
        &self,
        destination: &str,
        payload: &[u8],
        from_port: Option<Port>,
        to_port: Option<Port>,
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
        if payload.is_empty() || payload.len() > self.config.max_datagram_bytes {
            return Err(SamError::Rejected(
                "datagram arguments outside configured bounds".into(),
            ));
        }
        let socket = self.datagram_socket.as_ref().ok_or_else(|| {
            SamError::Rejected("shared datagram child has no forwarding socket".into())
        })?;
        let protocol = (self.style == SessionStyle::Raw).then_some(self.metadata.protocol);
        let frame = datagram::build_forwarded_frame(
            &version_text(self.datagram_frame_version),
            &self.id,
            destination,
            payload,
            from_port,
            to_port,
            protocol,
        )?;
        socket
            .send_to(&frame, self.config.datagram_endpoint)
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
            SamError::Rejected("shared datagram child has no receive socket".into())
        })?;
        let mut packet = vec![0; 65_507];
        let owner_closed = self.owner_closed.notified();
        let local_closed = self.closed_notify.notified();
        if !self.is_open() {
            return Err(SamError::Closed);
        }
        let (size, _) = tokio::select! {
            _ = owner_closed => return Err(SamError::Closed),
            _ = local_closed => return Err(SamError::Closed),
            result = socket.recv_from(&mut packet) => result?,
        };
        packet.truncate(size);
        datagram::decode_forwarded_datagram(self.style, self.metadata, &packet)
    }

    pub async fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.closed_notify.notify_waiters();
        self.operations.close();
    }
}

async fn open_control(config: &ClientConfig) -> Result<(Control<TcpStream>, SamVersion), SamError> {
    let stream = timeout(config.connect_timeout, TcpStream::connect(config.endpoint))
        .await
        .map_err(|_| SamError::Timeout)??;
    resource::socket_opened();
    let mut control = Control {
        reader: BufReader::new(stream),
        _lifetime: ReleaseOnDrop::new(resource::socket_closed),
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
        Err(rejection(reply))
    }
}

/// Turn a non-OK reply into the most specific error class available.
///
/// A style rejection becomes `Unsupported`, because that is a capability verdict rather
/// than a transient failure, and retrying it as transport trouble would be wrong.
fn rejection(reply: &Line) -> SamError {
    let result = i2pr_sam_proto::SamResult::parse(reply.field("RESULT").unwrap_or(""));
    if unsupported_style_reply(reply) {
        return SamError::Unsupported(result_name(&result).to_owned());
    }
    SamError::Rejected(reply.field("RESULT").unwrap_or("unknown").to_owned())
}

/// Some routers report an explicitly unknown session style using their generic error
/// result. Preserve that router verdict without treating other I2P_ERROR replies as support
/// information.
fn unsupported_style_reply(reply: &Line) -> bool {
    let result = i2pr_sam_proto::SamResult::parse(reply.field("RESULT").unwrap_or(""));
    result.is_unsupported_style()
        || (matches!(result, i2pr_sam_proto::SamResult::I2pError)
            && reply
                .field("MESSAGE")
                .is_some_and(|message| message.eq_ignore_ascii_case("Unknown STYLE")))
}

fn result_name(result: &i2pr_sam_proto::SamResult) -> &'static str {
    match result {
        i2pr_sam_proto::SamResult::CantReachPeer => "CANT_REACH_PEER",
        i2pr_sam_proto::SamResult::DuplicateId => "DUPLICATED_ID",
        i2pr_sam_proto::SamResult::DuplicateDestination => "DUPLICATED_DEST",
        i2pr_sam_proto::SamResult::I2pError => "I2P_ERROR",
        i2pr_sam_proto::SamResult::InvalidKey => "INVALID_KEY",
        i2pr_sam_proto::SamResult::InvalidId => "INVALID_ID",
        i2pr_sam_proto::SamResult::InvalidStyle => "INVALID_STYLE",
        i2pr_sam_proto::SamResult::KeyNotFound => "KEY_NOT_FOUND",
        i2pr_sam_proto::SamResult::LeaseSetNotFound => "LEASESET_NOT_FOUND",
        i2pr_sam_proto::SamResult::PeerNotFound => "PEER_NOT_FOUND",
        i2pr_sam_proto::SamResult::Timeout => "TIMEOUT",
        i2pr_sam_proto::SamResult::Ok => "OK",
        i2pr_sam_proto::SamResult::Unknown(_) => "UNKNOWN",
    }
}

/// Render one protocol token, quoting it only when whitespace demands it.
///
/// Callers must have validated their value first: this function escapes quotes and
/// backslashes, but it deliberately does not attempt to make a control character safe.
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

/// Bounded initial bridge-connection retry.
///
/// This governs establishing the client connection only. It does not reconnect an existing
/// session and never changes an established Destination identity. The name states what it
/// does, so a reader of a call site can see that an existing session is never silently
/// re-established underneath it.
#[derive(Clone, Debug)]
pub struct ConnectRetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_elapsed: Duration,
    /// Process-wide cap on connections simultaneously retrying.
    pub max_concurrent_admissions: usize,
    pub saturation: RetrySaturation,
}

/// What happens when too many clients are already retrying.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RetrySaturation {
    /// Queue until the shared admission budget frees up, still bounded by `max_elapsed`.
    #[default]
    Wait,
    /// Fail immediately rather than queueing.
    Reject,
}

impl Default for ConnectRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            max_elapsed: Duration::from_secs(30),
            max_concurrent_admissions: default_admission_limit(),
            saturation: RetrySaturation::Wait,
        }
    }
}

/// Process-wide admission budget for connect retries.
///
/// Without a shared cap, N callers each retrying M times can multiply load on a router that
/// is already struggling. The budget is process-wide, not per client.
fn default_admission_limit() -> usize {
    static LIMIT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("I2PR_SAM_MAX_CONNECT_RETRIES")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .unwrap_or(16)
    })
}

static CONNECT_RETRY_ADMISSION: std::sync::OnceLock<Arc<Semaphore>> = std::sync::OnceLock::new();

fn retry_admission() -> &'static Arc<Semaphore> {
    CONNECT_RETRY_ADMISSION.get_or_init(|| Arc::new(Semaphore::new(default_admission_limit())))
}

impl ConnectRetryPolicy {
    pub fn bounded(
        max_attempts: u32,
        initial_backoff: Duration,
        max_backoff: Duration,
        max_elapsed: Duration,
    ) -> Result<Self, SamError> {
        if max_attempts == 0 || initial_backoff > max_backoff || max_elapsed.is_zero() {
            return Err(SamError::Rejected(
                "invalid connect retry policy bounds".into(),
            ));
        }
        Ok(Self {
            max_attempts,
            initial_backoff,
            max_backoff,
            max_elapsed,
            ..Self::default()
        })
    }

    /// Backoff applied before attempt `attempt` (0-based), capped by `max_backoff`.
    ///
    /// Pure and side-effect free, so the schedule can be asserted without sleeping.
    pub fn backoff_before(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        // The first retry waits one initial backoff; each later retry doubles it.
        let factor = 1u32
            .checked_shl(attempt.saturating_sub(1).min(16))
            .unwrap_or(u32::MAX);
        self.initial_backoff
            .saturating_mul(factor)
            .min(self.max_backoff)
    }
}

/// Time source for connect retries.
///
/// Injecting this makes backoff and deadline behaviour testable without wall-clock waits,
/// which is the only way to assert a retry budget deterministically.
pub trait RetryClock: Send + Sync {
    /// Time consumed since the retry loop started.
    fn elapsed(&self) -> Duration;
    fn sleep(&self, duration: Duration) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// Wall-clock implementation backed by Tokio.
#[derive(Clone, Copy, Debug, Default)]
pub struct TokioRetryClock {
    started: Option<tokio::time::Instant>,
}

impl TokioRetryClock {
    pub fn start() -> Self {
        Self {
            started: Some(tokio::time::Instant::now()),
        }
    }
}

impl RetryClock for TokioRetryClock {
    fn elapsed(&self) -> Duration {
        match self.started {
            Some(started) => started.elapsed(),
            None => Duration::ZERO,
        }
    }

    fn sleep(&self, duration: Duration) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep(duration))
    }
}

/// Superseded name kept as an alias so existing call sites keep compiling.
pub type ReconnectPolicy = ConnectRetryPolicy;

fn remaining_budget(policy: &ConnectRetryPolicy, clock: &dyn RetryClock) -> Duration {
    policy.max_elapsed.saturating_sub(clock.elapsed())
}

async fn acquire_retry_admission(
    policy: &ConnectRetryPolicy,
    clock: &dyn RetryClock,
) -> Result<OwnedSemaphorePermit, SamError> {
    // `Arc<Semaphore>` so a permit can outlive the static reference.
    let global = Arc::clone(retry_admission());
    let permit = match policy.saturation {
        RetrySaturation::Reject => global
            .clone()
            .try_acquire_owned()
            .map_err(|_| SamError::RetryAdmissionSaturated)?,
        RetrySaturation::Wait => {
            let remaining = remaining_budget(policy, clock);
            if remaining.is_zero() {
                return Err(SamError::RetryAdmissionSaturated);
            }
            match timeout(remaining, global.clone().acquire_owned()).await {
                Ok(Ok(permit)) => permit,
                _ => return Err(SamError::RetryAdmissionSaturated),
            }
        }
    };
    Ok(permit)
}

pub struct StreamSession {
    config: ClientConfig,
    destination: SessionDestination,
    identity: Option<SessionIdentity>,
    id: String,
    operations: Arc<Semaphore>,
    control: Mutex<Option<Control<TcpStream>>>,
}
impl StreamSession {
    pub fn requested_destination(&self) -> &SessionDestination {
        &self.destination
    }
    pub fn identity(&self) -> Option<&SessionIdentity> {
        self.identity.as_ref()
    }
    pub fn id(&self) -> &str {
        &self.id
    }

    pub async fn connect(
        &self,
        destination: &str,
        from_port: Option<Port>,
        to_port: Option<Port>,
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
            cmd.push_str(&format!(" FROM_PORT={}", p.get()));
        }
        if let Some(p) = to_port {
            cmd.push_str(&format!(" TO_PORT={}", p.get()));
        }
        cmd.push('\n');
        let reply = control
            .command(
                &cmd,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") != Some("OK") {
            return Err(rejection(&reply));
        }
        resource::stream_opened();
        Ok(SamStream {
            inner: control_socket::Prefixed::new(Vec::new(), control.reader),
            peer: None,
            owner_live: None,
            _permit: permit,
            _lifetime: ReleaseOnDrop::new(resource::stream_closed),
        })
    }

    pub async fn accept(&self) -> Result<SamStream, SamError> {
        self.accept_with(false).await
    }

    /// Accept with an explicit `SILENT` choice.
    ///
    /// Non-silent accepts consume the router's peer identity block so payload framing
    /// starts at the first application byte. Silent accepts receive payload immediately.
    pub async fn accept_with(&self, silent: bool) -> Result<SamStream, SamError> {
        let permit = self
            .operations
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SamError::Closed)?;
        let (mut control, _) = open_control(&self.config).await?;
        let mut cmd = format!("STREAM ACCEPT ID={}", quote(&self.id));
        if silent {
            cmd.push_str(" SILENT=true");
        }
        cmd.push('\n');
        let reply = control
            .command(
                &cmd,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
        if reply.field("RESULT") != Some("OK") {
            return Err(rejection(&reply));
        }
        let (peer, pushed_back) = if silent {
            (None, Vec::new())
        } else {
            let (peer, pushed_back) = control_socket::read_stream_peer(
                &mut control.reader,
                self.config.control_timeout,
                self.config.max_frame_bytes,
            )
            .await?;
            (Some(peer), pushed_back)
        };
        resource::stream_opened();
        Ok(SamStream {
            inner: control_socket::Prefixed::new(pushed_back, control.reader),
            peer,
            owner_live: None,
            _permit: permit,
            _lifetime: ReleaseOnDrop::new(resource::stream_closed),
        })
    }

    pub async fn close(&self) {
        self.operations.close();
        self.control.lock().await.take();
    }
}

pub struct SamStream {
    inner: control_socket::Prefixed<BufReader<TcpStream>>,
    peer: Option<StreamPeer>,
    owner_live: Option<std::sync::Weak<AtomicBool>>,
    _permit: OwnedSemaphorePermit,
    _lifetime: ReleaseOnDrop,
}
impl SamStream {
    /// Authenticated peer announced by a non-silent accept.
    pub fn peer(&self) -> Option<&StreamPeer> {
        self.peer.as_ref()
    }

    pub fn remote_destination(&self) -> Option<&Destination> {
        self.peer.as_ref().map(|peer| &peer.destination)
    }
}
impl AsyncRead for SamStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if owner_lost(self.owner_live.as_ref()) {
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
        if owner_lost(self.owner_live.as_ref()) {
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

fn owner_lost(live: Option<&std::sync::Weak<AtomicBool>>) -> bool {
    live.is_some_and(|live| {
        !live
            .upgrade()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_and_generated_destinations_are_redacted() {
        let credentials = Credentials {
            username: "private-user".into(),
            password: "private-password".into(),
        };
        let display = format!("{credentials:?}");
        assert!(!display.contains("private-user"));
        assert!(!display.contains("private-password"));
        let generated = GeneratedDestination::new(
            Destination::new("public-key").unwrap(),
            SecretDestination::new("private-key").unwrap(),
        );
        let display = format!("{generated:?}");
        assert!(display.contains("public-key"));
        assert!(!display.contains("private-key"));
    }

    #[test]
    fn style_rejection_is_unsupported_not_a_transient_failure() {
        let line = parse_line(b"SESSION STATUS RESULT=INVALID_STYLE\n").unwrap();
        assert!(matches!(rejection(&line), SamError::Unsupported(_)));
        assert_eq!(
            classify_failure(&rejection(&line)),
            FailureClass::CapabilityUnsupported
        );
        let transient = parse_line(b"SESSION STATUS RESULT=CANT_REACH_PEER\n").unwrap();
        assert!(matches!(rejection(&transient), SamError::Rejected(_)));
        let duplicate = parse_line(b"SESSION STATUS RESULT=DUPLICATED_ID\n").unwrap();
        assert!(matches!(rejection(&duplicate), SamError::Rejected(_)));
    }

    #[test]
    fn transport_exclusions_are_enforced_before_any_wire_traffic() {
        assert!(
            datagram::ensure_transport_supported(
                SessionStyle::Datagram,
                DatagramTransport::ControlSocketV1
            )
            .is_ok()
        );
        assert!(
            datagram::ensure_transport_supported(
                SessionStyle::Datagram2,
                DatagramTransport::ControlSocketV1
            )
            .is_err()
        );
        assert!(
            datagram::ensure_transport_supported(
                SessionStyle::Datagram3,
                DatagramTransport::ControlSocketV1
            )
            .is_err()
        );
    }

    #[test]
    fn raw_subsession_may_not_claim_a_reserved_protocol() {
        // The specification forbids 6, 17, 19 and 20 for STYLE=RAW: those numbers belong
        // to the streaming and datagram styles, and a RAW subsession advertising one would
        // claim a listener the router will never route to it.
        for reserved in ["6", "17", "19", "20"] {
            assert!(
                validate_child_options(
                    SessionStyle::Raw,
                    &[("LISTEN_PROTOCOL".into(), reserved.into())]
                )
                .is_err(),
                "LISTEN_PROTOCOL={reserved} must be rejected"
            );
            assert!(
                listener_tuple(SessionStyle::Raw, &[("PROTOCOL".into(), reserved.into())]).is_err()
            );
        }
        assert!(listener_tuple(SessionStyle::Raw, &[("PROTOCOL".into(), "18".into())]).is_ok());
        assert!(
            listener_tuple(
                SessionStyle::Stream,
                &[
                    ("LISTEN_PORT".into(), "5".into()),
                    ("FROM_PORT".into(), "9".into())
                ]
            )
            .is_err(),
            "a STREAM listener port must match FROM_PORT"
        );
        assert!(
            listener_tuple(SessionStyle::Stream, &[("LISTEN_PORT".into(), "0".into())]).is_ok()
        );
    }

    #[test]
    fn connect_retry_backoff_is_pure_and_bounded() {
        let policy = ConnectRetryPolicy::bounded(
            5,
            Duration::from_millis(100),
            Duration::from_millis(400),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(policy.backoff_before(0), Duration::ZERO);
        assert_eq!(policy.backoff_before(1), Duration::from_millis(100));
        assert_eq!(policy.backoff_before(2), Duration::from_millis(200));
        assert_eq!(policy.backoff_before(3), Duration::from_millis(400));
        // Capped, and never overflowing on an absurd attempt index.
        assert_eq!(policy.backoff_before(40), Duration::from_millis(400));
        assert!(
            ConnectRetryPolicy::bounded(
                0,
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_secs(1)
            )
            .is_err()
        );
        assert!(
            ConnectRetryPolicy::bounded(
                1,
                Duration::from_secs(2),
                Duration::from_millis(1),
                Duration::from_secs(1)
            )
            .is_err(),
            "initial backoff above the ceiling is nonsense"
        );
    }

    #[test]
    fn peer_targets_distinguish_destinations_from_hash_addresses() {
        let destination = PeerTarget::classify(
            "me.i2p",
            "jT~IyXaoauTni6N4517EG8mrFUKpy0IlgZh-EY9csMAk8Ps7Rl4Swx8Tl7YRO7b~Ls7R36rOEc7qM0OTIcmyXDJ7rJYzBMptVCMzdCBu~cCWZZOubmzo7y2kub0kpsnw3tr~HY2LGI7cvbc8Lg~~KuV6pr3Q6rhQZXns1Qtotvw==",
        )
        .unwrap();
        assert!(destination.destination().is_some());
        let hash =
            PeerTarget::classify("stats.i2p", "ukeu3k5oycgaauneqgtnvsvmtzwy4zabb7d3bb2y2n6g")
                .unwrap();
        assert!(matches!(hash, PeerTarget::Base32Hash { .. }));
        assert!(
            hash.require_destination().is_err(),
            "a hash address must never be used where a Destination is required"
        );
    }

    #[test]
    fn destinations_with_control_characters_are_refused_before_framing() {
        assert!(validate_destination_token("good.b32.i2p").is_ok());
        assert!(
            validate_destination_token("peer\nSESSION CREATE STYLE=STREAM").is_err(),
            "a newline in a destination would inject a second SAM command"
        );
        assert!(validate_destination_token("").is_err());
        assert!(validate_destination_token(&"x".repeat(4096)).is_err());
    }

    #[test]
    fn session_options_reject_reserved_keys_and_control_values() {
        let options = SessionOptions::builder()
            .option("sam.udp.host", "127.0.0.1")
            .build(&["STYLE", "ID", "DESTINATION"])
            .unwrap();
        assert_eq!(options.len(), 1);
        assert!(
            SessionOptions::builder()
                .option("STYLE", "STREAM")
                .build(&["STYLE"])
                .is_err()
        );
        assert!(SessionOptions::new().with("bad key", "v").is_err());
        assert!(SessionOptions::new().with("ok", "a\nb").is_err());
        // A repeated key overwrites instead of duplicating a wire option.
        let merged = SessionOptions::from(vec![
            ("HEADER".to_owned(), "true".to_owned()),
            ("HEADER".to_owned(), "false".to_owned()),
        ]);
        assert_eq!(merged.get("HEADER"), Some("false"));
    }
}
