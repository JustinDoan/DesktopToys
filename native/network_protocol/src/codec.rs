use crate::{Envelope, PROTOCOL_VERSION};
use std::{error::Error, fmt};

pub const MAX_ENVELOPE_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum ProtocolCodecError {
    PayloadTooLarge { actual: usize, maximum: usize },
    InvalidJson(serde_json::Error),
    UnsupportedProtocol { received: u16, supported: u16 },
}

impl fmt::Display for ProtocolCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge { actual, maximum } => write!(
                f,
                "protocol payload is {actual} bytes; maximum is {maximum}"
            ),
            Self::InvalidJson(error) => write!(f, "invalid protocol JSON: {error}"),
            Self::UnsupportedProtocol {
                received,
                supported,
            } => write!(
                f,
                "unsupported protocol version {received}; supported version is {supported}"
            ),
        }
    }
}
impl Error for ProtocolCodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        if let Self::InvalidJson(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

pub fn encode_envelope(envelope: &Envelope) -> Result<Vec<u8>, ProtocolCodecError> {
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolCodecError::UnsupportedProtocol {
            received: envelope.protocol_version,
            supported: PROTOCOL_VERSION,
        });
    }
    let bytes = serde_json::to_vec(envelope).map_err(ProtocolCodecError::InvalidJson)?;
    ensure_size(bytes.len())?;
    Ok(bytes)
}

pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope, ProtocolCodecError> {
    ensure_size(bytes.len())?;
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(ProtocolCodecError::InvalidJson)?;
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolCodecError::UnsupportedProtocol {
            received: envelope.protocol_version,
            supported: PROTOCOL_VERSION,
        });
    }
    Ok(envelope)
}

fn ensure_size(actual: usize) -> Result<(), ProtocolCodecError> {
    if actual > MAX_ENVELOPE_BYTES {
        Err(ProtocolCodecError::PayloadTooLarge {
            actual,
            maximum: MAX_ENVELOPE_BYTES,
        })
    } else {
        Ok(())
    }
}
