//! Synchronous facade that drives the canonical async client on an owned Tokio runtime.

use i2pr_sam::{
    ClientConfig, DatagramSession, SamClient, SamError, SamStream, SharedSession, StreamSession,
};
use i2pr_sam_proto::{ReceivedDatagram, SessionStyle, SharedDialect};
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

pub struct BlockingClient {
    runtime: Arc<Runtime>,
    client: SamClient,
}

impl BlockingClient {
    pub fn connect(config: ClientConfig) -> Result<Self, BlockingError> {
        ensure_blocking_context()?;
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()?,
        );
        let client = runtime.block_on(SamClient::connect(config))?;
        Ok(Self { runtime, client })
    }

    pub fn connect_endpoint(endpoint: SocketAddr) -> Result<Self, BlockingError> {
        Self::connect(ClientConfig::new(endpoint))
    }

    pub fn generate_destination(
        &self,
        signature_type: Option<u16>,
    ) -> Result<(String, i2pr_sam::SecretDestination), BlockingError> {
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
        destination: &str,
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

    pub fn create_datagram_session(
        &self,
        destination: &str,
        id: &str,
        style: SessionStyle,
        options: &[(String, String)],
    ) -> Result<BlockingDatagramSession, BlockingError> {
        ensure_blocking_context()?;
        let session = self.runtime.block_on(self.client.create_datagram_session(
            destination,
            id,
            style,
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
        destination: &str,
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
        from_port: Option<u16>,
        to_port: Option<u16>,
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
        ensure_blocking_context()?;
        let stream = self.runtime.block_on(self.session.accept())?;
        Ok(BlockingStream {
            runtime: self.runtime.clone(),
            stream,
            io_timeout: self.io_timeout,
        })
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
        from_port: Option<u16>,
        to_port: Option<u16>,
    ) -> Result<(), BlockingError> {
        ensure_blocking_context()?;
        self.runtime
            .block_on(self.session.send(destination, payload, from_port, to_port))?;
        Ok(())
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
    pub fn connect(
        &self,
        destination: &str,
        from_port: Option<u16>,
        to_port: Option<u16>,
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
        ensure_blocking_context()?;
        let stream = self.runtime.block_on(self.child.accept())?;
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
        from_port: Option<u16>,
        to_port: Option<u16>,
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
