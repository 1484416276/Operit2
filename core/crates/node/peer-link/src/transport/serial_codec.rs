#![allow(non_snake_case)]

use operit_link::{encodeLink};
use crate::{LinkFrame};

/// Magic prefix used to resynchronize a serial carrier after boot logs or a
/// partially received frame.
pub const SERIAL_MAGIC: [u8; 4] = [0x4f, 0x50, 0x4c, 0x4b];
pub const SERIAL_VERSION: u8 = 1;
pub const SERIAL_HEADER_BYTES: usize = 13;
pub const MAX_SERIAL_FRAME_BYTES: usize = 256 * 1024;

/// Encodes one Link frame into the shared serial wire format.
pub fn encodeSerialFrame(frame: &LinkFrame) -> Result<Vec<u8>, String> {
    let payload = encodeLink(frame).map_err(|error| error.to_string())?;
    if payload.is_empty() || payload.len() > MAX_SERIAL_FRAME_BYTES {
        return Err(format!("invalid serial frame size: {}", payload.len()));
    }
    let length =
        u32::try_from(payload.len()).map_err(|_| "serial frame exceeds u32 length".to_string())?;
    let checksum = crc32(&payload);
    let mut output = Vec::with_capacity(SERIAL_HEADER_BYTES + payload.len());
    output.extend_from_slice(&SERIAL_MAGIC);
    output.push(SERIAL_VERSION);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(&checksum.to_be_bytes());
    output.extend_from_slice(&payload);
    Ok(output)
}

/// Incremental decoder for serial bytes. Invalid input is discarded until the
/// next valid magic, length, checksum, and Link payload are found.
pub struct SerialFrameDecoder {
    buffer: Vec<u8>,
    maxFrameBytes: usize,
}

impl SerialFrameDecoder {
    pub fn new() -> Self {
        Self::withMaxFrameBytes(MAX_SERIAL_FRAME_BYTES)
    }

    /// Applies the receiving device's memory budget without changing the wire format.
    pub fn withMaxFrameBytes(maxFrameBytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            maxFrameBytes: maxFrameBytes.clamp(1, MAX_SERIAL_FRAME_BYTES),
        }
    }

    /// Feeds bytes and returns the first complete frame, if one is available.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Option<LinkFrame>, String> {
        let bufferLimit = SERIAL_HEADER_BYTES + self.maxFrameBytes + 1024;
        if bytes.len() > bufferLimit || self.buffer.len().saturating_add(bytes.len()) > bufferLimit
        {
            self.buffer.clear();
            return Err("serial input exceeds bounded decoder buffer".to_string());
        }
        self.buffer
            .try_reserve(bytes.len())
            .map_err(|_| "serial decoder cannot reserve input buffer".to_string())?;
        self.buffer.extend_from_slice(bytes);
        loop {
            let Some(magicOffset) = findMagic(&self.buffer) else {
                retainMagicPrefix(&mut self.buffer);
                return Ok(None);
            };
            if magicOffset > 0 {
                self.buffer.drain(..magicOffset);
            }
            if self.buffer.len() < SERIAL_HEADER_BYTES {
                return Ok(None);
            }
            if self.buffer[4] != SERIAL_VERSION {
                self.buffer.drain(..1);
                continue;
            }
            let length = u32::from_be_bytes(
                self.buffer[5..9]
                    .try_into()
                    .expect("serial header length is fixed"),
            ) as usize;
            let expectedChecksum = u32::from_be_bytes(
                self.buffer[9..13]
                    .try_into()
                    .expect("serial header checksum is fixed"),
            );
            if length == 0 || length > self.maxFrameBytes {
                self.buffer.clear();
                return Err(format!("serial frame exceeds receive limit: {length}"));
            }
            let frameBytes = SERIAL_HEADER_BYTES + length;
            if self.buffer.len() < frameBytes {
                return Ok(None);
            }
            let payload = &self.buffer[SERIAL_HEADER_BYTES..frameBytes];
            if crc32(payload) != expectedChecksum {
                self.buffer.drain(..frameBytes);
                continue;
            }
            let frame = operit_link::decodeLink(payload)
                .map(Some)
                .map_err(|error| format!("decode serial Link frame: {error}"));
            self.buffer.drain(..frameBytes);
            return frame;
        }
    }
}

impl Default for SerialFrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

fn findMagic(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(SERIAL_MAGIC.len())
        .position(|window| window == SERIAL_MAGIC)
}

fn retainMagicPrefix(bytes: &mut Vec<u8>) {
    let keep = SERIAL_MAGIC.len().saturating_sub(1);
    if bytes.len() > keep {
        let start = bytes.len() - keep;
        bytes.drain(..start);
    }
}

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

use crate::{LinkFramePayload};

    fn testFrame() -> LinkFrame {
        LinkFrame {
            messageId: "serial-test".to_string(),
            payload: LinkFramePayload::PeerFrame(operit_link::PeerFrame { messageId: "heartbeat-test".into(), payload: operit_link::PeerFramePayload::Heartbeat(operit_link::PeerHeartbeat::Probe { sequence: 7, sentAt: 0 }) }),
        }
    }

    #[test]
    fn decoderResynchronizesAfterLogBytes() {
        let encoded = encodeSerialFrame(&testFrame()).expect("frame must encode");
        let mut decoder = SerialFrameDecoder::new();
        assert!(decoder.push(b"I (123) booting\r\n").unwrap().is_none());
        assert!(decoder.push(&encoded[..8]).unwrap().is_none());
        assert_eq!(decoder.push(&encoded[8..]).unwrap(), Some(testFrame()));
    }

    #[test]
    fn decoderRejectsBadChecksumAndContinues() {
        let mut invalid = encodeSerialFrame(&testFrame()).expect("frame must encode");
        invalid[SERIAL_HEADER_BYTES] ^= 0x01;
        let valid = encodeSerialFrame(&testFrame()).expect("frame must encode");
        let mut decoder = SerialFrameDecoder::new();
        let mut bytes = invalid;
        bytes.extend_from_slice(&valid);
        assert_eq!(decoder.push(&bytes).unwrap(), Some(testFrame()));
    }

    #[test]
    fn decoderHandlesPayloadContainingMagic() {
        let frame = LinkFrame {
            messageId: "magic-payload".to_string(),
            payload: LinkFramePayload::Close { code: "test".into(), message: "OPLK".into() },
        };
        let encoded = encodeSerialFrame(&frame).expect("frame must encode");
        let mut decoder = SerialFrameDecoder::new();
        assert_eq!(decoder.push(&encoded).unwrap(), Some(frame));
    }
}
