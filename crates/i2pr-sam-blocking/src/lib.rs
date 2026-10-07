//! Synchronous facade that drives the canonical async client on an owned Tokio runtime.

use i2pr_sam::{
    ClientConfig, ConnectRetryPolicy, DatagramSession, DatagramTransport, GeneratedDestination,
    PeerTarget, SamCapabilities, SamClient, SamError, SamStream, SessionDestination,
    SessionIdentity, SharedSession, StreamSession,
};
use i2pr_sam_proto::{Port, ReceivedDatagram, SessionStyle, SharedDialect};
use std::{io, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    runtime::{Builder, Runtime},
    time::timeout,
};

#[derive(Debug, thiserror::Error)]
pub enum BlockingError {
    #[error("blocking SAM calls cannot run inside a Tokio runtime")]
    NestedRuntime,
    #[error("blocking facade setup failed: {0}")]
    Runtime(#[from] io::Error),
    #[error(transparent)]
    Sam(#[from] SamError),
    #[error("blocking stream operation timed out")]
    Timeout,
}

fn ensure_blocking_context() -> Result<(), BlockingError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        Err(BlockingError::NestedRuntime)
    } else {
        Ok(())
    }
}

/// Build the dedicated runtime this facade drives the async client on.
fn owned_runtime() -> Result<Arc<Runtime>, BlockingError> {
    Ok(Arc::new(
        Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()?,
    ))
}

pub struct BlockingClient {
    runtime: Arc<Runtime>,
    client: SamClient,
}

impl BlockingClient {
    pub fn connect(config: ClientConfig) -> Result<Self, BlockingError> {
        let runtime = owned_runtime()?;
        let client = runtime.block_on(SamClient::connect(config))?;
        Ok(Self { runtime, client })
    }

    /// Connect with a bounded initial retry policy, mirroring the async client exactly.
    pub fn connect_with_policy(
        config: ClientConfig,
        policy: ConnectRetryPolicy,
    ) -> Result<Self, BlockingError> {
        ensure_blocking_context()?;
        let runtime = owned_runtime()?;
        let client = runtime.block_on(SamClient::connect_with_policy(config, policy))?;
        Ok(Self { runtime, client })
    }

    pub fn connect_endpoint(endpoint: SocketAddr) -> Result<Self, BlockingError> {
        Self::connect(ClientConfig::new(endpoint))
    }

    pub fn capabilities(&self) -> Result<SamCapabilities, BlockingError> {
        ensure_blocking_context()?;
        Ok(self.runtime.block_on(self.client.capabilities()))
    }

    /// Resolve the concrete identity of a session this client controls.
    pub fn session_identity(&self) -> Result<SessionIdentity, BlockingError> {
        ensure_blocking_context()?;
        Ok(self.runtime.block_on(self.client.session_identity())?)
    }

    pub fn resolve_peer(&self, name: &str) -> Result<PeerTarget, BlockingError> {
        ensure_blocking_context()?;
        Ok(self.runtime.block_on(self.client.resolve_peer(name))?)
    }

    /// Returns a typed pair, never a bare `(String, SecretDestination)` tuple.
    pub fn generate_destination(
        &self,
        signature_type: Option<u16>,
    ) -> Result<GeneratedDestination, BlockingError> {
        ensure_blocking_context()?;
        Ok(self
            .runtime
            .block_on(self.client.generate_destination(signature_type))?)
    }

    pub fn lookup(&self, name: &str) -> Result<String, BlockingError> {
        ensure_blocking_context()?;
        Ok(self.runtime.block_on(self.client.lookup(name))?)
    }

    pub fn create_stream_session(
        &self,
        destination: &SessionDestination,
        id: &str,
        options: &[(String, String)],
    ) -> Result<BlockingStreamSession, BlockingError> {
        ensure_blocking_context()?;
        let session =
            self.runtime
                .block_on(self.client.create_stream_session(destination, id, options))?;
        Ok(BlockingStreamSession {
            runtime: self.runtime.clone(),
            session,
            io_timeout: Duration::from_secs(300),
        })
    }

    /// Ordinary datagram session over the SAM datagram port with UDP forwarding.
    pub fn create_datagram_session(
        &self,
        destination: &SessionDestination,
        id: &str,
        style: SessionStyle,
        options: &[(String, String)],
    ) -> Result<BlockingDatagramSession, BlockingError> {
        self.create_datagram_session_with(
            destination,
            id,
            style,
            options,
            DatagramTransport::UdpForward,
        )
    }

    /// Ordinary datagram session over an explicit transport.
    ///
    /// The async client rejects the v1/v2-compatible control-socket transport for
    /// DATAGRAM2/DATAGRAM3 and for shared subsessions; this facade surfaces that refusal
    /// unchanged rather than silently falling back to UDP forwarding.
    pub fn create_datagram_session_with(
        &self,
        destination: &SessionDestination,
        id: &str,
        style: SessionStyle,
        options: &[(String, String)],
        transport: DatagramTransport,
    ) -> Result<BlockingDatagramSession, BlockingError> {
        ensure_blocking_context()?;
        let session = self
            .runtime
            .block_on(self.client.create_datagram_session_with(
                destination,
                id,
                style,
                transport,
                options,
            ))?;
        Ok(BlockingDatagramSession {
            runtime: self.runtime.clone(),
            session,
            recv_timeout: Duration::from_secs(300),
        })
    }

    pub fn create_shared_session(
        &self,
        destination: &SessionDestination,
        id: &str,
        dialect: SharedDialect,
        options: &[(String, String)],
    ) -> Result<BlockingSharedSession, BlockingError> {
        ensure_blocking_context()?;
        let session = self.runtime.block_on(self.client.create_shared_session(
            destination,
            id,
            dialect,
            options,
        ))?;
        Ok(BlockingSharedSession {
            runtime: self.runtime.clone(),
            session,
        })
    }
}

pub struct BlockingStreamSession {
    runtime: Arc<Runtime>,
    session: StreamSession,
    io_timeout: Duration,
}
impl BlockingStreamSession {
    pub fn set_io_timeout(&mut self, timeout: Duration) {
        self.io_timeout = timeout;
    }
    pub fn connect(
        &self,
        destination: &str,
        from_port: Option<Port>,
        to_port: Option<Port>,
    ) -> Result<BlockingStream, BlockingError> {
        ensure_blocking_context()?;
        let stream =
            self.runtime
                .block_on(self.session.connect(destination, from_port, to_port))?;
        Ok(BlockingStream {
            runtime: self.runtime.clone(),
            stream,
            io_timeout: self.io_timeout,
        })
    }
    pub fn accept(&self) -> Result<BlockingStream, BlockingError> {
        self.accept_with(false)
    }

    /// Accept with an explicit `SILENT` choice; non-silent accepts capture the peer.
    pub fn accept_with(&self, silent: bool) -> Result<BlockingStream, BlockingError> {
        ensure_blocking_context()?;
        let stream = self.runtime.block_on(self.session.accept_with(silent))?;
        Ok(BlockingStream {
            runtime: self.runtime.clone(),
            stream,
            io_timeout: self.io_timeout,
        })
    }

    /// The session's concrete identity, when the router exposed one.
    pub fn identity(&self) -> Option<&SessionIdentity> {
        self.session.identity()
    }

    pub fn close(&self) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(self.session.close());
        Ok(())
    }
}

pub struct BlockingStream {
    runtime: Arc<Runtime>,
    stream: SamStream,
    io_timeout: Duration,
}
impl BlockingStream {
    /// Authenticated peer announced by a non-silent accept.
    pub fn peer(&self) -> Option<&i2pr_sam::StreamPeer> {
        self.stream.peer()
    }

    pub fn remote_destination(&self) -> Option<&i2pr_sam_proto::Destination> {
        self.stream.remote_destination()
    }

    pub fn set_io_timeout(&mut self, timeout: Duration) {
        self.io_timeout = timeout;
    }
}
impl io::Read for BlockingStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        ensure_blocking_context().map_err(|e| io::Error::other(e.to_string()))?;
        self.runtime.block_on(async {
            timeout(self.io_timeout, self.stream.read(buf))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "SAM read timed out"))?
        })
    }
}
impl io::Write for BlockingStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        ensure_blocking_context().map_err(|e| io::Error::other(e.to_string()))?;
        self.runtime.block_on(async {
            timeout(self.io_timeout, self.stream.write(buf))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "SAM write timed out"))?
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        ensure_blocking_context().map_err(|e| io::Error::other(e.to_string()))?;
        self.runtime.block_on(async {
            timeout(self.io_timeout, self.stream.flush())
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "SAM flush timed out"))?
        })
    }
}

pub struct BlockingDatagramSession {
    runtime: Arc<Runtime>,
    session: DatagramSession,
    recv_timeout: Duration,
}
impl BlockingDatagramSession {
    pub fn set_recv_timeout(&mut self, duration: Duration) {
        self.recv_timeout = duration;
    }
    pub fn send(
        &self,
        destination: &str,
        payload: &[u8],
        from_port: Option<Port>,
        to_port: Option<Port>,
    ) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime
            .block_on(self.session.send(destination, payload, from_port, to_port))?;
        Ok(())
    }
    /// Transport in use, so callers can assert which mode a session runs.
    pub fn transport(&self) -> DatagramTransport {
        self.session.transport()
    }

    pub fn style(&self) -> SessionStyle {
        self.session.style()
    }

    pub fn identity(&self) -> Option<&SessionIdentity> {
        self.session.identity()
    }

    /// Deliveries dropped because the bounded queue was full.
    pub fn dropped_datagrams(&self) -> Result<u64, BlockingError> {
        ensure_blocking_context()?;
        Ok(self.runtime.block_on(self.session.dropped_datagrams()))
    }

    pub fn recv(&self) -> Result<ReceivedDatagram, BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(async {
            timeout(self.recv_timeout, self.session.recv())
                .await
                .map_err(|_| BlockingError::Timeout)?
                .map_err(BlockingError::Sam)
        })
    }
    pub fn close(&self) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(self.session.close());
        Ok(())
    }
}

pub struct BlockingSharedSession {
    runtime: Arc<Runtime>,
    session: SharedSession,
}
impl BlockingSharedSession {
    /// The concrete Destination every child of this session shares.
    pub fn identity(&self) -> &SessionIdentity {
        self.session.identity()
    }

    pub fn dialect(&self) -> SharedDialect {
        self.session.dialect()
    }

    pub fn add_child(
        &self,
        id: &str,
        style: SessionStyle,
        options: &[(String, String)],
    ) -> Result<BlockingSharedChild, BlockingError> {
        ensure_blocking_context()?;
        let child = self
            .runtime
            .block_on(self.session.add_child(id, style, options))?;
        Ok(BlockingSharedChild {
            runtime: self.runtime.clone(),
            child,
            io_timeout: Duration::from_secs(300),
            recv_timeout: Duration::from_secs(300),
        })
    }
    pub fn remove_child(&self, id: &str) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(self.session.remove_child(id))?;
        Ok(())
    }
    pub fn close(&self) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(self.session.close());
        Ok(())
    }
}

pub struct BlockingSharedChild {
    runtime: Arc<Runtime>,
    child: i2pr_sam::SharedChild,
    io_timeout: Duration,
    recv_timeout: Duration,
}
impl BlockingSharedChild {
    pub fn id(&self) -> &str {
        self.child.id()
    }
    /// The owner's concrete Destination; a child never has an identity of its own.
    pub fn identity(&self) -> &SessionIdentity {
        self.child.identity()
    }
    pub fn connect(
        &self,
        destination: &str,
        from_port: Option<Port>,
        to_port: Option<Port>,
    ) -> Result<BlockingStream, BlockingError> {
        ensure_blocking_context()?;
        let stream = self
            .runtime
            .block_on(self.child.connect(destination, from_port, to_port))?;
        Ok(BlockingStream {
            runtime: self.runtime.clone(),
            stream,
            io_timeout: self.io_timeout,
        })
    }
    pub fn accept(&self) -> Result<BlockingStream, BlockingError> {
        self.accept_with(false)
    }

    /// Accept with an explicit `SILENT` choice.
    pub fn accept_with(&self, silent: bool) -> Result<BlockingStream, BlockingError> {
        ensure_blocking_context()?;
        let stream = self.runtime.block_on(self.child.accept_with(silent))?;
        Ok(BlockingStream {
            runtime: self.runtime.clone(),
            stream,
            io_timeout: self.io_timeout,
        })
    }
    pub fn set_recv_timeout(&mut self, duration: Duration) {
        self.recv_timeout = duration;
    }
    pub fn send_datagram(
        &self,
        destination: &str,
        payload: &[u8],
        from_port: Option<Port>,
        to_port: Option<Port>,
    ) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(self.child.send_datagram(
            destination,
            payload,
            from_port,
            to_port,
        ))?;
        Ok(())
    }
    pub fn recv_datagram(&self) -> Result<ReceivedDatagram, BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(async {
            timeout(self.recv_timeout, self.child.recv_datagram())
                .await
                .map_err(|_| BlockingError::Timeout)?
                .map_err(BlockingError::Sam)
        })
    }
    pub fn close(&self) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime.block_on(self.child.close());
        Ok(())
    }
}
