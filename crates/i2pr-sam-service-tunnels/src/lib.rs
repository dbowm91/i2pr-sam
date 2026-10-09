//! SAM STREAM adapter for the public, runtime-neutral i2pr service-tunnel core.
//!
//! This crate consumes core types and filters through its pinned public API. It owns no
//! copied policy implementation. In this initial adapter slice, callers own local listener
//! acceptance; the adapter owns SAM session/group composition, peer-authenticated server
//! accepts, policy admission, and bounded forwarding to loopback TCP targets.

use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
    time::Instant,
};

use i2pr_sam::{
    SamClient, SamError, SamStream, SharedChild, SharedSession, StreamSession, destination_hash,
};
use i2pr_sam_proto::{Destination, SecretDestination, SessionStyle, SharedDialect};
use i2pr_service_tunnels::{
    DestinationCryptoPolicy, DestinationGroupKey, DestinationGroupSpec, DestinationPolicy,
    DestinationRef, FilteredServerRequest, HttpLimits, HttpRequestHead, HttpServerPolicy,
    ServerConnectionRateLimiter, ServerTarget, ServiceTunnelError, ServiceTunnelKind,
    ServiceTunnelSet, ServiceTunnelSpec, filter_server_request_with_policy, parse_request_head,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{Duration, timeout},
};

/// Stable key passed to an adapter-owned identity store.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct IdentityKey {
    /// Core-assigned dedicated or explicit linkability group.
    pub group: DestinationGroupKey,
    /// Optional logical key reference from the core policy.
    pub key_reference: Option<String>,
}

/// Private identity material returned by an adapter-owned store.
#[derive(Clone)]
pub struct StoredIdentity {
    public: Destination,
    secret: SecretDestination,
}

impl StoredIdentity {
    /// Construct an identity returned from persistent storage.
    pub fn new(public: Destination, secret: SecretDestination) -> Self {
        Self { public, secret }
    }

    /// Public Destination text.
    pub fn public(&self) -> &Destination {
        &self.public
    }

    /// Private key material, available only for explicit persistence/session import.
    pub fn secret(&self) -> &SecretDestination {
        &self.secret
    }
}

/// Bounded, non-secret storage failures. Implementations should avoid including key bytes in
/// errors because this value is rendered in adapter diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum IdentityStoreError {
    /// The store could not read the requested identity.
    #[error("identity store read failed")]
    Read,
    /// The store could not persist the generated identity.
    #[error("identity store write failed")]
    Write,
}

/// Persistence is supplied by the consuming application; this adapter never chooses a
/// filesystem path or writes key material itself.
pub trait DestinationIdentityStore: Send + Sync {
    /// Load a stored identity for a core-assigned group.
    fn load(&self, key: &IdentityKey) -> Result<Option<StoredIdentity>, IdentityStoreError>;

    /// Persist one generated identity before its session is started.
    fn store(&self, key: &IdentityKey, identity: &StoredIdentity)
    -> Result<(), IdentityStoreError>;
}

/// Adapter setup, policy, and transport failures.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// Invalid service specifications or profile combination.
    #[error(transparent)]
    Config(#[from] ServiceTunnelError),
    /// SAM operation failed.
    #[error(transparent)]
    Sam(#[from] SamError),
    /// HTTP request parsing failed in the pinned policy core.
    #[error("HTTP request head is malformed or incomplete")]
    HttpParseRejected,
    /// HTTP policy rejected the request in the pinned policy core.
    #[error("HTTP server request was refused by policy: {0}")]
    HttpPolicy(String),
    /// A requested profile is outside this adapter slice.
    #[error("service profile is not supported by the SAM STREAM adapter: {0}")]
    UnsupportedProfile(String),
    /// Persistence is required for this destination group.
    #[error("persistent destination identity requires an identity store")]
    IdentityStoreRequired,
    /// Adapter-owned identity persistence failed.
    #[error(transparent)]
    IdentityStore(#[from] IdentityStoreError),
    /// Peer identity was unavailable on a non-silent SAM accept.
    #[error("SAM accept did not provide an authenticated peer Destination")]
    PeerIdentityUnavailable,
    /// The accepted peer was denied by the core access or rate policy.
    #[error("authenticated peer was denied by server policy")]
    AccessDenied,
    /// Service identifier does not name an enabled configured profile.
    #[error("unknown or disabled service profile")]
    UnknownService,
    /// A client profile has no outbound destination.
    #[error("client profile has no outbound Destination reference")]
    DestinationMissing,
    /// SAM cannot currently resolve this future Destination reference form.
    #[error("encrypted-service Destination references are not supported by this SAM profile")]
    EncryptedServiceUnsupported,
    /// Per-service concurrent stream ceiling has been reached.
    #[error("service connection limit reached")]
    ConnectionLimit,
    /// Target is unsupported by this first adapter slice.
    #[error("server target is not loopback TCP")]
    UnsupportedTarget,
    /// Stream or local target I/O failed.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// Configured local target connection deadline expired.
    #[error("local target connection timed out")]
    TargetTimeout,
    /// HTTP request head exceeded the policy core's retained buffer limit.
    #[error("HTTP request head is incomplete or exceeds the retained-byte ceiling")]
    HttpHeadLimit,
    /// Rate-limit state mutex was poisoned; policy fails closed.
    #[error("server rate-limit state is unavailable")]
    RateStateUnavailable,
}

enum StreamEndpoint {
    Dedicated(Arc<StreamSession>),
    Shared(Arc<SharedChild>),
}

impl StreamEndpoint {
    async fn connect(&self, peer: &str) -> Result<SamStream, SamError> {
        match self {
            Self::Dedicated(session) => session.connect(peer, None, None).await,
            Self::Shared(child) => child.connect(peer, None, None).await,
        }
    }

    async fn accept(&self) -> Result<SamStream, SamError> {
        match self {
            Self::Dedicated(session) => session.accept_with(false).await,
            Self::Shared(child) => child.accept_with(false).await,
        }
    }
}

/// A server stream with authenticated peer identity and its connection-limit reservation.
pub struct AuthenticatedStream {
    stream: SamStream,
    peer_hash: [u8; 32],
    _permit: OwnedSemaphorePermit,
}

impl AuthenticatedStream {
    /// Canonical 32-byte SHA-256 hash of the peer Destination authenticated by SAM.
    pub fn peer_hash(&self) -> &[u8; 32] {
        &self.peer_hash
    }
}

/// An outbound SAM stream retaining its per-service concurrency reservation.
pub struct OutboundStream {
    stream: SamStream,
    _permit: OwnedSemaphorePermit,
}

impl OutboundStream {
    /// Borrow the connected SAM byte stream while retaining its connection reservation.
    pub fn stream_mut(&mut self) -> &mut SamStream {
        &mut self.stream
    }
}

/// A caller-provided complete HTTP request head after the pinned core's filters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilteredHttpRequest {
    /// Core-filtered serialized request head.
    pub request: FilteredServerRequest,
    /// Bytes already read after the header terminator, kept separate from the filtered head.
    pub initial_body_bytes: Vec<u8>,
}

/// Owns service-to-SAM session binding and server-side core policy composition.
pub struct SamServiceTunnelAdapter {
    sam: SamClient,
    specs: HashMap<String, ServiceTunnelSpec>,
    endpoints: HashMap<String, StreamEndpoint>,
    // SharedChild only keeps a weak owner link; retain each owner for the full adapter life.
    shared_owners: Vec<SharedSession>,
    limits: HashMap<String, Arc<Semaphore>>,
    rate_limiters: HashMap<String, Mutex<ServerConnectionRateLimiter>>,
    started: Instant,
}

impl SamServiceTunnelAdapter {
    /// Connect to SAM and construct every enabled supported profile before returning.
    ///
    /// The whole set is validated and unsupported profiles are rejected before a SAM
    /// socket is opened. Persistent groups require `identity_store`; new keys are saved
    /// before their group sessions are created.
    pub async fn connect(
        config: i2pr_sam::ClientConfig,
        set: ServiceTunnelSet,
        dialect: SharedDialect,
        identity_store: Option<&dyn DestinationIdentityStore>,
    ) -> Result<Self, AdapterError> {
        let active = supported_active_set(&set)?;
        let groups = active.destination_groups();
        if groups.iter().any(|group| group.persistent) && identity_store.is_none() {
            return Err(AdapterError::IdentityStoreRequired);
        }

        let sam = SamClient::connect(config).await?;
        let specs: HashMap<_, _> = active
            .tunnels
            .into_iter()
            .map(|spec| (spec.id.as_str().to_owned(), spec))
            .collect();
        let mut endpoints = HashMap::new();
        let mut shared_owners = Vec::new();
        let mut limits = HashMap::new();
        let mut rate_limiters = HashMap::new();

        for (id, spec) in &specs {
            limits.insert(id.clone(), Arc::new(Semaphore::new(spec.max_connections)));
            if spec.kind.is_server() {
                rate_limiters.insert(
                    id.clone(),
                    Mutex::new(ServerConnectionRateLimiter::new(
                        spec.access.connection_rates,
                    )),
                );
            }
        }

        for group in groups {
            let identity = if group.persistent {
                let store = identity_store.ok_or(AdapterError::IdentityStoreRequired)?;
                Some(resolve_persistent_identity(&sam, &group, &specs, store).await?)
            } else {
                None
            };
            let destination = identity
                .as_ref()
                .map(|identity| i2pr_sam::SessionDestination::WithKey {
                    public: identity.public().clone(),
                    secret: identity.secret().clone(),
                })
                .unwrap_or(i2pr_sam::SessionDestination::Generated);

            match &group.key {
                DestinationGroupKey::Dedicated(service_id) => {
                    let id = service_id.as_str();
                    let Some(spec) = specs.get(id) else { continue };
                    let options = server_stream_options(spec);
                    let session = sam
                        .create_stream_session(&destination, &session_id("s", id), &options)
                        .await?;
                    endpoints.insert(id.to_owned(), StreamEndpoint::Dedicated(Arc::new(session)));
                }
                DestinationGroupKey::Explicit(group_id) => {
                    let owner = sam
                        .create_shared_session(
                            &destination,
                            &session_id("g", group_id.as_str()),
                            dialect,
                            &[],
                        )
                        .await?;
                    let mut client_child: Option<Arc<SharedChild>> = None;
                    let mut server_ports = std::collections::HashSet::new();
                    for member in &group.members {
                        let id = member.as_str();
                        let Some(spec) = specs.get(id) else { continue };
                        if !spec.kind.is_server() {
                            continue;
                        }
                        let port = spec.inbound_port.unwrap_or(0);
                        if !server_ports.insert(port) {
                            return Err(AdapterError::UnsupportedProfile(
                                "shared server members need unique nonzero inbound ports".into(),
                            ));
                        }
                        let options = server_stream_options(spec);
                        let child = Arc::new(
                            owner
                                .add_child(&session_id("s", id), SessionStyle::Stream, &options)
                                .await?,
                        );
                        if port == 0 && client_child.is_none() {
                            client_child = Some(child.clone());
                        }
                        endpoints.insert(id.to_owned(), StreamEndpoint::Shared(child));
                    }
                    let clients: Vec<_> = group
                        .members
                        .iter()
                        .filter_map(|member| {
                            let id = member.as_str();
                            specs
                                .get(id)
                                .filter(|spec| !spec.kind.is_server())
                                .map(|_| id)
                        })
                        .collect();
                    if !clients.is_empty() {
                        let child = match client_child {
                            Some(child) => child,
                            None => Arc::new(
                                owner
                                    .add_child(
                                        &session_id("c", group_id.as_str()),
                                        SessionStyle::Stream,
                                        &[],
                                    )
                                    .await?,
                            ),
                        };
                        for id in clients {
                            endpoints.insert(id.to_owned(), StreamEndpoint::Shared(child.clone()));
                        }
                    }
                    shared_owners.push(owner);
                }
            }
        }

        Ok(Self {
            sam,
            specs,
            endpoints,
            shared_owners,
            limits,
            rate_limiters,
            started: Instant::now(),
        })
    }

    /// Open an outbound stream for a GenericClient profile to a validated core destination.
    pub async fn open_client_stream(
        &self,
        service_id: &str,
        destination: Option<&DestinationRef>,
    ) -> Result<OutboundStream, AdapterError> {
        let spec = self.spec(service_id)?;
        if spec.kind != ServiceTunnelKind::GenericClient {
            return Err(AdapterError::UnsupportedProfile(
                spec.kind.as_str().to_owned(),
            ));
        }
        let permit = self.acquire(service_id)?;
        let destination = destination
            .or(spec.destination.as_ref())
            .ok_or(AdapterError::DestinationMissing)?;
        let name = match destination {
            DestinationRef::EncryptedService { .. } => {
                return Err(AdapterError::EncryptedServiceUnsupported);
            }
            _ => destination.canonical_string(),
        };
        let peer = self.sam.resolve_peer(&name).await?;
        let endpoint = self
            .endpoints
            .get(service_id)
            .ok_or(AdapterError::UnknownService)?;
        // If connect fails, the local permit is dropped with this stack frame.
        let stream = endpoint.connect(peer.wire_value()).await?;
        Ok(OutboundStream {
            stream,
            _permit: permit,
        })
    }

    /// Accept one server stream, authenticate its peer, and apply core access/rate policy.
    pub async fn accept_server_stream(
        &self,
        service_id: &str,
    ) -> Result<AuthenticatedStream, AdapterError> {
        let spec = self.spec(service_id)?;
        if !spec.kind.is_server() || spec.kind == ServiceTunnelKind::StreamrServer {
            return Err(AdapterError::UnsupportedProfile(
                spec.kind.as_str().to_owned(),
            ));
        }
        let permit = self.acquire(service_id)?;
        let endpoint = self
            .endpoints
            .get(service_id)
            .ok_or(AdapterError::UnknownService)?;
        let stream = endpoint.accept().await?;
        let destination = stream
            .remote_destination()
            .ok_or(AdapterError::PeerIdentityUnavailable)?;
        let hash = *destination_hash(destination)?.as_bytes();
        if !spec.access.allows(&hash) {
            return Err(AdapterError::AccessDenied);
        }
        let limiter = self
            .rate_limiters
            .get(service_id)
            .ok_or(AdapterError::UnknownService)?;
        let mut limiter = limiter
            .lock()
            .map_err(|_| AdapterError::RateStateUnavailable)?;
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if !limiter.admit(hash, now_ms) {
            return Err(AdapterError::AccessDenied);
        }
        drop(limiter);
        Ok(AuthenticatedStream {
            stream,
            peer_hash: hash,
            _permit: permit,
        })
    }

    /// Delegate one complete HTTP server request head to the pinned policy/filter core.
    pub fn filter_http_server_request(
        &self,
        service_id: &str,
        bytes: &[u8],
    ) -> Result<FilteredHttpRequest, AdapterError> {
        let spec = self.spec(service_id)?;
        if spec.kind != ServiceTunnelKind::HttpServer {
            return Err(AdapterError::UnsupportedProfile(
                spec.kind.as_str().to_owned(),
            ));
        }
        let target = match spec.target.as_ref() {
            Some(ServerTarget::LoopbackTcp(address)) => address.to_string(),
            _ => return Err(AdapterError::UnsupportedTarget),
        };
        let head = parse_request_head(bytes, HttpLimits::defaults())
            .map_err(|_| AdapterError::HttpParseRejected)?;
        filter_http_request(&head, &target, &spec.http_policy)
    }

    /// Connect an authenticated accepted server stream to its validated loopback TCP target.
    ///
    /// For HTTP server profiles, the request head is bounded, filtered by the upstream core,
    /// then forwarded with any already-read body bytes. `copy_bidirectional` uses fixed-size
    /// internal buffers and the service's connection permit remains held until completion.
    pub async fn forward_server_stream(
        &self,
        service_id: &str,
        mut accepted: AuthenticatedStream,
    ) -> Result<(), AdapterError> {
        let spec = self.spec(service_id)?;
        let target = match spec.target.as_ref() {
            Some(ServerTarget::LoopbackTcp(address)) => *address,
            _ => return Err(AdapterError::UnsupportedTarget),
        };
        let filtered = if spec.kind == ServiceTunnelKind::HttpServer {
            let limits = HttpLimits::defaults();
            let read_deadline = Duration::from_millis(spec.timeouts.read_timeout_ms);
            let bytes = timeout(read_deadline, read_http_head(&mut accepted.stream, limits))
                .await
                .map_err(|_| AdapterError::HttpHeadLimit)??;
            Some(self.filter_http_server_request(service_id, &bytes)?)
        } else {
            None
        };

        let connect_deadline = Duration::from_millis(spec.timeouts.connect_timeout_ms);
        let mut local = timeout(connect_deadline, TcpStream::connect(target))
            .await
            .map_err(|_| AdapterError::TargetTimeout)??;
        if let Some(filtered) = filtered {
            let write_deadline = Duration::from_millis(spec.timeouts.write_timeout_ms);
            timeout(
                write_deadline,
                local.write_all(&filtered.request.head_bytes),
            )
            .await
            .map_err(|_| {
                AdapterError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "HTTP request-head write timed out",
                ))
            })??;
            timeout(
                write_deadline,
                local.write_all(&filtered.initial_body_bytes),
            )
            .await
            .map_err(|_| {
                AdapterError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "HTTP request-body write timed out",
                ))
            })??;
        }
        bounded_bidirectional_copy(
            &mut accepted.stream,
            &mut local,
            spec.max_buffered_bytes_per_direction,
            Duration::from_millis(spec.timeouts.read_timeout_ms),
            Duration::from_millis(spec.timeouts.write_timeout_ms),
            Duration::from_millis(spec.timeouts.shutdown_timeout_ms),
        )
        .await?;
        Ok(())
    }

    /// Exact core-derived destination-group plan for the enabled adapter set.
    pub fn destination_groups(&self) -> Vec<DestinationGroupSpec> {
        let set = ServiceTunnelSet {
            tunnels: self.specs.values().cloned().collect(),
        };
        set.destination_groups()
    }

    /// Number of retained SAM shared-session owners. This is observable for diagnostics/tests.
    pub fn shared_owner_count(&self) -> usize {
        self.shared_owners.len()
    }

    fn spec(&self, id: &str) -> Result<&ServiceTunnelSpec, AdapterError> {
        self.specs.get(id).ok_or(AdapterError::UnknownService)
    }

    fn acquire(&self, id: &str) -> Result<OwnedSemaphorePermit, AdapterError> {
        self.limits
            .get(id)
            .ok_or(AdapterError::UnknownService)?
            .clone()
            .try_acquire_owned()
            .map_err(|_| AdapterError::ConnectionLimit)
    }
}

/// Call the upstream HTTP server filter with the full pinned service profile.
pub fn filter_http_request(
    head: &HttpRequestHead,
    local_authority: &str,
    policy: &HttpServerPolicy,
) -> Result<FilteredHttpRequest, AdapterError> {
    let initial_body_bytes = head.initial_body_bytes.clone();
    let request = filter_server_request_with_policy(head, local_authority, policy)
        .map_err(|error| AdapterError::HttpPolicy(error.to_string()))?;
    Ok(FilteredHttpRequest {
        request,
        initial_body_bytes,
    })
}

/// Validate a complete set and return the core's exact destination groups for enabled,
/// supported STREAM profiles. Group membership and persistence come only from core policy.
pub fn plan_destination_groups(
    set: &ServiceTunnelSet,
) -> Result<Vec<DestinationGroupSpec>, AdapterError> {
    Ok(supported_active_set(set)?.destination_groups())
}

fn supported_active_set(set: &ServiceTunnelSet) -> Result<ServiceTunnelSet, AdapterError> {
    set.validate()?;
    let enabled: Vec<_> = set
        .tunnels
        .iter()
        .filter(|spec| spec.enabled)
        .cloned()
        .collect();
    for spec in &enabled {
        ensure_supported(spec)?;
        if spec.policy.crypto_policy() != DestinationCryptoPolicy::default() {
            return Err(AdapterError::UnsupportedProfile(format!(
                "{} requests a Destination crypto policy unsupported by the current SAM client",
                spec.kind.as_str()
            )));
        }
    }
    let active = ServiceTunnelSet { tunnels: enabled };
    active.validate()?;
    Ok(active)
}

fn ensure_supported(spec: &ServiceTunnelSpec) -> Result<(), AdapterError> {
    if !matches!(
        spec.kind,
        ServiceTunnelKind::GenericClient
            | ServiceTunnelKind::GenericServer
            | ServiceTunnelKind::HttpServer
    ) {
        return Err(AdapterError::UnsupportedProfile(
            spec.kind.as_str().to_owned(),
        ));
    }
    if matches!(
        spec.policy.ownership(),
        DestinationPolicy::WithCrypto { .. }
    ) && spec.policy.crypto_policy() != DestinationCryptoPolicy::default()
    {
        return Err(AdapterError::UnsupportedProfile(
            "non-default Destination crypto policy".into(),
        ));
    }
    Ok(())
}

fn session_id(prefix: &str, id: &str) -> String {
    format!("{prefix}-{id}")
}

fn server_stream_options(spec: &ServiceTunnelSpec) -> Vec<(String, String)> {
    spec.inbound_port
        .filter(|port| *port != 0)
        .map(|port| vec![("LISTEN_PORT".to_owned(), port.to_string())])
        .unwrap_or_default()
}

fn identity_key(
    group: &DestinationGroupSpec,
    specs: &HashMap<String, ServiceTunnelSpec>,
) -> IdentityKey {
    let key_reference = group.members.iter().find_map(|id| {
        let spec = specs.get(id.as_str())?;
        match spec.policy.ownership() {
            DestinationPolicy::KeyReference(reference) => Some(reference.as_str().to_owned()),
            _ => None,
        }
    });
    IdentityKey {
        group: group.key.clone(),
        key_reference,
    }
}

async fn resolve_persistent_identity(
    sam: &SamClient,
    group: &DestinationGroupSpec,
    specs: &HashMap<String, ServiceTunnelSpec>,
    store: &dyn DestinationIdentityStore,
) -> Result<StoredIdentity, AdapterError> {
    let key = identity_key(group, specs);
    if let Some(identity) = store.load(&key)? {
        return Ok(identity);
    }
    let generated = sam.generate_destination(Some(7)).await?;
    let identity = StoredIdentity::new(generated.public().clone(), generated.secret().clone());
    store.store(&key, &identity)?;
    Ok(identity)
}

async fn read_http_head(
    stream: &mut SamStream,
    limits: HttpLimits,
) -> Result<Vec<u8>, AdapterError> {
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(bytes);
        }
        let remaining = limits.retained_buffer_max_bytes.saturating_sub(bytes.len());
        if remaining == 0 {
            return Err(AdapterError::HttpHeadLimit);
        }
        let read_limit = remaining.min(1024);
        let count = stream.read(&mut chunk[..read_limit]).await?;
        if count == 0 {
            return Err(AdapterError::HttpHeadLimit);
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

async fn bounded_bidirectional_copy<A, B>(
    a: &mut A,
    b: &mut B,
    buffer_bytes: usize,
    read_timeout: Duration,
    write_timeout: Duration,
    shutdown_timeout: Duration,
) -> Result<(), AdapterError>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (a_read, a_write) = tokio::io::split(a);
    let (b_read, b_write) = tokio::io::split(b);
    tokio::try_join!(
        copy_with_deadlines(
            a_read,
            b_write,
            buffer_bytes,
            read_timeout,
            write_timeout,
            shutdown_timeout,
        ),
        copy_with_deadlines(
            b_read,
            a_write,
            buffer_bytes,
            read_timeout,
            write_timeout,
            shutdown_timeout,
        ),
    )?;
    Ok(())
}

async fn copy_with_deadlines<R, W>(
    mut reader: R,
    mut writer: W,
    buffer_bytes: usize,
    read_timeout: Duration,
    write_timeout: Duration,
    shutdown_timeout: Duration,
) -> Result<u64, io::Error>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0_u8; buffer_bytes];
    let mut copied = 0_u64;
    loop {
        let count = timeout(read_timeout, reader.read(&mut buffer))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "stream read timed out"))??;
        if count == 0 {
            timeout(shutdown_timeout, writer.shutdown())
                .await
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "stream shutdown timed out")
                })??;
            return Ok(copied);
        }
        timeout(write_timeout, writer.write_all(&buffer[..count]))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "stream write timed out"))??;
        copied = copied.saturating_add(count as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_sam::ClientConfig;
    use i2pr_service_tunnels::{
        DestinationGroupId, IdlePolicy, LocalListenerSpec, ServerAccessPolicy, ServiceTimeouts,
        Socks5ClientOptions, TunnelShaping,
    };
    use std::{collections::HashMap as StdHashMap, net::IpAddr};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::{TcpListener, TcpStream},
        task::JoinHandle,
    };

    const OWNER: &str = "b3duZXItaWRlbnRpdHktZml4dHVyZS0zMmJ5dGVzIQA=";
    const PEER: &str = "cGVlci1pZGVudGl0eS1maXh0dXJlLTMzYnl0ZXMhIS4u";

    fn client_spec(id: &str, policy: DestinationPolicy) -> ServiceTunnelSpec {
        let port = match id {
            "one" => 19001,
            "two" => 19002,
            "solo" => 19003,
            "first" => 19004,
            "second" => 19005,
            "socks" => 19006,
            _ => 19010,
        };
        ServiceTunnelSpec {
            id: i2pr_service_tunnels::ServiceTunnelId::parse(id).unwrap(),
            kind: ServiceTunnelKind::GenericClient,
            enabled: true,
            listener: Some(
                LocalListenerSpec::parse(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port).unwrap(),
            ),
            target: None,
            targets: Vec::new(),
            destination: Some(DestinationRef::parse("example.i2p").unwrap()),
            policy,
            inbound_port: None,
            max_connections: 4,
            max_buffered_bytes_per_direction: 4096,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: Default::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        }
    }

    fn server_spec(id: &str) -> ServiceTunnelSpec {
        let mut spec = client_spec(id, DestinationPolicy::Dedicated);
        spec.kind = ServiceTunnelKind::GenericServer;
        spec.listener = None;
        spec.target = Some(ServerTarget::parse("127.0.0.1:8080").unwrap());
        spec.destination = None;
        spec.inbound_port = Some(80);
        spec
    }

    fn mock_bridge() -> (std::net::SocketAddr, JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let listener = TcpListener::from_std(listener).unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(handle_mock_connection(socket));
            }
        });
        (address, task)
    }

    async fn handle_mock_connection(socket: TcpStream) {
        let mut reader = BufReader::new(socket);
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let _ = reader
                .get_mut()
                .write_all(b"HELLO REPLY RESULT=OK VERSION=3.3\n")
                .await;
        } else {
            return;
        }
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                return;
            }
            let reply = if line.starts_with("DEST GENERATE") {
                format!("DEST REPLY PUB={OWNER} PRIV=c2VjcmV0LWtleQ==\n")
            } else if line.starts_with("SESSION CREATE") {
                "SESSION STATUS RESULT=OK DESTINATION=c2VjcmV0LWtleQ==\n".to_owned()
            } else if line.starts_with("NAMING LOOKUP NAME=ME") {
                format!("NAMING REPLY RESULT=OK NAME=ME VALUE={OWNER}\n")
            } else if line.starts_with("NAMING LOOKUP") {
                format!("NAMING REPLY RESULT=OK NAME=example.i2p VALUE={OWNER}\n")
            } else if line.starts_with("SESSION ADD") || line.starts_with("SESSION REMOVE") {
                "SESSION STATUS RESULT=OK\n".to_owned()
            } else if line.starts_with("STREAM CONNECT") {
                "STREAM STATUS RESULT=OK\n".to_owned()
            } else if line.starts_with("STREAM ACCEPT") {
                let mut bytes =
                    format!("STREAM STATUS RESULT=OK\n{PEER}\nFROM_PORT=1\nTO_PORT=2\n\n");
                bytes.push_str("payload");
                let _ = reader.get_mut().write_all(bytes.as_bytes()).await;
                return;
            } else {
                return;
            };
            if reader.get_mut().write_all(reply.as_bytes()).await.is_err() {
                return;
            }
        }
    }

    #[derive(Default)]
    struct MemoryStore(Mutex<StdHashMap<IdentityKey, StoredIdentity>>);

    impl DestinationIdentityStore for MemoryStore {
        fn load(&self, key: &IdentityKey) -> Result<Option<StoredIdentity>, IdentityStoreError> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }

        fn store(
            &self,
            key: &IdentityKey,
            identity: &StoredIdentity,
        ) -> Result<(), IdentityStoreError> {
            self.0.lock().unwrap().insert(key.clone(), identity.clone());
            Ok(())
        }
    }

    #[test]
    fn core_group_planner_preserves_explicit_and_dedicated_domains() {
        let shared = DestinationPolicy::SharedGroup(DestinationGroupId::parse("app").unwrap());
        let set = ServiceTunnelSet {
            tunnels: vec![
                client_spec("one", shared.clone()),
                client_spec("two", shared),
                client_spec("solo", DestinationPolicy::Dedicated),
            ],
        };
        let groups = plan_destination_groups(&set).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups
                .iter()
                .find(|g| matches!(g.key, DestinationGroupKey::Explicit(_)))
                .unwrap()
                .members
                .len(),
            2
        );
        assert_eq!(
            groups
                .iter()
                .find(|g| matches!(g.key, DestinationGroupKey::Dedicated(_)))
                .unwrap()
                .members
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn unsupported_and_persistent_profiles_fail_before_sam_connect() {
        let mut socks = client_spec("socks", DestinationPolicy::Dedicated);
        socks.kind = ServiceTunnelKind::Socks5Client;
        socks.socks5_options = Some(Socks5ClientOptions::defaults());
        let config = ClientConfig::new("127.0.0.1:0".parse().unwrap());
        assert!(matches!(
            SamServiceTunnelAdapter::connect(
                config.clone(),
                ServiceTunnelSet {
                    tunnels: vec![socks]
                },
                SharedDialect::Master,
                None
            )
            .await,
            Err(AdapterError::UnsupportedProfile(_))
        ));
        assert!(matches!(
            SamServiceTunnelAdapter::connect(
                config,
                ServiceTunnelSet {
                    tunnels: vec![server_spec("server")]
                },
                SharedDialect::Master,
                None
            )
            .await,
            Err(AdapterError::IdentityStoreRequired)
        ));
    }

    #[tokio::test]
    async fn shared_client_group_uses_one_owner_and_connects_over_sam() {
        let (endpoint, bridge) = mock_bridge();
        let shared = DestinationPolicy::SharedGroup(DestinationGroupId::parse("shared").unwrap());
        let set = ServiceTunnelSet {
            tunnels: vec![
                client_spec("first", shared.clone()),
                client_spec("second", shared),
            ],
        };
        let adapter = SamServiceTunnelAdapter::connect(
            ClientConfig::new(endpoint),
            set,
            SharedDialect::Master,
            None,
        )
        .await
        .unwrap();
        assert_eq!(adapter.shared_owner_count(), 1);
        assert_eq!(adapter.destination_groups()[0].members.len(), 2);
        let stream = adapter.open_client_stream("first", None).await.unwrap();
        drop(stream);
        bridge.abort();
    }

    #[tokio::test]
    async fn server_accept_maps_authenticated_destination_hash() {
        let (endpoint, bridge) = mock_bridge();
        let adapter = SamServiceTunnelAdapter::connect(
            ClientConfig::new(endpoint),
            ServiceTunnelSet {
                tunnels: vec![server_spec("server")],
            },
            SharedDialect::Master,
            Some(&MemoryStore::default()),
        )
        .await
        .unwrap();
        let accepted = adapter.accept_server_stream("server").await.unwrap();
        let expected = destination_hash(&Destination::new(PEER).unwrap()).unwrap();
        assert_eq!(accepted.peer_hash(), expected.as_bytes());
        drop(accepted);
        bridge.abort();
    }

    #[test]
    fn http_server_request_filter_is_delegated_to_core() {
        let bytes = b"GET / HTTP/1.1\r\nHost: attacker.i2p\r\nReferer: secret.i2p\r\n\r\nbody";
        let head = parse_request_head(bytes, HttpLimits::defaults()).unwrap();
        let filtered =
            filter_http_request(&head, "127.0.0.1:8080", &HttpServerPolicy::default()).unwrap();
        let output = String::from_utf8_lossy(&filtered.request.head_bytes).to_ascii_lowercase();
        assert!(output.contains("host: 127.0.0.1:8080"), "{output:?}");
        assert!(!output.contains("referer:"));
        assert_eq!(filtered.initial_body_bytes, b"body");
    }
}
