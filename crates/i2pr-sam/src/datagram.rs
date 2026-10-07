//! Datagram wire encoding and decoding for both documented transports.
//!
//! Ordinary DATAGRAM1/RAW traffic has two transports and they are not interchangeable:
//!
//! * UDP forwarding - the client sends `3.0 <id> <destination> ...\n<payload>` to the SAM
//!   datagram port and the bridge forwards received messages to `HOST:PORT`. With
//!   `HEADER=true` a forwarded RAW datagram is prefixed with a FROM_PORT/TO_PORT/PROTOCOL
//!   block; without it the payload arrives with no metadata at all.
//! * v1/v2-compatible control socket - `DATAGRAM SEND` / `RAW SEND` on the session socket,
//!   with unsolicited `DATAGRAM RECEIVED` / `RAW RECEIVED` lines followed by `SIZE` raw
//!   bytes. Only ordinary DATAGRAM1 and RAW may use it.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use i2pr_sam_proto::{
    AuthenticatedDatagram, Destination, I2pProtocol, Port, RawDatagram, ReceivedDatagram,
    SessionStyle, UnverifiedDatagram3, UnverifiedSourceHash,
};

use crate::SamError;

/// How one ordinary datagram session moves datagrams.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DatagramTransport {
    /// Send through the SAM datagram port and receive through a forwarded UDP socket.
    #[default]
    UdpForward,
    /// Send and receive through the session control socket, v1/v2-compatible.
    ControlSocketV1,
}

impl DatagramTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UdpForward => "udp_forward",
            Self::ControlSocketV1 => "control_socket_v1",
        }
    }
}

/// Reject transports the specification excludes, before any router state is created.
///
/// DATAGRAM2 and DATAGRAM3 never use the v1/v2-compatible mechanism, so offering it for
/// them would be a client-side protocol violation rather than a router capability question.
pub fn ensure_transport_supported(
    style: SessionStyle,
    transport: DatagramTransport,
) -> Result<(), SamError> {
    match (style, transport) {
        (_, DatagramTransport::UdpForward) => Ok(()),
        (style, DatagramTransport::ControlSocketV1) if style.supports_control_socket_datagram() => {
            Ok(())
        }
        (style, DatagramTransport::ControlSocketV1) => Err(SamError::Unsupported(format!(
            "{} sessions cannot use v1/v2-compatible control-socket datagram commands",
            style.as_wire()
        ))),
    }
}

/// Build the UDP-forwarded outbound frame.
///
/// `version_text` is the SAM version token the frame header carries; `3.x` is accepted from
/// SAM 3.2 onward and routers require `3.0` before that.
pub fn build_forwarded_frame(
    version_text: &str,
    session_id: &str,
    destination: &str,
    payload: &[u8],
    from_port: Option<Port>,
    to_port: Option<Port>,
    protocol: Option<I2pProtocol>,
) -> Result<Vec<u8>, SamError> {
    if !valid_session_id(session_id) || destination.is_empty() {
        return Err(SamError::Rejected(
            "invalid datagram frame arguments".into(),
        ));
    }
    if destination
        .bytes()
        .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err(SamError::Rejected(
            "invalid datagram destination token".into(),
        ));
    }
    let mut header = format!("{version_text} {session_id} {destination}");
    if let Some(port) = from_port {
        header.push_str(&format!(" FROM_PORT={}", port.get()));
    }
    if let Some(to_port) = to_port {
        header.push_str(&format!(" TO_PORT={}", to_port.get()));
    }
    if let Some(protocol) = protocol {
        header.push_str(&format!(" PROTOCOL={}", protocol.get()));
    }
    header.push('\n');
    let mut frame = Vec::with_capacity(header.len() + payload.len());
    frame.extend_from_slice(header.as_bytes());
    frame.extend_from_slice(payload);
    if frame.len() > 65_507 {
        return Err(SamError::Rejected(
            "SAM UDP frame exceeds IPv4 datagram limit".into(),
        ));
    }
    Ok(frame)
}

/// Build a v1/v2-compatible direct send command line; the payload follows as raw bytes.
pub fn build_direct_send_command(
    style: SessionStyle,
    session_id: &str,
    destination: &str,
    size: usize,
    from_port: Option<Port>,
    to_port: Option<Port>,
    protocol: Option<I2pProtocol>,
) -> Result<String, SamError> {
    if !style.supports_control_socket_datagram() {
        return Err(SamError::Unsupported(format!(
            "{} sessions cannot use v1/v2-compatible control-socket datagram commands",
            style.as_wire()
        )));
    }
    if !valid_session_id(session_id) {
        return Err(SamError::Rejected("invalid session ID".into()));
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
    if size == 0 {
        return Err(SamError::Rejected(
            "datagram payload outside configured bounds".into(),
        ));
    }
    let verb = if style == SessionStyle::Raw {
        "RAW SEND"
    } else {
        "DATAGRAM SEND"
    };
    let mut command = format!("{verb} ID={session_id} DESTINATION={destination}");
    if let Some(port) = from_port {
        command.push_str(&format!(" FROM_PORT={}", port.get()));
    }
    if let Some(to_port) = to_port {
        command.push_str(&format!(" TO_PORT={}", to_port.get()));
    }
    if let Some(protocol) = protocol {
        command.push_str(&format!(" PROTOCOL={}", protocol.get()));
    }
    command.push_str(&format!(" SIZE={size}\n"));
    Ok(command)
}

/// Metadata a forwarded RAW datagram can carry, depending on the requested header mode.
#[derive(Clone, Copy, Debug)]
pub struct ForwardedMetadata {
    /// Whether the session asked for `HEADER=true`. Without it no metadata is on the wire.
    pub header: bool,
    pub protocol: I2pProtocol,
}

/// Decode a UDP-forwarded datagram.
///
/// When `metadata.header` is false a forwarded RAW datagram is bare payload, so port and
/// protocol values come from the session defaults instead of the wire. Inventing a header
/// for that case would misreport what the router actually sent.
pub fn decode_forwarded_datagram(
    style: SessionStyle,
    metadata: ForwardedMetadata,
    bytes: &[u8],
) -> Result<ReceivedDatagram, SamError> {
    if style == SessionStyle::Raw {
        if !metadata.header {
            return Ok(ReceivedDatagram::Raw(RawDatagram {
                from_port: Port::new(0),
                to_port: Port::new(0),
                protocol: metadata.protocol,
                payload: bytes.to_vec(),
            }));
        }
        let split = bytes
            .windows(2)
            .position(|w| w == b"\n\n")
            .ok_or_else(|| SamError::Rejected("invalid RAW forwarding header".into()))?;
        let header = std::str::from_utf8(&bytes[..split])
            .map_err(|_| SamError::Rejected("invalid RAW forwarding header".into()))?;
        let mut from_port = Port::new(0);
        let mut to_port = Port::new(0);
        let mut protocol = metadata.protocol;
        for line in header.split('\n') {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let parsed = value
                .parse::<u16>()
                .map_err(|_| SamError::Rejected("invalid RAW header value".into()));
            match key {
                "FROM_PORT" => {
                    from_port = Port::new(
                        parsed.map_err(|_| SamError::Rejected("invalid source port".into()))?,
                    )
                }
                "TO_PORT" => {
                    to_port = Port::new(
                        parsed
                            .map_err(|_| SamError::Rejected("invalid destination port".into()))?,
                    )
                }
                "PROTOCOL" => {
                    protocol = I2pProtocol::new(
                        value
                            .parse()
                            .map_err(|_| SamError::Rejected("invalid I2P protocol".into()))?,
                    )
                    .map_err(SamError::Protocol)?;
                }
                _ => {}
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
        if let Some(value) = word.strip_prefix("FROM_PORT=") {
            from_port = Port::new(
                value
                    .parse()
                    .map_err(|_| SamError::Rejected("invalid source port".into()))?,
            );
        } else if let Some(value) = word.strip_prefix("TO_PORT=") {
            to_port = Port::new(
                value
                    .parse()
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

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_metadata(header: bool) -> ForwardedMetadata {
        ForwardedMetadata {
            header,
            protocol: I2pProtocol::new(18).expect("valid protocol"),
        }
    }

    #[test]
    fn forwarded_raw_without_header_is_bare_payload() {
        let decoded =
            decode_forwarded_datagram(SessionStyle::Raw, raw_metadata(false), b"anonymous-payload")
                .unwrap();
        match decoded {
            ReceivedDatagram::Raw(message) => {
                assert_eq!(message.payload, b"anonymous-payload");
                assert_eq!(message.protocol.get(), 18);
                assert_eq!(message.from_port.get(), 0);
            }
            other => panic!("unexpected trust shape: {other:?}"),
        }
    }

    #[test]
    fn forwarded_raw_with_header_keeps_ports_and_protocol() {
        let decoded = decode_forwarded_datagram(
            SessionStyle::Raw,
            raw_metadata(true),
            b"FROM_PORT=1\nTO_PORT=2\nPROTOCOL=18\n\npayload",
        )
        .unwrap();
        match decoded {
            ReceivedDatagram::Raw(message) => {
                assert_eq!(message.payload, b"payload");
                assert_eq!(message.from_port.get(), 1);
                assert_eq!(message.to_port.get(), 2);
                assert_eq!(message.protocol.get(), 18);
            }
            other => panic!("unexpected trust shape: {other:?}"),
        }
        assert!(
            decode_forwarded_datagram(
                SessionStyle::Raw,
                raw_metadata(true),
                b"payload-without-header"
            )
            .is_err()
        );
    }

    #[test]
    fn direct_send_commands_are_style_specific_and_bounded() {
        let command = build_direct_send_command(
            SessionStyle::Datagram,
            "dgram",
            "peer.b32.i2p",
            5,
            Some(Port::new(3)),
            Some(Port::new(4)),
            None,
        )
        .unwrap();
        assert_eq!(
            command,
            "DATAGRAM SEND ID=dgram DESTINATION=peer.b32.i2p FROM_PORT=3 TO_PORT=4 SIZE=5\n"
        );
        let raw = build_direct_send_command(
            SessionStyle::Raw,
            "raw",
            "peer",
            2,
            None,
            None,
            Some(I2pProtocol::new(18).unwrap()),
        )
        .unwrap();
        assert_eq!(raw, "RAW SEND ID=raw DESTINATION=peer PROTOCOL=18 SIZE=2\n");
        for style in [
            SessionStyle::Datagram2,
            SessionStyle::Datagram3,
            SessionStyle::Stream,
        ] {
            assert!(
                build_direct_send_command(style, "s", "peer", 1, None, None, None).is_err(),
                "{style:?} must not use v1/v2 direct send"
            );
            assert!(ensure_transport_supported(style, DatagramTransport::ControlSocketV1).is_err());
            assert!(ensure_transport_supported(style, DatagramTransport::UdpForward).is_ok());
        }
    }

    #[test]
    fn forwarded_frame_header_matches_specification_order() {
        let frame = build_forwarded_frame(
            "3.0",
            "dgram",
            "peer",
            b"payload",
            Some(Port::new(2)),
            Some(Port::new(3)),
            None,
        )
        .unwrap();
        assert_eq!(&frame[..], b"3.0 dgram peer FROM_PORT=2 TO_PORT=3\npayload");
        let raw_frame = build_forwarded_frame(
            "3.3",
            "raw",
            "peer",
            b"p",
            None,
            None,
            Some(I2pProtocol::new(18).unwrap()),
        )
        .unwrap();
        assert_eq!(raw_frame, b"3.3 raw peer PROTOCOL=18\np");
        // Protocol numbers reserved for other SAM styles are rejected outright.
        for reserved in [6u8, 17, 19, 20] {
            assert!(
                I2pProtocol::new(reserved).is_err(),
                "{reserved} must be rejected"
            );
        }
        assert!(build_forwarded_frame("3.0", "bad id", "peer", b"x", None, None, None).is_err());
        assert!(build_forwarded_frame("3.0", "ok", "pe er", b"x", None, None, None).is_err());
    }
}
