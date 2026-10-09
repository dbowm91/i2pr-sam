//! Concrete session identity: the value that makes shared-session claims verifiable.
//!
//! A shared session only proves one linkability domain if the client can name the exact
//! Destination the router adopted. The request token `TRANSIENT` is not an identity: it is
//! echoed back by the client and proves nothing. This module therefore models identity as
//! the router-resolved public Destination plus its canonical SHA-256 hash, which is the
//! same hash I2P uses for Datagram3 source identification.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use i2pr_sam_proto::{Destination, DestinationHash, SecretDestination};
use sha2::{Digest, Sha256};

use crate::SamError;

/// Canonical SHA-256 of a base64 Destination's decoded bytes.
pub fn destination_hash(destination: &Destination) -> Result<DestinationHash, SamError> {
    // I2P Base64 substitutes '-' and '~' for the standard '+' and '/' characters.
    // SAM routers return Destinations in that alphabet, so normalize before using the
    // standard Base64 decoder. Padding remains '=' in both encodings.
    let standard_base64 = destination.as_str().replace('-', "+").replace('~', "/");
    let bytes = BASE64
        .decode(standard_base64)
        .map_err(|_| SamError::Rejected("Destination is not valid base64".into()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(DestinationHash::new(hasher.finalize().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn destination_hash_accepts_the_i2p_base64_alphabet() {
        let standard = STANDARD.encode([0xfb, 0xef, 0xff]);
        assert_eq!(standard, "++//");
        let i2p = Destination::new("--~~").unwrap();
        assert_eq!(
            destination_hash(&i2p).unwrap(),
            destination_hash(&Destination::new(standard).unwrap()).unwrap()
        );
    }
}

/// A concrete public Destination together with its canonical hash.
///
/// `Debug` prints the hash and the destination, never key material, so identity may be
/// recorded in logs and conformance artifacts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionIdentity {
    destination: Destination,
    hash: DestinationHash,
}

impl SessionIdentity {
    pub fn new(destination: Destination) -> Result<Self, SamError> {
        let hash = destination_hash(&destination)?;
        Ok(Self { destination, hash })
    }

    pub fn parse(text: &str) -> Result<Self, SamError> {
        Self::new(Destination::new(text).map_err(|_| {
            SamError::Rejected("session identity is not a valid Destination".into())
        })?)
    }

    pub fn destination(&self) -> &Destination {
        &self.destination
    }

    pub fn hash(&self) -> &DestinationHash {
        &self.hash
    }
}

/// Public/private Destination pair produced by `DEST GENERATE`.
#[derive(Clone)]
pub struct GeneratedDestination {
    public: Destination,
    secret: SecretDestination,
}

impl GeneratedDestination {
    pub fn new(public: Destination, secret: SecretDestination) -> Self {
        Self { public, secret }
    }

    pub fn public(&self) -> &Destination {
        &self.public
    }

    pub fn secret(&self) -> &SecretDestination {
        &self.secret
    }

    pub fn into_parts(self) -> (Destination, SecretDestination) {
        (self.public, self.secret)
    }
}

impl std::fmt::Debug for GeneratedDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedDestination")
            .field("public", &self.public.as_str())
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// Which Destination a session should adopt.
#[derive(Clone, Debug, Default)]
pub enum SessionDestination {
    /// Let the router generate a Destination.
    ///
    /// The caller must then resolve the resulting concrete identity; returning the literal
    /// request token is never acceptable evidence of a shared linkability domain.
    #[default]
    Transient,
    /// Generate a keypair on the utility connection and import it.
    ///
    /// The identity is concrete before the session exists, so no router behaviour is
    /// required to prove it.
    Generated,
    /// Import a public Destination the caller already controls.
    Imported(Destination),
    /// Import public Destination plus private key material, such as an offline-signed one.
    WithKey {
        public: Destination,
        secret: SecretDestination,
    },
}

impl SessionDestination {
    /// The `DESTINATION=` wire value, or `None` when the router must generate one.
    pub fn wire_value(&self) -> Option<String> {
        match self {
            Self::Transient => None,
            Self::Generated => None,
            Self::Imported(public) => Some(public.as_str().to_owned()),
            Self::WithKey { secret, .. } => Some(secret.expose().to_owned()),
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient)
    }

    /// Reject tokens that are not Destination text before they reach the wire.
    pub fn validate(&self) -> Result<(), SamError> {
        match self {
            Self::Transient | Self::Generated => Ok(()),
            Self::Imported(public) => {
                destination_hash(public)?;
                Ok(())
            }
            Self::WithKey { public, .. } => {
                destination_hash(public)?;
                Ok(())
            }
        }
    }
}

/// Why a session identity could not be established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityFailure {
    /// The router did not return a Destination for this session.
    NotReported,
    /// The router returned text that is not a usable Destination.
    Malformed,
    /// `NAMING LOOKUP NAME=ME` failed even though the session exists.
    LookupFailed,
}
