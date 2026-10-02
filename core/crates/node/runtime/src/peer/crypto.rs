//! Runtime 的密钥交换和 AEAD；PeerLink 始终只看到标准 Link Call。
use operit_link::*;
use operit_peer_link::{PeerConnection, PeerMessage};
use ring::{aead, agreement, digest, hkdf, hmac, rand::{SecureRandom, SystemRandom}};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) fn error(message: impl Into<String>) -> CoreLinkError {
    CoreLinkError::new("PEER_SECURITY", message)
}
pub(super) fn random() -> Result<[u8; 32], CoreLinkError> {
    let mut bytes = [0; 32];
    SystemRandom::new().fill(&mut bytes).map_err(|_| error("Random source failed"))?;
    Ok(bytes)
}
pub(super) fn ephemeral() -> Result<(agreement::EphemeralPrivateKey, Vec<u8>), CoreLinkError> {
    let key = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
        .map_err(|_| error("X25519 key generation failed"))?;
    let public = key.compute_public_key().map_err(|_| error("X25519 public key failed"))?;
    Ok((key, public.as_ref().to_vec()))
}
pub(super) fn agree(key: agreement::EphemeralPrivateKey, public: &[u8]) -> Result<Vec<u8>, CoreLinkError> {
    agreement::agree_ephemeral(key, &agreement::UnparsedPublicKey::new(&agreement::X25519, public), |bytes| bytes.to_vec())
        .map_err(|_| error("Invalid X25519 public key"))
}
struct KeyLength;
impl hkdf::KeyType for KeyLength { fn len(&self) -> usize { 32 } }
pub(super) fn derive(secret: &[u8], salt: &[u8], label: &[u8]) -> Result<[u8; 32], CoreLinkError> {
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(secret);
    let labels = [label];
    let mut key = [0; 32];
    prk.expand(&labels, KeyLength).map_err(|_| error("HKDF expansion failed"))?
        .fill(&mut key).map_err(|_| error("HKDF failed"))?;
    Ok(key)
}
pub(super) fn proof(key: &[u8], context: &[u8], role: &[u8]) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    let mut message = context.to_vec(); message.extend_from_slice(role);
    hmac::sign(&key, &message).as_ref().to_vec()
}
pub(super) fn verify(key: &[u8], context: &[u8], role: &[u8], supplied: &[u8]) -> Result<(), CoreLinkError> {
    let mut message = context.to_vec(); message.extend_from_slice(role);
    hmac::verify(&hmac::Key::new(hmac::HMAC_SHA256, key), &message, supplied)
        .map_err(|_| error("Key/token proof rejected"))
}
/// A receiver-only, fixed-width code; leading zeroes are significant.
pub(super) fn pairingCode() -> Result<String, CoreLinkError> {
    Ok(format!("{:06}", u32::from_be_bytes(random()?[..4].try_into().unwrap()) % 1_000_000))
}

#[cfg(test)]
mod pairing_code_tests {
    #[test]
    fn receiver_code_is_six_ascii_digits() {
        for _ in 0..100 {
            let code = super::pairingCode().unwrap();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
        }
    }
}
/// 整个 transcript 编码绑定版本、双方身份、两份公钥和服务端随机挑战。
#[derive(Serialize, Deserialize)]
pub(super) struct Transcript {
    pub version: u32, pub sessionId: String,
    pub clientNodeId: String, pub serverNodeId: String,
    pub clientPublic: Vec<u8>, pub serverPublic: Vec<u8>, pub challenge: Vec<u8>,
}
pub(super) fn transcript(value: &Transcript) -> Result<Vec<u8>, CoreLinkError> {
    let bytes = encodeLink(value).map_err(|e| error(e.to_string()))?;
    Ok(digest::digest(&digest::SHA256, &bytes).as_ref().to_vec())
}
struct Cipher { key: aead::LessSafeKey, next: u64, context: Vec<u8> }
impl Cipher {
    fn new(key: [u8; 32], context: &[u8]) -> Result<Self, CoreLinkError> {
        Ok(Self { key: aead::LessSafeKey::new(aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key)
            .map_err(|_| error("AEAD initialization failed"))?), next: 0, context: context.to_vec() })
    }
    fn nonce(&mut self) -> Result<aead::Nonce, CoreLinkError> {
        let next = self.next; self.next = next.checked_add(1).ok_or_else(|| error("Session nonce exhausted"))?;
        let mut nonce = [0; 12]; nonce[4..].copy_from_slice(&next.to_be_bytes());
        Ok(aead::Nonce::assume_unique_for_key(nonce))
    }
}
/// 两方向独立密钥、单调 nonce。收到明文、乱序、重放或错误 tag 一律断开。
pub(super) struct Channel {
    pub raw: Arc<dyn PeerConnection>,
    send: Mutex<Cipher>, receive: Mutex<Cipher>,
    pub transaction: Mutex<()>,
}
impl Channel {
    pub fn new(raw: Arc<dyn PeerConnection>, key: &[u8], context: &[u8], client: bool) -> Result<Arc<Self>, CoreLinkError> {
        let c2s = derive(key, context, b"operit-link-client-to-server-v1")?;
        let s2c = derive(key, context, b"operit-link-server-to-client-v1")?;
        Ok(Arc::new(Self { raw, send: Mutex::new(Cipher::new(if client { c2s } else { s2c }, context)?),
            receive: Mutex::new(Cipher::new(if client { s2c } else { c2s }, context)?), transaction: Mutex::new(()) }))
    }
    pub async fn send(&self, message: PeerMessage) -> Result<(), CoreLinkError> {
        let mut cipher = self.send.lock().await;
        let mut bytes = encodeLink(message).map_err(|e| error(e.to_string()))?;
        let sequence = cipher.next;
        let nonce = cipher.nonce()?;
        cipher.key.seal_in_place_append_tag(nonce, aead::Aad::from(&cipher.context), &mut bytes)
            .map_err(|_| error("Encryption failed"))?;
        self.raw.send(PeerMessage::Request(CoreLinkRequest::Call(CoreCallRequest::new(
            sequence.to_string(), "$peer.session", "data", CoreValue::Bytes(bytes))))).await.map_err(error)
    }
    pub async fn receive(&self) -> Result<Option<PeerMessage>, CoreLinkError> {
        let mut cipher = self.receive.lock().await;
        let Some(message) = self.raw.receive().await.map_err(error)? else { return Ok(None); };
        let PeerMessage::Request(CoreLinkRequest::Call(request)) = message else { return Err(error("Encrypted Call required")); };
        if request.target != "$peer.session" || request.methodName != "data" || request.requestId.0 != cipher.next.to_string() {
            return Err(error("Unexpected/replayed session message"));
        }
        let CoreValue::Bytes(mut bytes) = request.args else { return Err(error("Encrypted bytes required")); };
        let nonce = cipher.nonce()?;
        let plaintext = cipher.key.open_in_place(nonce, aead::Aad::from(&cipher.context), &mut bytes)
            .map_err(|_| error("Session authentication tag rejected"))?;
        decodeLink(plaintext).map(Some).map_err(|e| error(e.to_string()))
    }
    pub async fn exchange(&self, request: CoreLinkRequest) -> Result<CoreLinkResponse, CoreLinkError> {
        let _guard = self.transaction.lock().await;
        self.send(PeerMessage::Request(request)).await?;
        match self.receive().await? {
            Some(PeerMessage::Response(response)) => Ok(response),
            _ => Err(error("Missing Link response")),
        }
    }
}
