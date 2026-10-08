//! Control-socket framing primitives.
//!
//! Two wire problems are solved here. First, a non-silent `STREAM ACCEPT` reply is followed
//! by a peer identity block before any payload byte, so payload framing must not start at
//! the status line. Second, ordinary DATAGRAM1 and RAW sessions without a forwarding
//! `PORT` receive data as unsolicited `DATAGRAM RECEIVED` / `RAW RECEIVED` lines on the same
//! socket that carries command replies, so replies and deliveries must be demultiplexed by
//! one reader.

use i2pr_sam_proto::{
    AuthenticatedDatagram, DatagramDelivery, Destination, I2pProtocol, IncomingKind, Line, Port,
    RawDatagram, ReceivedDatagram, SessionStyle, parse_line,
};
use std::{collections::VecDeque, str, sync::Arc, time::Duration};

/// One command's pending reply slot.
type PendingReply = oneshot::Sender<Result<Line, SamError>>;
type WaiterQueue = VecDeque<PendingReply>;

use tokio::{
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
        BufReader,
    },
    net::TcpStream,
    sync::{Mutex, Notify, oneshot},
    time::timeout,
};

use crate::SamError;

/// Bounded receive queue for control-socket datagram deliveries.
///
/// The router may deliver faster than the caller consumes, so the queue is bounded by both
/// message count and byte count. Overflow drops the newest delivery and is counted; it never
/// grows memory and never silently reorders or duplicates another operation's frame.
#[derive(Debug)]
pub struct DatagramInbox {
    queue: Mutex<InboxState>,
    notify: Notify,
    capacity: usize,
    max_bytes: usize,
}

#[derive(Debug, Default)]
struct InboxState {
    queue: VecDeque<ReceivedDatagram>,
    bytes: usize,
    dropped: u64,
}

impl DatagramInbox {
    pub fn new(capacity: usize, max_bytes: usize) -> Self {
        Self {
            queue: Mutex::new(InboxState::default()),
            notify: Notify::new(),
            capacity,
            max_bytes,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    fn payload_len(datagram: &ReceivedDatagram) -> usize {
        match datagram {
            ReceivedDatagram::Authenticated(message) => message.payload.len(),
            ReceivedDatagram::Unverified(message) => message.payload.len(),
            ReceivedDatagram::Raw(message) => message.payload.len(),
        }
    }

    async fn push(&self, datagram: ReceivedDatagram) {
        let len = Self::payload_len(&datagram);
        {
            let mut state = self.queue.lock().await;
            let would_exceed = state.queue.len() >= self.capacity
                || state.bytes.saturating_add(len) > self.max_bytes;
            if would_exceed {
                state.dropped = state.dropped.saturating_add(1);
                crate::resource::datagrams_dropped(1);
                return;
            }
            state.bytes = state.bytes.saturating_add(len);
            state.queue.push_back(datagram);
        }
        self.notify.notify_one();
    }

    pub async fn dropped(&self) -> u64 {
        self.queue.lock().await.dropped
    }

    pub async fn len(&self) -> usize {
        self.queue.lock().await.queue.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.queue.is_empty()
    }

    /// Take the oldest delivery, waiting until one arrives or `closed` fires.
    pub async fn recv(&self, closed: &Notify) -> Option<ReceivedDatagram> {
        loop {
            {
                let mut state = self.queue.lock().await;
                if let Some(datagram) = state.queue.pop_front() {
                    state.bytes = state.bytes.saturating_sub(Self::payload_len(&datagram));
                    return Some(datagram);
                }
            }
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = closed.notified() => return None,
            }
        }
    }

    pub async fn wake_all(&self) {
        self.notify.notify_waiters();
    }
}

/// Read one `\n`-terminated line, bounded by `ceiling` bytes.
pub async fn read_line_bounded<R: AsyncBufRead + Unpin>(
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

/// Read exactly `size` payload bytes, bounded by `ceiling`.
pub async fn read_exact_bounded<R: AsyncRead + Unpin>(
    reader: &mut R,
    size: usize,
    ceiling: usize,
) -> Result<Vec<u8>, SamError> {
    if size == 0 || size > ceiling {
        return Err(SamError::Rejected(
            "control-socket payload size outside bounds".into(),
        ));
    }
    let mut payload = vec![0; size];
    let mut filled = 0;
    while filled < size {
        let read = reader.read(&mut payload[filled..]).await?;
        if read == 0 {
            return Err(SamError::Closed);
        }
        filled += read;
    }
    Ok(payload)
}

/// Field names a router uses in the STREAM accept block. A peer Destination can never start
/// with one of these, because that would make the token far too short to be a Destination.
const SAM_FIELD_NAMES: [&str; 8] = [
    "FROM_PORT",
    "TO_PORT",
    "PROTOCOL",
    "DESTINATION",
    "EXPIRES",
    "SESSION",
    "MESSAGE",
    "RESULT",
];

/// True when the token is a bare protocol token rather than a `KEY=VALUE` field line.
fn looks_like_bare_token(text: &str) -> bool {
    if text.is_empty() || text.bytes().any(|b| b.is_ascii_whitespace()) {
        return false;
    }
    match text.split_once('=') {
        Some((key, _)) => !SAM_FIELD_NAMES.contains(&key),
        None => true,
    }
}

/// Authenticated peer identity announced by a non-silent `STREAM ACCEPT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamPeer {
    pub destination: Destination,
    pub from_port: Option<Port>,
    pub to_port: Option<Port>,
}

/// Consume the peer identity block that precedes payload on a non-silent `STREAM ACCEPT`.
///
/// The specification sends `$destination`, then optional `FROM_PORT=`/`TO_PORT=` lines, then
/// a blank line. Deployed Java I2P instead folds the port fields onto the destination line
/// (`$destination FROM_PORT=0 TO_PORT=0`), so inline fields are accepted there too. Any line
/// that is not part of that block is pushed back to the payload reader so a router that
/// omits the terminator can never corrupt the first payload bytes. The continuation check
/// works on bytes: live payload is binary and usually not valid UTF-8, so decoding before
/// the blank check would mistake it for an empty terminator and swallow those bytes.
pub async fn read_stream_peer<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    deadline: Duration,
    ceiling: usize,
) -> Result<(StreamPeer, Vec<u8>), SamError> {
    let line = timeout(deadline, read_line_bounded(reader, ceiling))
        .await
        .map_err(|_| SamError::Timeout)??;
    if line.is_empty() {
        return Err(SamError::Closed);
    }
    let text = str::from_utf8(&line)
        .map_err(|_| SamError::Rejected("stream peer identity is not UTF-8".into()))?
        .trim_end_matches(['\r', '\n']);
    // A real Destination is standard base64 and therefore ends in `=` padding, so a naive
    // "contains `=` therefore it is not a Destination" test rejects exactly the shape a
    // router really sends. What separates the two is whether the token begins with a known
    // SAM field name, which a Destination never does.
    let mut tokens = text.split_whitespace();
    let destination_token =
        tokens
            .next()
            .filter(|token| looks_like_bare_token(token))
            .ok_or_else(|| {
                SamError::Rejected("stream accept did not announce a peer Destination".into())
            })?;
    let mut peer = StreamPeer {
        destination: Destination::new(destination_token)
            .map_err(|_| SamError::Rejected("stream peer Destination is invalid".into()))?,
        from_port: None,
        to_port: None,
    };
    for token in tokens {
        let (key, value) = token.split_once('=').ok_or_else(|| {
            SamError::Rejected("stream peer identity line is malformed".into())
        })?;
        let parsed = value.parse::<u16>().map(Port::new);
        match (key, parsed) {
            ("FROM_PORT", Ok(port)) => peer.from_port = Some(port),
            ("TO_PORT", Ok(port)) => peer.to_port = Some(port),
            _ => {
                return Err(SamError::Rejected(
                    "stream peer identity line is malformed".into(),
                ));
            }
        }
    }
    let mut pushed_back = Vec::new();
    loop {
        let line = match timeout(deadline, read_line_bounded(reader, ceiling)).await {
            Ok(result) => result?,
            Err(_) => break,
        };
        let content = strip_line_ending(&line);
        if content.is_empty() {
            break;
        }
        let text = match str::from_utf8(content) {
            Ok(text) => text,
            Err(_) => {
                pushed_back.extend_from_slice(&line);
                break;
            }
        };
        let (key, value) = match text.split_once('=') {
            Some(pair) => pair,
            None => {
                pushed_back.extend_from_slice(&line);
                break;
            }
        };
        let parsed = value.parse::<u16>().map(Port::new);
        match (key, parsed) {
            ("FROM_PORT", Ok(port)) => peer.from_port = Some(port),
            ("TO_PORT", Ok(port)) => peer.to_port = Some(port),
            // Not part of the peer block: hand it back to the payload reader untouched.
            _ => {
                pushed_back.extend_from_slice(&line);
                break;
            }
        }
    }
    Ok((peer, pushed_back))
}

/// The line without its trailing newline; a blank line yields an empty slice.
fn strip_line_ending(line: &[u8]) -> &[u8] {
    let mut content: &[u8] = line;
    while content.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        content = &content[..content.len() - 1];
    }
    content
}

/// Split a `Control` connection into a demultiplexing datagram link.
pub struct ControlDatagramLink {
    id: String,
    style: SessionStyle,
    writer: Mutex<tokio::net::tcp::OwnedWriteHalf>,
    waiters: Arc<Mutex<WaiterQueue>>,
    inbox: Arc<DatagramInbox>,
    closed: Arc<tokio::sync::Notify>,
    dead: Arc<std::sync::atomic::AtomicBool>,
    reader: Mutex<Option<tokio::task::JoinHandle<()>>>,
    control_timeout: Duration,
    max_frame_bytes: usize,
    max_datagram_bytes: usize,
    unexpected: Arc<std::sync::atomic::AtomicU64>,
}

impl ControlDatagramLink {
    #[allow(clippy::too_many_arguments)]
    /// `leftover` carries bytes the request/response phase had already buffered. Dropping
    /// them would silently lose a delivery the router sent before the session switched to
    /// demultiplexed mode, so they are replayed ahead of the socket's own bytes.
    pub fn new(
        id: String,
        style: SessionStyle,
        stream: TcpStream,
        leftover: Vec<u8>,
        control_timeout: Duration,
        max_frame_bytes: usize,
        max_datagram_bytes: usize,
        max_inbox_messages: usize,
        max_inbox_bytes: usize,
    ) -> Self {
        crate::resource::socket_opened();
        let (read_half, write_half) = stream.into_split();
        let inbox = Arc::new(DatagramInbox::new(max_inbox_messages, max_inbox_bytes));
        let waiters = Arc::new(Mutex::new(VecDeque::new()));
        let closed = Arc::new(Notify::new());
        let dead = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let unexpected = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let reader = tokio::spawn(read_loop(
            Prefixed::new(leftover, BufReader::new(read_half)),
            inbox.clone(),
            waiters.clone(),
            closed.clone(),
            dead.clone(),
            unexpected.clone(),
            max_frame_bytes,
            max_datagram_bytes,
            max_inbox_messages,
            max_inbox_bytes,
        ));
        Self {
            id,
            style,
            writer: Mutex::new(write_half),
            waiters,
            inbox,
            closed,
            dead,
            reader: Mutex::new(Some(reader)),
            control_timeout,
            max_frame_bytes,
            max_datagram_bytes,
            unexpected,
        }
    }

    fn is_dead(&self) -> bool {
        self.dead.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Send a command line and await the matching reply.
    pub async fn command(&self, command: &str) -> Result<Line, SamError> {
        if self.is_dead() {
            return Err(SamError::Closed);
        }
        let (sender, receiver) = oneshot::channel();
        self.waiters.lock().await.push_back(sender);
        if let Err(error) = self.write_line(command).await {
            self.fail_waiters(&error).await;
            return Err(error);
        }
        match timeout(self.control_timeout, receiver).await {
            Ok(Ok(reply)) => reply,
            Ok(Err(_)) => Err(SamError::Closed),
            Err(_) => {
                self.fail_waiters(&SamError::Timeout).await;
                Err(SamError::Timeout)
            }
        }
    }

    async fn write_line(&self, command: &str) -> Result<(), SamError> {
        if command.len() > self.max_frame_bytes
            || !command.ends_with('\n')
            || command.bytes().filter(|b| *b == b'\n').count() != 1
            || command.contains('\r')
        {
            return Err(SamError::Protocol(i2pr_sam_proto::ParseError::TooLong));
        }
        let mut writer = self.writer.lock().await;
        writer.write_all(command.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Send one direct datagram, written as a single frame to avoid interleaving.
    ///
    /// SAM v1 sends carry no reply: the router answers nothing, so the send completes once
    /// the bytes reach the socket. Delivery is proven by receipt, never by a reply line,
    /// which is also why no waiter is registered here — an unsolicited later line must
    /// never be mistaken for this send's answer.
    pub async fn send_payload(&self, command: &str, payload: &[u8]) -> Result<(), SamError> {
        if payload.is_empty() || payload.len() > self.max_datagram_bytes {
            return Err(SamError::Rejected(
                "datagram payload outside configured bounds".into(),
            ));
        }
        if self.is_dead() {
            return Err(SamError::Closed);
        }
        let write_result = {
            let mut writer = self.writer.lock().await;
            match writer.write_all(command.as_bytes()).await {
                Ok(()) => writer.write_all(payload).await,
                Err(error) => Err(error),
            }
        };
        if let Err(error) = write_result {
            return Err(SamError::Io(error));
        }
        if let Err(error) = self.writer.lock().await.flush().await {
            return Err(SamError::Io(error));
        }
        Ok(())
    }

    async fn fail_waiters(&self, error: &SamError) {
        let mut waiters = self.waiters.lock().await;
        for waiter in waiters.drain(..) {
            let _ = waiter.send(Err(reusable(error)));
        }
    }

    /// Take the oldest delivered datagram, or `Closed` when the session ends.
    pub async fn recv_datagram(&self) -> Result<ReceivedDatagram, SamError> {
        self.inbox.recv(&self.closed).await.ok_or(SamError::Closed)
    }

    pub async fn dropped_datagrams(&self) -> u64 {
        self.inbox.dropped().await
    }

    pub async fn queued_datagrams(&self) -> usize {
        self.inbox.len().await
    }

    /// Lines the router sent that answered no outstanding command.
    pub fn unexpected_lines(&self) -> u64 {
        self.unexpected.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn style(&self) -> SessionStyle {
        self.style
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Stop the reader and release the socket exactly once.
    pub async fn close(&self) {
        self.dead.store(true, std::sync::atomic::Ordering::Release);
        self.closed.notify_waiters();
        self.inbox.wake_all().await;
        self.fail_waiters(&SamError::Closed).await;
        if let Some(handle) = self.reader.lock().await.take() {
            handle.abort();
            let _ = handle.await;
        }
        let mut writer = self.writer.lock().await;
        let _ = writer.shutdown().await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn read_loop(
    socket: Prefixed<BufReader<tokio::net::tcp::OwnedReadHalf>>,
    inbox: Arc<DatagramInbox>,
    waiters: Arc<Mutex<WaiterQueue>>,
    closed: Arc<Notify>,
    dead: Arc<std::sync::atomic::AtomicBool>,
    unexpected: Arc<std::sync::atomic::AtomicU64>,
    max_frame_bytes: usize,
    max_datagram_bytes: usize,
    inbox_capacity: usize,
    inbox_bytes: usize,
) {
    let mut reader = socket;
    let _ = (inbox_capacity, inbox_bytes);
    loop {
        let raw = match read_line_bounded(&mut reader, max_frame_bytes).await {
            Ok(raw) => raw,
            Err(_error) => {
                // Transport failures end the session exactly like EOF: every waiter and
                // every queued delivery learns the link is closed.
                dead.store(true, std::sync::atomic::Ordering::Release);
                closed.notify_waiters();
                inbox.wake_all().await;
                let mut pending = waiters.lock().await;
                for waiter in pending.drain(..) {
                    let _ = waiter.send(Err(SamError::Closed));
                }
                return;
            }
        };
        if raw.is_empty() {
            dead.store(true, std::sync::atomic::Ordering::Release);
            closed.notify_waiters();
            inbox.wake_all().await;
            let mut pending = waiters.lock().await;
            for waiter in pending.drain(..) {
                let _ = waiter.send(Err(SamError::Closed));
            }
            return;
        }
        let line = match parse_line(&raw) {
            Ok(line) => line,
            Err(_) => continue,
        };
        match IncomingKind::classify(&line) {
            IncomingKind::DatagramDelivery | IncomingKind::RawDelivery => {
                let raw_mode = IncomingKind::classify(&line) == IncomingKind::RawDelivery;
                let header = if raw_mode {
                    DatagramDelivery::parse_raw(&line)
                } else {
                    DatagramDelivery::parse_datagram(&line)
                };
                let header = match header {
                    Ok(header) => header,
                    Err(_) => continue,
                };
                let payload =
                    match read_exact_bounded(&mut reader, header.size, max_datagram_bytes).await {
                        Ok(payload) => payload,
                        Err(_) => {
                            dead.store(true, std::sync::atomic::Ordering::Release);
                            closed.notify_waiters();
                            inbox.wake_all().await;
                            let mut pending = waiters.lock().await;
                            for waiter in pending.drain(..) {
                                let _ = waiter.send(Err(SamError::Closed));
                            }
                            return;
                        }
                    };
                let datagram = build_datagram(raw_mode, header, payload);
                inbox.push(datagram).await;
            }
            _ => {
                let mut pending = waiters.lock().await;
                match pending.pop_front() {
                    Some(waiter) => {
                        let _ = waiter.send(Ok(line));
                    }
                    // No command is awaiting this line. Counting it keeps the behaviour
                    // observable instead of silently discarding router output.
                    None => {
                        unexpected.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

/// Rebuild an owned copy of an error so it can be handed to many waiting commands.
fn reusable(error: &SamError) -> SamError {
    match error {
        SamError::Timeout => SamError::Closed,
        SamError::Io(inner) => SamError::Io(std::io::Error::new(inner.kind(), inner.to_string())),
        SamError::Protocol(inner) => SamError::Protocol(inner.clone()),
        SamError::Rejected(message) => SamError::Rejected(message.clone()),
        SamError::Unsupported(message) => SamError::Unsupported(message.clone()),
        SamError::NameNotFound => SamError::NameNotFound,
        SamError::IdentityUnavailable => SamError::IdentityUnavailable,
        SamError::Closed => SamError::Closed,
        SamError::RetryAdmissionSaturated => SamError::RetryAdmissionSaturated,
    }
}

fn build_datagram(raw_mode: bool, header: DatagramDelivery, payload: Vec<u8>) -> ReceivedDatagram {
    if raw_mode {
        return ReceivedDatagram::Raw(RawDatagram {
            from_port: header.from_port.unwrap_or_else(|| Port::new(0)),
            to_port: header.to_port.unwrap_or_else(|| Port::new(0)),
            protocol: header
                .protocol
                .unwrap_or_else(|| I2pProtocol::new(18).expect("default RAW protocol is valid")),
            payload,
        });
    }
    match header.source {
        Some(source) => ReceivedDatagram::Authenticated(AuthenticatedDatagram {
            source,
            from_port: header.from_port.unwrap_or_else(|| Port::new(0)),
            to_port: header.to_port.unwrap_or_else(|| Port::new(0)),
            payload,
        }),
        // A DATAGRAM1 delivery without a source would silently weaken authentication.
        None => ReceivedDatagram::Raw(RawDatagram {
            from_port: header.from_port.unwrap_or_else(|| Port::new(0)),
            to_port: header.to_port.unwrap_or_else(|| Port::new(0)),
            protocol: I2pProtocol::new(18).expect("default RAW protocol is valid"),
            payload,
        }),
    }
}

/// Reader wrapper that yields pushed-back bytes before the socket's own bytes.
pub struct Prefixed<R> {
    prefix: Vec<u8>,
    inner: R,
}

impl<R> Prefixed<R> {
    pub fn new(prefix: Vec<u8>, inner: R) -> Self {
        Self { prefix, inner }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Prefixed<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if !this.prefix.is_empty() {
            let take = this.prefix.len().min(buf.remaining());
            buf.put_slice(&this.prefix[..take]);
            this.prefix.drain(..take);
            return std::task::Poll::Ready(Ok(()));
        }
        std::pin::Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<R: AsyncBufRead + Unpin> AsyncBufRead for Prefixed<R> {
    fn poll_fill_buf(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&[u8]>> {
        let this = self.get_mut();
        if this.prefix.is_empty() {
            return std::pin::Pin::new(&mut this.inner).poll_fill_buf(cx);
        }
        std::task::Poll::Ready(Ok(this.prefix.as_slice()))
    }

    fn consume(self: std::pin::Pin<&mut Self>, amount: usize) {
        let this = self.get_mut();
        if this.prefix.is_empty() {
            std::pin::Pin::new(&mut this.inner).consume(amount);
            return;
        }
        let taken = amount.min(this.prefix.len());
        this.prefix.drain(..taken);
        if taken < amount {
            std::pin::Pin::new(&mut this.inner).consume(amount - taken);
        }
    }
}

impl<R: AsyncWrite + Unpin> AsyncWrite for Prefixed<R> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
