//! Runtime-neutral, bounded SAM line protocol primitives.
//!
//! The crate intentionally owns no I/O, runtime, timers, or process state.

use std::{fmt, str};

pub const MAX_LINE_BYTES: usize = 16 * 1024;
pub const MAX_TOKENS: usize = 128;
pub const MAX_KEY_BYTES: usize = 128;
pub const MAX_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_DESTINATION_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    TooLong,
    TooManyTokens,
    InvalidUtf8,
    InvalidLineEnding,
    Empty,
    UnterminatedQuote,
    InvalidEscape,
    InvalidToken,
    DuplicateKey,
    LimitExceeded,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "SAM line exceeds configured limit",
            Self::TooManyTokens => "SAM line contains too many tokens",
            Self::InvalidUtf8 => "SAM line is not valid UTF-8",
            Self::InvalidLineEnding => "SAM line must end with LF or CRLF",
            Self::Empty => "SAM line is empty",
            Self::UnterminatedQuote => "SAM line contains an unterminated quoted value",
            Self::InvalidEscape => "SAM line contains an invalid escape sequence",
            Self::InvalidToken => "SAM line contains an invalid token",
            Self::DuplicateKey => "SAM line contains a duplicate key",
            Self::LimitExceeded => "SAM token exceeds configured limit",
        })
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value: String,
}
impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Field")
            .field("key", &self.key)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub words: Vec<String>,
    pub fields: Vec<Field>,
    pub trailing: Option<String>,
}

impl Line {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.as_str())
    }

    pub fn required(&self, key: &str) -> Result<&str, ParseError> {
        self.field(key).ok_or(ParseError::InvalidToken)
    }

    pub fn serialize(&self) -> Result<String, ParseError> {
        if self.words.is_empty() {
            return Err(ParseError::Empty);
        }
        let mut out = String::new();
        for word in &self.words {
            if !out.is_empty() {
                out.push(' ');
            }
            if word.is_empty()
                || word
                    .bytes()
                    .any(|b| b.is_ascii_whitespace() || b == b'=' || b == b'"')
            {
                return Err(ParseError::InvalidToken);
            }
            out.push_str(word);
        }
        for field in &self.fields {
            if field.key.is_empty()
                || field.key.len() > MAX_KEY_BYTES
                || field
                    .key
                    .bytes()
                    .any(|b| b.is_ascii_whitespace() || b == b'=')
            {
                return Err(ParseError::InvalidToken);
            }
            if field.value.len() > MAX_VALUE_BYTES {
                return Err(ParseError::LimitExceeded);
            }
            if field.value.contains(['\n', '\r']) {
                return Err(ParseError::InvalidLineEnding);
            }
            out.push(' ');
            out.push_str(&field.key);
            out.push('=');
            if field.value.is_empty()
                || field
                    .value
                    .bytes()
                    .any(|b| b.is_ascii_whitespace() || b == b'"' || b == b'\\')
            {
                out.push('"');
                for ch in field.value.chars() {
                    if ch == '"' || ch == '\\' {
                        out.push('\\');
                    }
                    out.push(ch);
                }
                out.push('"');
            } else {
                out.push_str(&field.value);
            }
        }
        if let Some(tail) = &self.trailing {
            if tail.contains(['\n', '\r']) {
                return Err(ParseError::InvalidLineEnding);
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(tail);
        }
        if out.len() + 1 > MAX_LINE_BYTES {
            return Err(ParseError::TooLong);
        }
        out.push('\n');
        Ok(out)
    }
}

/// Parse one complete SAM line. Limits are checked before token allocation.
pub fn parse_line(input: &[u8]) -> Result<Line, ParseError> {
    if input.len() > MAX_LINE_BYTES {
        return Err(ParseError::TooLong);
    }
    let bytes = input
        .strip_suffix(b"\r\n")
        .or_else(|| input.strip_suffix(b"\n"))
        .ok_or(ParseError::InvalidLineEnding)?;
    if bytes.contains(&b'\r') || bytes.contains(&b'\n') {
        return Err(ParseError::InvalidLineEnding);
    }
    let text = str::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8)?;
    if text.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some((head, tail)) = text.split_once(' ')
        && matches!(head, "PING" | "PONG")
    {
        return Ok(Line {
            words: vec![head.to_owned()],
            fields: Vec::new(),
            trailing: Some(tail.to_owned()),
        });
    }
    let b = text.as_bytes();
    let mut i = 0;
    let mut words = Vec::new();
    let mut fields = Vec::new();
    while i < b.len() {
        while i < b.len() && b[i] == b' ' {
            i += 1;
        }
        if i == b.len() {
            break;
        }
        if words.len() + fields.len() >= MAX_TOKENS {
            return Err(ParseError::TooManyTokens);
        }
        let start = i;
        while i < b.len() && b[i] != b' ' && b[i] != b'=' {
            i += 1;
        }
        if i == start {
            return Err(ParseError::InvalidToken);
        }
        let token = &text[start..i];
        if i < b.len() && b[i] == b'=' {
            if token.len() > MAX_KEY_BYTES {
                return Err(ParseError::LimitExceeded);
            }
            i += 1;
            let value = if i < b.len() && b[i] == b'"' {
                i += 1;
                let mut value = String::new();
                let mut closed = false;
                while i < b.len() {
                    match b[i] {
                        b'"' => {
                            i += 1;
                            closed = true;
                            break;
                        }
                        b'\\' => {
                            i += 1;
                            if i >= b.len() || !matches!(b[i], b'"' | b'\\') {
                                return Err(ParseError::InvalidEscape);
                            }
                            value.push(b[i] as char);
                            i += 1;
                        }
                        _ => {
                            let ch = text[i..].chars().next().ok_or(ParseError::InvalidUtf8)?;
                            value.push(ch);
                            i += ch.len_utf8();
                        }
                    }
                    if value.len() > MAX_VALUE_BYTES {
                        return Err(ParseError::LimitExceeded);
                    }
                }
                if !closed {
                    return Err(ParseError::UnterminatedQuote);
                }
                if i < b.len() && b[i] != b' ' {
                    return Err(ParseError::InvalidToken);
                }
                value
            } else {
                let start_value = i;
                while i < b.len() && b[i] != b' ' {
                    i += 1;
                }
                if i - start_value > MAX_VALUE_BYTES {
                    return Err(ParseError::LimitExceeded);
                }
                text[start_value..i].to_owned()
            };
            if fields.iter().any(|f: &Field| f.key == token) {
                return Err(ParseError::DuplicateKey);
            }
            fields.push(Field {
                key: token.to_owned(),
                value,
            });
        } else {
            if token.contains('"') {
                return Err(ParseError::InvalidToken);
            }
            words.push(token.to_owned());
        }
    }
    if words.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(Line {
        words,
        fields,
        trailing: None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SamVersion {
    pub major: u8,
    pub minor: u8,
}
impl SamVersion {
    pub const V3_0: Self = Self { major: 3, minor: 0 };
    pub const V3_1: Self = Self { major: 3, minor: 1 };
    pub const V3_3: Self = Self { major: 3, minor: 3 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Port(u16);
impl Port {
    pub fn new(value: u16) -> Self {
        Self(value)
    }
    pub fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2pProtocol(u8);
impl I2pProtocol {
    pub fn new(value: u8) -> Result<Self, ParseError> {
        if matches!(value, 6 | 17 | 19 | 20) {
            return Err(ParseError::InvalidToken);
        }
        Ok(Self(value))
    }
    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionId(String);
impl SessionId {
    pub fn new(value: impl Into<String>) -> Result<Self, ParseError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
        {
            return Err(ParseError::InvalidToken);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Destination(String);
impl Destination {
    pub fn new(value: impl Into<String>) -> Result<Self, ParseError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_DESTINATION_BYTES {
            return Err(ParseError::LimitExceeded);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical 32-byte hash of a concrete I2P Destination.
///
/// This is the value that may be published in conformance artifacts: it proves a
/// specific linkability domain without ever exposing destination or key material.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DestinationHash([u8; 32]);
impl DestinationHash {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Debug for DestinationHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DestinationHash({self})")
    }
}
impl fmt::Display for DestinationHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretDestination(String);
impl SecretDestination {
    pub fn new(value: impl Into<String>) -> Result<Self, ParseError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_DESTINATION_BYTES {
            return Err(ParseError::LimitExceeded);
        }
        Ok(Self(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretDestination([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnverifiedSourceHash([u8; 32]);
impl UnverifiedSourceHash {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedDatagram {
    pub source: Destination,
    pub from_port: Port,
    pub to_port: Port,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnverifiedDatagram3 {
    pub source_hash: UnverifiedSourceHash,
    pub from_port: Port,
    pub to_port: Port,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawDatagram {
    pub from_port: Port,
    pub to_port: Port,
    pub protocol: I2pProtocol,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceivedDatagram {
    Authenticated(AuthenticatedDatagram),
    Unverified(UnverifiedDatagram3),
    Raw(RawDatagram),
}

/// Header of a size-delimited datagram delivered on the SAM control socket.
///
/// Ordinary DATAGRAM1 and RAW sessions without a forwarding `PORT` receive data in the
/// v1/v2-compatible form: a header line, then exactly `size` raw bytes with no base64
/// framing. DATAGRAM2 and DATAGRAM3 never use this mechanism.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatagramDelivery {
    /// Authenticated source Destination. `None` for RAW, which has no source identity.
    pub source: Option<Destination>,
    pub from_port: Option<Port>,
    pub to_port: Option<Port>,
    /// Only present for RAW deliveries.
    pub protocol: Option<I2pProtocol>,
    pub size: usize,
}

impl DatagramDelivery {
    fn parse(line: &Line, raw: bool) -> Result<Self, ParseError> {
        let size: usize = line
            .field("SIZE")
            .ok_or(ParseError::InvalidToken)?
            .parse()
            .map_err(|_| ParseError::InvalidToken)?;
        if size == 0 {
            return Err(ParseError::InvalidToken);
        }
        let source = match line.field("DESTINATION") {
            Some(text) if !raw => Some(Destination::new(text)?),
            _ => None,
        };
        let port = |key: &str| -> Result<Option<Port>, ParseError> {
            match line.field(key) {
                None => Ok(None),
                Some(text) => Ok(Some(Port::new(
                    text.parse().map_err(|_| ParseError::InvalidToken)?,
                ))),
            }
        };
        let protocol = match line.field("PROTOCOL") {
            None => None,
            Some(text) => Some(I2pProtocol::new(
                text.parse().map_err(|_| ParseError::InvalidToken)?,
            )?),
        };
        if !raw && protocol.is_some() {
            return Err(ParseError::InvalidToken);
        }
        Ok(Self {
            source,
            from_port: port("FROM_PORT")?,
            to_port: port("TO_PORT")?,
            protocol,
            size,
        })
    }

    /// Parse `<- DATAGRAM RECEIVED DESTINATION=.. SIZE=.. [FROM_PORT=..] [TO_PORT=..]`.
    pub fn parse_datagram(line: &Line) -> Result<Self, ParseError> {
        match (
            line.words.first().map(String::as_str),
            line.words.get(1).map(String::as_str),
        ) {
            (Some("DATAGRAM"), Some("RECEIVED")) => Self::parse(line, false),
            _ => Err(ParseError::InvalidToken),
        }
    }

    /// Parse `<- RAW RECEIVED SIZE=.. [FROM_PORT=..] [TO_PORT=..] [PROTOCOL=..]`.
    pub fn parse_raw(line: &Line) -> Result<Self, ParseError> {
        match (
            line.words.first().map(String::as_str),
            line.words.get(1).map(String::as_str),
        ) {
            (Some("RAW"), Some("RECEIVED")) => Self::parse(line, true),
            _ => Err(ParseError::InvalidToken),
        }
    }
}

/// Classify a line read from a SAM control socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomingKind {
    /// A `... REPLY` answer to a command this client sent, such as `NAMING REPLY`.
    Reply,
    /// A `... STATUS` answer, including router-originated post-`OK` notices.
    Status,
    /// Unsolicited `DATAGRAM RECEIVED`, followed by `SIZE` raw bytes.
    DatagramDelivery,
    /// Unsolicited `RAW RECEIVED`, followed by `SIZE` raw bytes.
    RawDelivery,
    /// Any other line the router originated, such as `STREAM RECEIVED` or `PONG`.
    Asynchronous,
}
impl IncomingKind {
    pub fn classify(line: &Line) -> Self {
        match (
            line.words.first().map(String::as_str),
            line.words.get(1).map(String::as_str),
        ) {
            (Some("DATAGRAM"), Some("RECEIVED")) => Self::DatagramDelivery,
            (Some("RAW"), Some("RECEIVED")) => Self::RawDelivery,
            _ if line.words.get(1).map(String::as_str) == Some("REPLY") => Self::Reply,
            _ if line.words.get(1).map(String::as_str) == Some("STATUS") => Self::Status,
            _ => Self::Asynchronous,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SamResult {
    Ok,
    CantReachPeer,
    DuplicateId,
    DuplicateDestination,
    I2pError,
    InvalidKey,
    InvalidId,
    InvalidStyle,
    KeyNotFound,
    LeaseSetNotFound,
    PeerNotFound,
    Timeout,
    Unknown(String),
}
impl SamResult {
    pub fn parse(value: &str) -> Self {
        match value {
            "OK" => Self::Ok,
            "CANT_REACH_PEER" => Self::CantReachPeer,
            "DUPLICATED_ID" => Self::DuplicateId,
            "DUPLICATED_DEST" => Self::DuplicateDestination,
            "I2P_ERROR" => Self::I2pError,
            "INVALID_KEY" => Self::InvalidKey,
            "INVALID_ID" => Self::InvalidId,
            "INVALID_STYLE" => Self::InvalidStyle,
            "KEY_NOT_FOUND" => Self::KeyNotFound,
            "LEASESET_NOT_FOUND" => Self::LeaseSetNotFound,
            "PEER_NOT_FOUND" => Self::PeerNotFound,
            "TIMEOUT" => Self::Timeout,
            _ => Self::Unknown(value.to_owned()),
        }
    }

    /// True when the router states it does not implement the requested operation.
    ///
    /// This is a capability verdict, never a transient-failure verdict: unreachable
    /// peers, expired leasesets, and timeouts are deliberately excluded.
    pub fn is_unsupported_style(&self) -> bool {
        matches!(self, Self::InvalidStyle | Self::InvalidId)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Hello,
    DestGenerate,
    NamingLookup,
    SessionCreate,
    SessionAdd,
    SessionRemove,
    SessionStatus,
    StreamConnect,
    StreamAccept,
    StreamForward,
    StreamStatus,
    DatagramSend,
    RawSend,
    Ping,
    Pong,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamCommand {
    pub kind: CommandKind,
    pub fields: Vec<Field>,
    pub trailing: Option<String>,
}

impl SamCommand {
    pub fn from_line(line: &Line) -> Result<Self, ParseError> {
        let (first, second) = (
            line.words.first().map(String::as_str),
            line.words.get(1).map(String::as_str),
        );
        let kind = match (first, second) {
            (Some("HELLO"), Some("VERSION")) => CommandKind::Hello,
            (Some("DEST"), Some("GENERATE")) => CommandKind::DestGenerate,
            (Some("NAMING"), Some("LOOKUP")) => CommandKind::NamingLookup,
            (Some("SESSION"), Some("CREATE")) => CommandKind::SessionCreate,
            (Some("SESSION"), Some("ADD")) => CommandKind::SessionAdd,
            (Some("SESSION"), Some("REMOVE")) => CommandKind::SessionRemove,
            (Some("SESSION"), Some("STATUS")) => CommandKind::SessionStatus,
            (Some("STREAM"), Some("CONNECT")) => CommandKind::StreamConnect,
            (Some("STREAM"), Some("ACCEPT")) => CommandKind::StreamAccept,
            (Some("STREAM"), Some("FORWARD")) => CommandKind::StreamForward,
            (Some("STREAM"), Some("STATUS")) => CommandKind::StreamStatus,
            (Some("DATAGRAM"), Some("SEND")) => CommandKind::DatagramSend,
            (Some("RAW"), Some("SEND")) => CommandKind::RawSend,
            (Some("PING"), _) => CommandKind::Ping,
            (Some("PONG"), _) => CommandKind::Pong,
            _ => return Err(ParseError::InvalidToken),
        };
        Ok(Self {
            kind,
            fields: line.fields.clone(),
            trailing: line.trailing.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamReply {
    pub command: String,
    pub subcommand: String,
    pub result: Option<SamResult>,
    pub fields: Vec<Field>,
}
impl SamReply {
    pub fn from_line(line: &Line) -> Result<Self, ParseError> {
        if line.words.len() < 2 || line.words[1] != "REPLY" {
            return Err(ParseError::InvalidToken);
        }
        let result = line.field("RESULT").map(SamResult::parse);
        Ok(Self {
            command: line.words[0].clone(),
            subcommand: line.words[1].clone(),
            result,
            fields: line.fields.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Unknown,
    Supported,
    Unsupported,
}
impl Support {
    /// Stable artifact spelling. Never derives support from a negotiated version.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
        }
    }
    pub fn parse(value: &str) -> Self {
        match value {
            "supported" => Self::Supported,
            "unsupported" => Self::Unsupported,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamCapabilities {
    pub negotiated_version: Option<SamVersion>,
    pub stream: Support,
    pub datagram: Support,
    pub raw: Support,
    pub datagram2: Support,
    pub datagram3: Support,
    pub shared_master: Support,
    pub shared_primary: Support,
    pub session_add_remove: Support,
    pub naming_lookup_options: Support,
    pub authentication: Support,
    pub ping_pong: Support,
    /// v1/v2-compatible control-socket datagram send/receive for ordinary DATAGRAM1.
    pub datagram_direct: Support,
    /// v1/v2-compatible control-socket send/receive for ordinary RAW.
    pub raw_direct: Support,
    /// `NAMING LOOKUP NAME=ME` identity resolution for the current session.
    pub session_identity_lookup: Support,
}
impl Default for SamCapabilities {
    fn default() -> Self {
        Self {
            negotiated_version: None,
            stream: Support::Unknown,
            datagram: Support::Unknown,
            raw: Support::Unknown,
            datagram2: Support::Unknown,
            datagram3: Support::Unknown,
            shared_master: Support::Unknown,
            shared_primary: Support::Unknown,
            session_add_remove: Support::Unknown,
            naming_lookup_options: Support::Unknown,
            authentication: Support::Unknown,
            ping_pong: Support::Unknown,
            datagram_direct: Support::Unknown,
            raw_direct: Support::Unknown,
            session_identity_lookup: Support::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStyle {
    Stream,
    Datagram,
    Raw,
    Datagram2,
    Datagram3,
}
impl SessionStyle {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Stream => "STREAM",
            Self::Datagram => "DATAGRAM",
            Self::Raw => "RAW",
            Self::Datagram2 => "DATAGRAM2",
            Self::Datagram3 => "DATAGRAM3",
        }
    }
    /// Only ordinary DATAGRAM1 and RAW use the v1/v2-compatible control-socket modes.
    ///
    /// DATAGRAM2/3 and every shared subsession are excluded by specification, and this
    /// predicate is the single place that exclusion is expressed.
    pub fn supports_control_socket_datagram(self) -> bool {
        matches!(self, Self::Datagram | Self::Raw)
    }
    /// DATAGRAM1/D2 authenticate their source; DATAGRAM3 does not; RAW has no source.
    pub fn source_trust(self) -> DatagramSourceTrust {
        match self {
            Self::Datagram | Self::Datagram2 => DatagramSourceTrust::Authenticated,
            Self::Datagram3 => DatagramSourceTrust::UnverifiedHash,
            Self::Raw => DatagramSourceTrust::NoSource,
            Self::Stream => DatagramSourceTrust::NotADatagram,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatagramSourceTrust {
    NotADatagram,
    Authenticated,
    UnverifiedHash,
    NoSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharedDialect {
    Master,
    Primary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    PreHello,
    Utility,
    SessionControl,
    SharedOwner,
    Child,
    StreamData,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolState {
    phase: Phase,
}
impl Default for ProtocolState {
    fn default() -> Self {
        Self {
            phase: Phase::PreHello,
        }
    }
}
impl ProtocolState {
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn hello_succeeded(&mut self) -> Result<(), ParseError> {
        if self.phase != Phase::PreHello {
            return Err(ParseError::InvalidToken);
        }
        self.phase = Phase::Utility;
        Ok(())
    }
    pub fn session_created(&mut self) -> Result<(), ParseError> {
        if self.phase != Phase::Utility {
            return Err(ParseError::InvalidToken);
        }
        self.phase = Phase::SessionControl;
        Ok(())
    }
    pub fn shared_owner_created(&mut self) -> Result<(), ParseError> {
        if self.phase != Phase::Utility {
            return Err(ParseError::InvalidToken);
        }
        self.phase = Phase::SharedOwner;
        Ok(())
    }
    pub fn child_added(&mut self) -> Result<(), ParseError> {
        if self.phase != Phase::SharedOwner {
            return Err(ParseError::InvalidToken);
        }
        self.phase = Phase::Child;
        Ok(())
    }
    pub fn stream_connected(&mut self) -> Result<(), ParseError> {
        if self.phase != Phase::SessionControl {
            return Err(ParseError::InvalidToken);
        }
        self.phase = Phase::StreamData;
        Ok(())
    }
    pub fn close(&mut self) {
        self.phase = Phase::Closed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_roundtrip() {
        let line = parse_line(b"SESSION CREATE STYLE=STREAM DESTINATION=TRANSIENT\n").unwrap();
        assert_eq!(
            parse_line(line.serialize().unwrap().as_bytes()).unwrap(),
            line
        );
        let escaped = parse_line("NAMING REPLY NAME=foo VALUE=\"a b\\\"c\"\n".as_bytes()).unwrap();
        assert_eq!(escaped.field("VALUE"), Some("a b\"c"));
    }
    #[test]
    fn rejects_duplicate_and_bounds() {
        assert_eq!(parse_line(b"A X=1 X=2\n"), Err(ParseError::DuplicateKey));
        assert_eq!(
            parse_line(&vec![b'a'; MAX_LINE_BYTES + 1]),
            Err(ParseError::TooLong)
        );
        assert_eq!(
            parse_line(b"HELLO VERSION\nEXTRA=1\n"),
            Err(ParseError::InvalidLineEnding)
        );
        let max_value = format!("A X={}\n", "v".repeat(MAX_VALUE_BYTES));
        assert!(parse_line(max_value.as_bytes()).is_ok());
        let over_value = format!("A X={}\n", "v".repeat(MAX_VALUE_BYTES + 1));
        assert_eq!(
            parse_line(over_value.as_bytes()),
            Err(ParseError::LimitExceeded)
        );
        let max_line = format!("{}\n", "w".repeat(MAX_LINE_BYTES - 1));
        assert!(parse_line(max_line.as_bytes()).is_ok());
        let too_many = format!(
            "A {}\n",
            (0..MAX_TOKENS)
                .map(|i| format!("K{i}=v"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert_eq!(
            parse_line(too_many.as_bytes()),
            Err(ParseError::TooManyTokens)
        );
    }
    #[test]
    fn capability_is_not_version() {
        let c = SamCapabilities {
            negotiated_version: Some(SamVersion::V3_3),
            ..SamCapabilities::default()
        };
        assert_eq!(c.datagram3, Support::Unknown);
    }

    #[test]
    fn ping_keeps_arbitrary_tail_and_secrets_redact() {
        let line = parse_line(b"PING arbitrary  data \"not parsed\"\n").unwrap();
        assert_eq!(
            line.trailing.as_deref(),
            Some("arbitrary  data \"not parsed\"")
        );
        assert!(
            format!(
                "{:?}",
                SecretDestination::new("private-key-material").unwrap()
            )
            .contains("REDACTED")
        );
        let parsed = parse_line(b"DEST REPLY PRIV=private-key-material\n").unwrap();
        assert!(!format!("{parsed:?}").contains("private-key-material"));
    }

    #[test]
    fn state_legality_and_arbitrary_bounded_input() {
        let mut state = ProtocolState::default();
        assert_eq!(state.session_created(), Err(ParseError::InvalidToken));
        state.hello_succeeded().unwrap();
        state.session_created().unwrap();
        state.stream_connected().unwrap();
        assert_eq!(state.phase(), Phase::StreamData);
        state.close();
        assert_eq!(state.phase(), Phase::Closed);

        let mut state = 0x5eed_u32;
        for sample in 0..4096 {
            let len = sample % 512;
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (state >> 24) as u8
                })
                .collect();
            let _ = parse_line(&bytes);
        }
    }

    #[test]
    fn generated_legal_values_round_trip_canonically() {
        let values = [
            "ordinary",
            "",
            "with spaces",
            "quote\"inside",
            "slash\\inside",
            "snowman ☃",
        ];
        for (index, value) in values.into_iter().enumerate() {
            let line = Line {
                words: vec!["NAMING".into(), "LOOKUP".into()],
                fields: vec![
                    Field {
                        key: "NAME".into(),
                        value: value.into(),
                    },
                    Field {
                        key: "INDEX".into(),
                        value: index.to_string(),
                    },
                ],
                trailing: None,
            };
            let serialized = line.serialize().unwrap();
            assert_eq!(parse_line(serialized.as_bytes()).unwrap(), line);
        }
    }

    #[test]
    fn command_words_and_port_types() {
        let cmd =
            SamCommand::from_line(&parse_line(b"STREAM CONNECT ID=x DESTINATION=y\n").unwrap())
                .unwrap();
        assert_eq!(cmd.kind, CommandKind::StreamConnect);
        assert_eq!(Port::new(65_535).get(), 65_535);
        assert_eq!(SessionId::new("safe-1").unwrap().as_str(), "safe-1");
        assert!(SessionId::new("bad id").is_err());
    }

    #[test]
    fn control_socket_datagram_headers_are_parsed_with_trust_typing() {
        // The header line and the size-delimited body arrive as separate reads, so the
        // parser only ever sees the header line.
        let datagram =
            parse_line(b"DATAGRAM RECEIVED DESTINATION=peer SIZE=5 FROM_PORT=7 TO_PORT=9\n")
                .unwrap();
        assert_eq!(
            IncomingKind::classify(&datagram),
            IncomingKind::DatagramDelivery
        );
        let header = DatagramDelivery::parse_datagram(&datagram).unwrap();
        assert_eq!(
            header.source.as_ref().map(Destination::as_str),
            Some("peer")
        );
        assert_eq!(header.from_port, Some(Port::new(7)));
        assert_eq!(header.to_port, Some(Port::new(9)));
        assert_eq!(header.protocol, None);
        assert_eq!(header.size, 5);

        let raw = parse_line(b"RAW RECEIVED SIZE=3 FROM_PORT=1 TO_PORT=2 PROTOCOL=18\n").unwrap();
        assert_eq!(IncomingKind::classify(&raw), IncomingKind::RawDelivery);
        let header = DatagramDelivery::parse_raw(&raw).unwrap();
        assert_eq!(header.source, None, "RAW carries no source identity");
        assert_eq!(header.protocol.map(I2pProtocol::get), Some(18));
        assert_eq!(header.size, 3);

        assert_eq!(
            IncomingKind::classify(&parse_line(b"DATAGRAM STATUS RESULT=OK MESSAGE=1\n").unwrap()),
            IncomingKind::Status
        );
        assert_eq!(
            IncomingKind::classify(&parse_line(b"NAMING REPLY RESULT=OK VALUE=v\n").unwrap()),
            IncomingKind::Reply
        );
        assert_eq!(
            IncomingKind::classify(&parse_line(b"STREAM RECEIVED ID=a SIZE=1\n").unwrap()),
            IncomingKind::Asynchronous
        );
    }

    #[test]
    fn control_socket_datagram_headers_reject_contradictory_metadata() {
        // SIZE is mandatory and must be non-zero: it bounds the following raw bytes.
        assert_eq!(
            DatagramDelivery::parse_datagram(
                &parse_line(b"DATAGRAM RECEIVED DESTINATION=p\n").unwrap()
            ),
            Err(ParseError::InvalidToken)
        );
        assert_eq!(
            DatagramDelivery::parse_datagram(
                &parse_line(b"DATAGRAM RECEIVED DESTINATION=p SIZE=0\n").unwrap()
            ),
            Err(ParseError::InvalidToken)
        );
        assert_eq!(
            DatagramDelivery::parse_datagram(
                &parse_line(b"DATAGRAM RECEIVED DESTINATION=p SIZE=notanumber\n").unwrap()
            ),
            Err(ParseError::InvalidToken)
        );
        // PROTOCOL belongs to RAW only; accepting it here would blur trust typing.
        assert_eq!(
            DatagramDelivery::parse_datagram(
                &parse_line(b"DATAGRAM RECEIVED DESTINATION=p SIZE=1 PROTOCOL=18\n").unwrap()
            ),
            Err(ParseError::InvalidToken)
        );
        // RAW never carries a source destination.
        assert_eq!(
            DatagramDelivery::parse_raw(
                &parse_line(b"RAW RECEIVED SIZE=1 DESTINATION=p\n").unwrap()
            ),
            Ok(DatagramDelivery {
                source: None,
                from_port: None,
                to_port: None,
                protocol: None,
                size: 1
            })
        );
        assert_eq!(
            DatagramDelivery::parse_raw(&parse_line(b"RAW RECEIVED SIZE=1 PROTOCOL=6\n").unwrap()),
            Err(ParseError::InvalidToken)
        );
        assert!(
            DatagramDelivery::parse_raw(
                &parse_line(b"DATAGRAM RECEIVED DESTINATION=p SIZE=1\n").unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn only_ordinary_datagram_and_raw_use_the_v1_v2_direct_modes() {
        assert!(SessionStyle::Datagram.supports_control_socket_datagram());
        assert!(SessionStyle::Raw.supports_control_socket_datagram());
        assert!(!SessionStyle::Datagram2.supports_control_socket_datagram());
        assert!(!SessionStyle::Datagram3.supports_control_socket_datagram());
        assert!(!SessionStyle::Stream.supports_control_socket_datagram());
        assert_eq!(
            SessionStyle::Datagram.source_trust(),
            DatagramSourceTrust::Authenticated
        );
        assert_eq!(
            SessionStyle::Datagram2.source_trust(),
            DatagramSourceTrust::Authenticated
        );
        assert_eq!(
            SessionStyle::Datagram3.source_trust(),
            DatagramSourceTrust::UnverifiedHash
        );
        assert_eq!(
            SessionStyle::Raw.source_trust(),
            DatagramSourceTrust::NoSource
        );
    }

    #[test]
    fn unsupported_style_is_distinct_from_transient_failure() {
        assert!(SamResult::parse("INVALID_STYLE").is_unsupported_style());
        assert!(!SamResult::parse("CANT_REACH_PEER").is_unsupported_style());
        assert!(!SamResult::parse("TIMEOUT").is_unsupported_style());
        assert!(!SamResult::parse("KEY_NOT_FOUND").is_unsupported_style());
        assert!(!SamResult::parse("PEER_NOT_FOUND").is_unsupported_style());
        assert!(!SamResult::parse("I2P_ERROR").is_unsupported_style());
        assert_eq!(
            Support::parse(Support::Unsupported.as_str()),
            Support::Unsupported
        );
        assert_eq!(Support::parse("supported"), Support::Supported);
        assert_eq!(Support::parse("nonsense"), Support::Unknown);
    }

    #[test]
    fn destination_hash_renders_canonical_hex() {
        let hash = DestinationHash::new([0xab; 32]);
        let text = hash.to_string();
        assert_eq!(text.len(), 64);
        assert!(text.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(&text[..4], "abab");
        assert!(format!("{hash:?}").contains(&text));
    }
}
