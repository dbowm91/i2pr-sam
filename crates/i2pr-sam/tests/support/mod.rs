//! Deterministic mock SAM bridge for the regression suite.
//!
//! Every test in this suite asserts on the bytes a router actually put on the wire, because
//! the defects this suite closes were all invisible behind "the call returned `Ok`". The
//! bridge therefore offers three things:
//!
//! * **scripted replies** — a per-connection list of rules matched on exact command bytes;
//! * **a byte-level write log** — every byte the client wrote, including size-delimited
//!   payloads, is recorded so a test can assert on the frame rather than on the return value;
//! * **deterministic failure** — a rule whose command does not match panics the connection
//!   and is recorded, so a broken fixture cannot silently pass.
//!
//! Only tokio is used. No sleeps are required for correctness: every wait is either a
//! command/reply exchange or a bounded poll for a counter that the client itself drives.

// Each integration test binary compiles its own copy of this harness, so a rule used by one
// test file is dead code in the next. The harness is deliberately a superset.
#![allow(dead_code)]

use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

/// Synthetic stand-in for a router-generated Destination: 32 bytes, valid standard base64.
///
/// These are not real I2P keypairs and carry no key material. They exist so the suite can
/// compute an expected SHA-256 independently and compare it with the client's own hash.
pub const OWNER_DESTINATION: &str = "b3duZXItaWRlbnRpdHktZml4dHVyZS0zMmJ5dGVzIQA=";
/// Synthetic peer Destination announced by a non-silent `STREAM ACCEPT`.
///
/// Deliberately chosen with no base64 padding. `read_stream_peer` decides whether the first
/// line is a Destination or a `KEY=VALUE` pair, and every base64 Destination whose length is
/// not a multiple of three ends in `=`. A padding-free fixture keeps the framing tests
/// independent of that question; [`PADDED_PEER_DESTINATION`] covers the padded shape.
pub const PEER_DESTINATION: &str = "cGVlci1pZGVudGl0eS1maXh0dXJlLTMzYnl0ZXMhIS4u";
/// A Destination with the shape a real 256-byte I2P keypair produces: base64 with `==`
/// padding. Used to prove the peer-block reader accepts the shape real Destinations have.
pub const PADDED_PEER_DESTINATION: &str = "AnaotwyWc5TeobcE4H0aRJSkZA45JJ/smBVLfJzRaqdtS86oPyRmhqEbtMEO1ym3MiI6/x9le0bApII+2bMgVgJ2qLcMlnOU3qG3BOB9GkSUpGQOOSSf7JgVS3yc0WqnbUvOqD8kZoahG7TBDtcptzIiOv8fZXtGwKSCPtmzIFYCdqi3DJZzlN6htwTgfRpElKRkDjkkn+yYFUt8nNFqp21Lzqg/JGaGoRu0wQ7XKbcyIjr/H2V7RsCkgj7ZsyBWAnaotwyWc5TeobcE4H0aRJSkZA45JJ/smBVLfJzRaqdtS86oPyRmhqEbtMEO1ym3MiI6/x9le0bApII+2bMgVg==";

/// Synthetic third Destination, used where two distinct identities must not be confused.
pub const THIRD_DESTINATION: &str = "dGhpcmQtaWRlbnRpdHktZml4dHVyZS0zMmJ5dGVzIQA=";

/// Opaque router-side private key blob echoed by `SESSION STATUS ... DESTINATION=`.
/// It is never valid key material and never used as an identity.
pub const ROUTER_PRIVATE_BLOB: &str = "cm91dGVyLXNpZGUtcHJpdmF0ZS1rZXktYmxvYg==";

/// The only HELLO reply this suite accepts.
pub const HELLO_REPLY: &str = "HELLO REPLY RESULT=OK VERSION=3.3 MAJOR=3 MINOR=3\n";
/// A successful session creation reply.
pub const SESSION_OK: &str =
    "SESSION STATUS RESULT=OK EXPIRES=3600 DESTINATION=cm91dGVyLXNpZGUtcHJpdmF0ZS1rZXktYmxvYg==\n";
/// `NAMING LOOKUP NAME=ME` answered with a concrete Destination.
pub fn naming_me_ok(destination: &str) -> String {
    format!("NAMING REPLY RESULT=OK NAME=ME VALUE={destination}\n")
}

/// Convenience: an owned `String` as raw reply bytes.
pub fn bytes(text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

/// Concatenate byte slices into one reply frame.
pub fn frame(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

// ---------------------------------------------------------------------------
// Scripting vocabulary
// ---------------------------------------------------------------------------

/// How a scripted rule compares the command line it received.
///
/// The comparison is against the line with its terminator removed, so an expectation must not
/// end in `\n`.
#[derive(Clone, Debug)]
pub enum Match {
    Exact(String),
    StartsWith(String),
    EndsWith(String),
    Contains(String),
    /// Accept any command line; still recorded for later byte assertions.
    Any,
}

impl Match {
    pub fn exact(text: impl Into<String>) -> Self {
        Self::Exact(text.into())
    }

    pub fn starts_with(text: impl Into<String>) -> Self {
        Self::StartsWith(text.into())
    }

    #[allow(dead_code)]
    pub fn ends_with(text: impl Into<String>) -> Self {
        Self::EndsWith(text.into())
    }

    #[allow(dead_code)]
    pub fn contains(text: impl Into<String>) -> Self {
        Self::Contains(text.into())
    }

    #[allow(dead_code)]
    pub fn any() -> Self {
        Self::Any
    }

    fn accepts(&self, line: &str) -> bool {
        match self {
            Self::Exact(text) => line == text,
            Self::StartsWith(text) => line.starts_with(text.as_str()),
            Self::EndsWith(text) => line.ends_with(text.as_str()),
            Self::Contains(text) => line.contains(text.as_str()),
            Self::Any => true,
        }
    }

    fn describe(&self) -> String {
        match self {
            Self::Exact(text) => format!("exactly {text:?}"),
            Self::StartsWith(text) => format!("starting with {text:?}"),
            Self::EndsWith(text) => format!("ending with {text:?}"),
            Self::Contains(text) => format!("containing {text:?}"),
            Self::Any => "any line".to_owned(),
        }
    }
}

/// A text slot a rule can capture a command field into, so a later step can use it
/// (for example the ephemeral UDP forwarding `PORT` the client advertised).
#[derive(Clone, Debug, Default)]
pub struct TextSlot(Arc<Mutex<Option<String>>>);

impl TextSlot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().expect("text slot poisoned").clone()
    }
}

/// A byte slot a rule can capture a size-delimited payload into.
#[derive(Clone, Debug, Default)]
pub struct BytesSlot(Arc<Mutex<Vec<u8>>>);

impl BytesSlot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Vec<u8> {
        self.0.lock().expect("byte slot poisoned").clone()
    }
}

/// One scripted step of a connection.
#[derive(Clone, Debug)]
pub enum Rule {
    /// Read one `\n`-terminated command and reply. The reply may contain raw payload bytes.
    Line { expect: Match, reply: Vec<u8> },
    /// Read one command, capture one `KEY=` field from it, then reply.
    Capture {
        expect: Match,
        field: String,
        slot: TextSlot,
        reply: Vec<u8>,
    },
    /// Read exactly `len` raw bytes and require them to equal `expect`.
    Raw { len: usize, expect: Vec<u8> },
    /// Read exactly `len` raw bytes into `slot`.
    RawCapture { len: usize, slot: BytesSlot },
    /// Write bytes unsolicited, without waiting for a command.
    Push { bytes: Vec<u8> },
    /// Write each frame unsolicited, one socket write per frame, without reading anything.
    ///
    /// This models a router that outruns its reader. Frames are complete, so the client's
    /// bounded queue must drop whole deliveries rather than interleave them.
    Flood { frames: Vec<Vec<u8>> },
    /// Read one command line without asserting on it.
    SkipLine,
    /// Read until the peer closes the connection.
    Eof,
    /// Close the connection immediately.
    Close,
    /// Hold the connection open without reading, for bounded-timeout assertions.
    Delay(Duration),
}

impl Rule {
    /// Expect one command line, then reply with text.
    pub fn line(expect: Match, reply: &str) -> Self {
        Self::Line {
            expect,
            reply: bytes(reply),
        }
    }

    /// Expect one command line, then reply with arbitrary bytes.
    pub fn line_bytes(expect: Match, reply: Vec<u8>) -> Self {
        Self::Line { expect, reply }
    }

    /// Expect one command line, capture a `KEY=` field, then reply with text.
    pub fn capture(expect: Match, field: &str, slot: TextSlot, reply: &str) -> Self {
        Self::Capture {
            expect,
            field: field.to_owned(),
            slot,
            reply: bytes(reply),
        }
    }

    pub fn raw(len: usize, expect: &[u8]) -> Self {
        Self::Raw {
            len,
            expect: expect.to_vec(),
        }
    }

    pub fn raw_capture(len: usize, slot: BytesSlot) -> Self {
        Self::RawCapture { len, slot }
    }

    pub fn push(reply: impl Into<Vec<u8>>) -> Self {
        Self::Push {
            bytes: reply.into(),
        }
    }

    /// Push every frame unsolicited, in order.
    pub fn flood(frames: Vec<Vec<u8>>) -> Self {
        Self::Flood { frames }
    }

    pub fn skip_line() -> Self {
        Self::SkipLine
    }

    pub fn eof() -> Self {
        Self::Eof
    }

    pub fn close() -> Self {
        Self::Close
    }

    pub fn delay(duration: Duration) -> Self {
        Self::Delay(duration)
    }
}

/// An ordered list of rules for one connection.
pub type Script = Vec<Rule>;

/// A well-behaved mock router: every command a session lifecycle needs is answered, and the
/// connection then keeps recording whatever the client writes until the client closes it.
pub fn default_script() -> Script {
    vec![
        Rule::line(Match::starts_with("HELLO VERSION"), HELLO_REPLY),
        Rule::line(Match::starts_with("SESSION CREATE"), SESSION_OK),
        Rule::line(
            Match::exact("NAMING LOOKUP NAME=ME"),
            &naming_me_ok(OWNER_DESTINATION),
        ),
        Rule::line(Match::starts_with("SESSION ADD"), SESSION_OK),
        Rule::line(Match::starts_with("SESSION REMOVE"), SESSION_OK),
        Rule::line(
            Match::starts_with("STREAM CONNECT"),
            "STREAM STATUS RESULT=OK\n",
        ),
        Rule::line(
            Match::starts_with("STREAM ACCEPT"),
            &stream_accept_with_peer(),
        ),
        Rule::line(
            Match::starts_with("DEST GENERATE"),
            &format!("DEST REPLY PUB={OWNER_DESTINATION} PRIV={ROUTER_PRIVATE_BLOB}\n"),
        ),
        Rule::line(Match::starts_with("PING"), "PONG\n"),
    ]
}

/// A non-silent `STREAM ACCEPT` reply: status line, peer identity block, then payload.
pub fn stream_accept_with_peer() -> String {
    format!("STREAM STATUS RESULT=OK\n{PEER_DESTINATION}\nFROM_PORT=1\nTO_PORT=2\n\n")
}

/// One `DATAGRAM RECEIVED` delivery: header line then exactly `SIZE` **raw** payload bytes.
///
/// The payload is deliberately not base64 and may contain `\n` and NUL bytes, because the
/// v1/v2-compatible control socket carries raw bytes.
pub fn datagram_delivery(
    destination: &str,
    from_port: u16,
    to_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let header = format!(
        "DATAGRAM RECEIVED DESTINATION={destination} SIZE={} FROM_PORT={from_port} TO_PORT={to_port}\n",
        payload.len()
    );
    frame(&[header.as_bytes(), payload])
}

// ---------------------------------------------------------------------------
// Bridge
// ---------------------------------------------------------------------------

struct ConnectionState {
    index: usize,
    written: Arc<Mutex<Vec<u8>>>,
    failures: Arc<Mutex<Vec<String>>>,
}

/// A read-only view of what the client wrote on one connection.
#[derive(Clone)]
pub struct Connection {
    index: usize,
    written: Arc<Mutex<Vec<u8>>>,
    failures: Arc<Mutex<Vec<String>>>,
}

impl Connection {
    pub fn index(&self) -> usize {
        self.index
    }

    /// Every byte the client wrote on this connection, in order.
    pub fn written(&self) -> Vec<u8> {
        self.written.lock().expect("write log poisoned").clone()
    }

    pub fn written_text(&self) -> String {
        String::from_utf8_lossy(&self.written()).into_owned()
    }

    pub fn wrote(&self, needle: &[u8]) -> bool {
        contains(&self.written(), needle)
    }

    /// Assert the client wrote exactly this byte sequence at some point.
    #[track_caller]
    pub fn assert_wrote(&self, needle: &[u8]) {
        assert!(
            self.wrote(needle),
            "connection {} never wrote {:?}\n--- client wrote ---\n{}",
            self.index,
            String::from_utf8_lossy(needle),
            self.written_text()
        );
    }

    #[track_caller]
    pub fn assert_lacks(&self, needle: &[u8]) {
        assert!(
            !self.wrote(needle),
            "connection {} unexpectedly wrote {:?}\n--- client wrote ---\n{}",
            self.index,
            String::from_utf8_lossy(needle),
            self.written_text()
        );
    }

    pub fn occurrences(&self, needle: &[u8]) -> usize {
        count(&self.written(), needle)
    }

    /// The bytes the client wrote after the first occurrence of `marker`.
    #[track_caller]
    pub fn after(&self, marker: &[u8]) -> Vec<u8> {
        let written = self.written();
        match find(&written, marker) {
            Some(at) => written[at + marker.len()..].to_vec(),
            None => panic!(
                "connection {} never wrote marker {:?}\n--- client wrote ---\n{}",
                self.index,
                String::from_utf8_lossy(marker),
                self.written_text()
            ),
        }
    }

    pub fn failures(&self) -> Vec<String> {
        self.failures.lock().expect("failure log poisoned").clone()
    }
}

struct BridgeState {
    live: AtomicUsize,
    accepted: AtomicUsize,
    connections: Mutex<Vec<Arc<ConnectionState>>>,
}

impl BridgeState {
    fn register(&self) -> (usize, Arc<ConnectionState>) {
        let index = self.accepted.fetch_add(1, Ordering::SeqCst);
        self.live.fetch_add(1, Ordering::SeqCst);
        let state = Arc::new(ConnectionState {
            index,
            written: Arc::new(Mutex::new(Vec::new())),
            failures: Arc::new(Mutex::new(Vec::new())),
        });
        self.connections
            .lock()
            .expect("connection registry poisoned")
            .push(state.clone());
        (index, state)
    }

    fn release(&self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A mock SAM bridge on an ephemeral loopback port that serves many concurrent connections.
pub struct MockBridge {
    endpoint: SocketAddr,
    state: Arc<BridgeState>,
    accept_task: JoinHandle<()>,
}

impl MockBridge {
    /// Start a bridge whose every connection runs [`default_script`].
    pub async fn start() -> Self {
        Self::start_with_fallback(Vec::new(), default_script()).await
    }

    /// Start a bridge that gives connection `n` the script at index `n`, falling back to
    /// [`default_script`] once the scripted list is exhausted.
    pub async fn start_with(scripts: Vec<Script>) -> Self {
        Self::start_with_fallback(scripts, default_script()).await
    }

    /// Start a bridge with an explicit fallback script for connections beyond `scripts`.
    pub async fn start_with_fallback(scripts: Vec<Script>, fallback: Script) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock bridge bind");
        let endpoint = listener.local_addr().expect("mock bridge addr");
        let state = Arc::new(BridgeState {
            live: AtomicUsize::new(0),
            accepted: AtomicUsize::new(0),
            connections: Mutex::new(Vec::new()),
        });
        let scripts = Arc::new(scripts);
        let fallback = Arc::new(fallback);
        let task = {
            let state = state.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((socket, _)) = listener.accept().await else {
                        return;
                    };
                    let (index, connection) = state.register();
                    let script = scripts
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| (*fallback).clone());
                    let state = state.clone();
                    tokio::spawn(async move {
                        serve(socket, connection, script, state).await;
                    });
                }
            })
        };
        Self {
            endpoint,
            state,
            accept_task: task,
        }
    }

    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    /// A client configuration pointed at this bridge with short, bounded timeouts.
    pub fn client_config(&self) -> i2pr_sam::ClientConfig {
        let mut config = i2pr_sam::ClientConfig::new(self.endpoint);
        config.connect_timeout = Duration::from_secs(5);
        config.control_timeout = Duration::from_secs(3);
        config
    }

    pub fn accepted_connections(&self) -> usize {
        self.state.accepted.load(Ordering::SeqCst)
    }

    /// Connections the bridge has accepted and not yet seen close.
    pub fn live_connections(&self) -> usize {
        self.state.live.load(Ordering::SeqCst)
    }

    pub fn connection(&self, index: usize) -> Connection {
        let registry = self.state.connections.lock().expect("registry poisoned");
        let state = registry
            .get(index)
            .unwrap_or_else(|| panic!("connection {index} was never opened"));
        Connection {
            index,
            written: state.written.clone(),
            failures: state.failures.clone(),
        }
    }

    /// Bytes the client wrote on any connection, in accept order.
    pub fn all_written(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for index in 0..self.accepted_connections() {
            out.extend_from_slice(&self.connection(index).written());
        }
        out
    }

    pub fn wrote(&self, needle: &[u8]) -> bool {
        contains(&self.all_written(), needle)
    }

    #[track_caller]
    pub fn assert_wrote(&self, needle: &[u8]) {
        assert!(
            self.wrote(needle),
            "no connection wrote {:?}\n--- all client writes ---\n{}",
            String::from_utf8_lossy(needle),
            String::from_utf8_lossy(&self.all_written())
        );
    }

    #[track_caller]
    pub fn assert_lacks(&self, needle: &[u8]) {
        assert!(
            !self.wrote(needle),
            "some connection wrote {:?}\n--- all client writes ---\n{}",
            String::from_utf8_lossy(needle),
            String::from_utf8_lossy(&self.all_written())
        );
    }

    /// Scripted expectations that did not hold. A non-empty list means a fixture drifted.
    #[track_caller]
    pub fn assert_scripts_clean(&self) {
        let registry = self.state.connections.lock().expect("registry poisoned");
        let mut failures = Vec::new();
        for state in registry.iter() {
            for failure in state.failures.lock().expect("failure log poisoned").iter() {
                failures.push(format!("connection {}: {failure}", state.index));
            }
        }
        assert!(
            failures.is_empty(),
            "mock bridge script expectations failed:\n{}",
            failures.join("\n")
        );
    }

    /// Wait until `predicate` holds, polling on a short timer. Used only for counters the
    /// client itself advances, so the wait is bounded by the client's own progress.
    pub async fn wait_for(&self, what: &str, mut predicate: impl FnMut() -> bool) {
        let deadline = Duration::from_secs(5);
        let started = tokio::time::Instant::now();
        while !predicate() {
            assert!(
                started.elapsed() < deadline,
                "timed out after 5s waiting for {what}"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
}

impl Drop for MockBridge {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

async fn serve(
    socket: TcpStream,
    state: Arc<ConnectionState>,
    script: Script,
    bridge: Arc<BridgeState>,
) {
    let mut wire = Wire {
        socket,
        log: state.written.clone(),
        pending: Vec::new(),
    };
    for rule in script {
        if let Err(failure) = apply(&mut wire, &state.failures, &rule).await {
            state
                .failures
                .lock()
                .expect("failure log poisoned")
                .push(failure);
            break;
        }
    }
    // Past the script the connection keeps recording, so a test can still assert on
    // trailing bytes the client wrote after its last command.
    wire.drain().await;
    bridge.release();
}

async fn apply(
    wire: &mut Wire,
    failures: &Arc<Mutex<Vec<String>>>,
    rule: &Rule,
) -> Result<(), String> {
    match rule {
        Rule::Line { expect, reply } => {
            let line = wire.read_line().await?;
            check(expect, &line, failures)?;
            wire.write(reply).await
        }
        Rule::Capture {
            expect,
            field,
            slot,
            reply,
        } => {
            let line = wire.read_line().await?;
            check(expect, &line, failures)?;
            let text = String::from_utf8_lossy(&line).into_owned();
            let value = text
                .split_ascii_whitespace()
                .find_map(|word| word.strip_prefix(&format!("{field}=")))
                .map(str::to_owned)
                .ok_or_else(|| format!("command {text:?} carried no {field}= field for capture"))?;
            *slot.0.lock().expect("text slot poisoned") = Some(value);
            wire.write(reply).await
        }
        Rule::Raw { len, expect } => {
            let got = wire.read_exact(*len).await?;
            if got != *expect {
                return Err(format!(
                    "expected {} raw payload bytes {:?} but read {:?}",
                    len,
                    String::from_utf8_lossy(expect),
                    String::from_utf8_lossy(&got)
                ));
            }
            Ok(())
        }
        Rule::RawCapture { len, slot } => {
            let got = wire.read_exact(*len).await?;
            slot.0
                .lock()
                .expect("byte slot poisoned")
                .extend_from_slice(&got);
            Ok(())
        }
        Rule::Push { bytes } => wire.write(bytes).await,
        Rule::Flood { frames } => {
            for frame in frames {
                wire.write(frame).await?;
            }
            Ok(())
        }
        Rule::SkipLine => wire.read_line().await.map(|_| ()),
        Rule::Eof => {
            wire.read_to_end().await;
            Ok(())
        }
        Rule::Close => {
            wire.shutdown().await;
            Ok(())
        }
        Rule::Delay(duration) => {
            tokio::time::sleep(*duration).await;
            Ok(())
        }
    }
}

fn check(expect: &Match, line: &[u8], _failures: &Arc<Mutex<Vec<String>>>) -> Result<(), String> {
    let text = String::from_utf8_lossy(line);
    let text = text.trim_end_matches(['\r', '\n']);
    if expect.accepts(text) {
        Ok(())
    } else {
        Err(format!(
            "expected a line {} but router received {text:?}",
            expect.describe()
        ))
    }
}

/// A socket that records every byte it reads while still honouring message boundaries.
struct Wire {
    socket: TcpStream,
    log: Arc<Mutex<Vec<u8>>>,
    pending: Vec<u8>,
}

impl Wire {
    async fn fill(&mut self) -> Result<bool, String> {
        if !self.pending.is_empty() {
            return Ok(true);
        }
        let mut buffer = vec![0u8; 8192];
        match self.socket.read(&mut buffer).await {
            Ok(0) => Ok(false),
            Ok(count) => {
                self.log
                    .lock()
                    .expect("write log poisoned")
                    .extend_from_slice(&buffer[..count]);
                self.pending = buffer[..count].to_vec();
                Ok(true)
            }
            Err(error) => Err(format!("socket read failed: {error}")),
        }
    }

    async fn read_line(&mut self) -> Result<Vec<u8>, String> {
        let mut line = Vec::new();
        loop {
            if !self.fill().await? {
                return Err(if line.is_empty() {
                    "connection closed before the next command arrived".to_owned()
                } else {
                    format!(
                        "connection closed mid-line after {:?}",
                        String::from_utf8_lossy(&line)
                    )
                });
            }
            match self.pending.iter().position(|byte| *byte == b'\n') {
                Some(at) => {
                    line.extend_from_slice(&self.pending[..=at]);
                    self.pending.drain(..at + 1);
                    return Ok(line);
                }
                None => {
                    let chunk = std::mem::take(&mut self.pending);
                    line.extend_from_slice(&chunk);
                }
            }
        }
    }

    async fn read_exact(&mut self, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            if !self.fill().await? {
                return Err(format!(
                    "connection closed after {} of {len} expected payload bytes",
                    out.len()
                ));
            }
            let take = (len - out.len()).min(self.pending.len());
            out.extend_from_slice(&self.pending[..take]);
            self.pending.drain(..take);
        }
        Ok(out)
    }

    async fn read_to_end(&mut self) {
        self.pending.clear();
        let mut buffer = vec![0u8; 8192];
        while let Ok(count) = self.socket.read(&mut buffer).await {
            if count == 0 {
                return;
            }
            self.log
                .lock()
                .expect("write log poisoned")
                .extend_from_slice(&buffer[..count]);
        }
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.socket
            .write_all(bytes)
            .await
            .map_err(|error| format!("router write failed: {error}"))?;
        self.socket
            .flush()
            .await
            .map_err(|error| format!("router flush failed: {error}"))
    }

    async fn shutdown(&mut self) {
        let _ = self.socket.shutdown().await;
    }

    /// Record whatever the client writes until it closes the connection.
    async fn drain(&mut self) {
        self.read_to_end().await;
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}
