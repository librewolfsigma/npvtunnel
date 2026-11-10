//! The `.npvt` v2 container: real cryptography for shared tunnel configs.
//!
//! Design (concept blueprint, Section 9):
//!
//! ```text
//! MAGIC "NPVT2\0" | u16 version (LE) | u32 header_len (LE) | u32 payload_len (LE)
//!                 | header (canonical JSON, UTF-8)   | payload blob
//! ```
//!
//! - **Passphrase lock**: Argon2id (default m=64 MiB, t=3, p=4) stretches
//!   the passphrase into an XChaCha20-Poly1305 AEAD over the JSON payload;
//!   AAD = canonical
//!   header bytes with the signature cleared (and, for key locks, all slot
//!   ciphertexts cleared).
//! - **Key lock**: per-recipient X25519 (ephemeral sender key) -> HKDF-SHA-256
//!   -> AEAD; each slot carries its own salt/nonce/ciphertext.
//! - **Open mode**: payload is plaintext JSON; integrity comes from the
//!   creator signature.
//! - **Signature**: Ed25519 (`verify_strict`) over
//!   `b"npvt2-sign-v1" || SHA-256(aad) || u32 payload_len || SHA-256(payload)`.
//!   A present-but-invalid signature is a hard failure (fail closed).
//!
//! Authentication failures are deliberately indistinguishable: callers get
//! [`Error::CryptoAuth`] whether the passphrase is wrong or a byte was
//! tampered with.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};
use zeroize::Zeroize;

use crate::error::{Error, Result};
use crate::profile::{unix_now, Profile};

/// Container magic.
pub const MAGIC: &[u8; 6] = b"NPVT2\0";
/// Container format version implemented by this module.
pub const FORMAT_VERSION: u16 = 1;
/// Maximum accepted container size on decode.
pub const MAX_CONTAINER_LEN: usize = 8 * 1024 * 1024;
/// Maximum base64 length accepted on `npvt://` links (4/3 of the above).
pub const MAX_CONTAINER_B64_LEN: usize = (MAX_CONTAINER_LEN / 3 + 1) * 4;
/// Maximum header (JSON) size.
pub const MAX_HEADER_LEN: usize = 256 * 1024;
/// Maximum number of key-lock recipients per container.
pub const MAX_RECIPIENTS: usize = 16;

const SIGN_DOMAIN: &[u8] = b"npvt2-sign-v1";
const KDF_INFO_RECIPIENT: &[u8] = b"npvt2-recipient-v1";
const FRAME_OVERHEAD: usize = 6 + 2 + 4 + 4;

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

fn unb64(data: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .map_err(|_| Error::Crypto("invalid base64 field in container".into()))
}

fn le_u32(n: usize) -> Result<[u8; 4]> {
    u32::try_from(n)
        .map(u32::to_le_bytes)
        .map_err(|_| Error::Crypto("container size exceeds u32".into()))
}

/// Argon2id parameters stored in the header. Defaults: m=64 MiB, t=3, p=4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    /// Always `argon2id`.
    pub alg: String,
    /// Memory cost in KiB.
    pub m_kib: u32,
    /// Time cost (iterations).
    pub t: u32,
    /// Parallelism (lanes).
    pub p: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        KdfParams {
            alg: "argon2id".to_string(),
            m_kib: 65_536,
            t: 3,
            p: 4,
        }
    }
}

impl KdfParams {
    fn validate(&self) -> Result<()> {
        if self.alg != "argon2id" {
            return Err(Error::Crypto("unsupported KDF algorithm".into()));
        }
        if !(8_192..=1_048_576).contains(&self.m_kib)
            || !(1..=10).contains(&self.t)
            || !(1..=8).contains(&self.p)
        {
            return Err(Error::Crypto("KDF parameters out of accepted range".into()));
        }
        Ok(())
    }

    fn derive(&self, passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
        let params = Params::new(self.m_kib, self.t, self.p, Some(32))
            .map_err(|_| Error::Crypto("invalid KDF params".into()))?;
        let mut key = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|_| Error::Crypto("KDF failed".into()))?;
        Ok(key)
    }
}

fn aead_encrypt(key: &[u8; 32], nonce: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let nonce: [u8; 24] = nonce
        .try_into()
        .map_err(|_| Error::Crypto("bad nonce length".into()))?;
    XChaCha20Poly1305::new(key.into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| Error::Crypto("encryption failed".into()))
}

fn aead_decrypt(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let nonce: [u8; 24] = nonce
        .try_into()
        .map_err(|_| Error::Crypto("bad nonce length".into()))?;
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| Error::CryptoAuth)
}

/// Share policy carried inside the signed header.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Unix seconds after which this config is considered expired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// Creator asks that this config only run on metered connections.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub metered_only: bool,
    /// Suggested group label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_hint: Option<String>,
}

impl Policy {
    /// True when `expires_at` is set and in the past relative to `now_unix`.
    pub fn is_expired(&self, now_unix: u64) -> bool {
        self.expires_at.is_some_and(|e| e <= now_unix)
    }
}

/// One key-lock recipient slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipientSlot {
    /// Recipient X25519 public key (base64url).
    pub recipient_pub: String,
    /// Ephemeral sender X25519 public key (base64url).
    pub eph_pub: String,
    /// HKDF salt (base64url).
    pub salt: String,
    /// AEAD nonce (base64url).
    pub nonce: String,
    /// AEAD ciphertext (base64url).
    pub ct: String,
}

/// Lock mode description stored in the header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
pub enum CryptoHeader {
    /// Passphrase lock: Argon2id + XChaCha20-Poly1305.
    Passphrase {
        kdf: KdfParams,
        salt: String,
        nonce: String,
    },
    /// Key lock: one X25519 slot per recipient.
    Keys { recipients: Vec<RecipientSlot> },
    /// Open: payload is plaintext JSON; integrity from the signature only.
    Open,
}

impl CryptoHeader {
    /// The AAD form: key-lock slots with ciphertexts cleared.
    fn normalized_for_aad(&self) -> CryptoHeader {
        match self {
            CryptoHeader::Keys { recipients } => CryptoHeader::Keys {
                recipients: recipients
                    .iter()
                    .map(|r| RecipientSlot {
                        recipient_pub: r.recipient_pub.clone(),
                        eph_pub: r.eph_pub.clone(),
                        salt: r.salt.clone(),
                        nonce: r.nonce.clone(),
                        ct: String::new(),
                    })
                    .collect(),
            },
            other => other.clone(),
        }
    }
}

/// Creator signature block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureBlock {
    /// Always `ed25519`.
    pub alg: String,
    /// Creator Ed25519 public key (base64url).
    pub pubkey: String,
    /// Signature value (base64url).
    pub value: String,
}

/// Payload encoding description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadDescr {
    /// `xchacha20poly1305` (locked modes) or `none` (open mode).
    pub alg: String,
    /// Always `json`.
    pub encoding: String,
}

/// The container header. Canonical JSON = serde field order, so both sides
/// derive identical AAD/signature inputs from the same logical header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    /// Always `npvt2`.
    pub format: String,
    /// [`FORMAT_VERSION`].
    pub version: u16,
    /// Creation time, unix seconds.
    pub created: u64,
    /// Signed share policy.
    pub policy: Policy,
    /// Lock mode description.
    pub crypto: CryptoHeader,
    /// Payload encoding description.
    pub payload: PayloadDescr,
    /// Creator signature, if signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<SignatureBlock>,
}

#[derive(Serialize, Deserialize)]
struct SecureData {
    profile: Profile,
}

/// How to lock a container on encode.
pub enum Lock {
    /// "Anyone with the app": plaintext payload, signed if a signer is given.
    Open,
    /// Passphrase lock (Argon2id + AEAD).
    Passphrase(String),
    /// Key lock for one or more recipient X25519 public keys.
    Keys(Vec<[u8; 32]>),
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Lock::Passphrase(passphrase) = self {
            passphrase.zeroize();
        }
    }
}

/// How to unlock a container on decode.
#[derive(Clone, Copy)]
pub enum Unlock<'a> {
    /// Only valid for Open containers.
    Any,
    /// Passphrase for Passphrase-locked containers.
    Passphrase(&'a str),
    /// One or more X25519 secrets; the matching slot is selected automatically.
    Keys(&'a [[u8; 32]]),
}

/// Signature verification outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    /// The container carries a creator signature.
    pub signed: bool,
    /// Creator Ed25519 public key (base64url), if signed.
    pub creator: Option<String>,
    /// The signature verified (always true when `signed`; decode fails closed).
    pub valid: bool,
}

/// A successfully decoded container.
#[derive(Debug, Clone)]
pub struct Decoded {
    /// The tunnel profile.
    pub profile: Profile,
    /// The signed share policy.
    pub policy: Policy,
    /// Signature verification outcome.
    pub verification: Verification,
}

/// Generate a new creator signing key (Ed25519).
pub fn generate_signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

/// Generate a recipient keypair: `(secret, public)` X25519 keys.
pub fn generate_recipient_keypair() -> ([u8; 32], [u8; 32]) {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (*secret.as_bytes(), *public.as_bytes())
}

fn canonical_header(header: &Header) -> Result<Vec<u8>> {
    serde_json::to_vec(header)
        .map_err(|e| Error::Crypto(format!("header serialization failed: {e}")))
}

fn aad_of(header: &Header) -> Result<Vec<u8>> {
    canonical_header(&Header {
        crypto: header.crypto.normalized_for_aad(),
        sig: None,
        ..header.clone()
    })
}

fn sign_input(aad: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(SIGN_DOMAIN.len() + 32 + 4 + 32);
    buf.extend_from_slice(SIGN_DOMAIN);
    buf.extend_from_slice(&Sha256::digest(aad));
    buf.extend_from_slice(&le_u32(payload.len()).expect("payload < 4 GiB"));
    buf.extend_from_slice(&Sha256::digest(payload));
    buf
}

/// Encode a profile into a v2 container.
///
/// When a signer is supplied the container is signed (strongly recommended);
/// a tampered or unsigned container is always detectable on decode.
pub fn encode(
    profile: &Profile,
    policy: &Policy,
    lock: &Lock,
    signer: Option<&SigningKey>,
) -> Result<Vec<u8>> {
    profile.validate()?;
    let payload_json = serde_json::to_vec(&SecureData {
        profile: profile.clone(),
    })?;

    let mut header = Header {
        format: "npvt2".to_string(),
        version: FORMAT_VERSION,
        created: unix_now(),
        policy: policy.clone(),
        crypto: CryptoHeader::Open,
        payload: PayloadDescr {
            alg: "xchacha20poly1305".to_string(),
            encoding: "json".to_string(),
        },
        sig: None,
    };

    let payload_blob: Vec<u8> = match lock {
        Lock::Open => {
            header.payload.alg = "none".to_string();
            header.crypto = CryptoHeader::Open;
            payload_json
        }
        Lock::Passphrase(passphrase) => {
            let kdf = KdfParams::default();
            let mut salt = [0u8; 16];
            let mut nonce = [0u8; 24];
            OsRng.fill_bytes(&mut salt);
            OsRng.fill_bytes(&mut nonce);
            let key = kdf.derive(passphrase, &salt)?;
            header.crypto = CryptoHeader::Passphrase {
                kdf,
                salt: b64(&salt),
                nonce: b64(&nonce),
            };
            let aad = aad_of(&header)?;
            aead_encrypt(&key, &nonce, &payload_json, &aad)?
        }
        Lock::Keys(recipient_secrets) => {
            if recipient_secrets.is_empty() || recipient_secrets.len() > MAX_RECIPIENTS {
                return Err(Error::Crypto(format!(
                    "key lock needs 1..={MAX_RECIPIENTS} recipients"
                )));
            }
            let mut slots: Vec<RecipientSlot> = Vec::with_capacity(recipient_secrets.len());
            let mut keys: Vec<[u8; 32]> = Vec::with_capacity(recipient_secrets.len());
            for secret in recipient_secrets {
                let recipient_pub = PublicKey::from(*secret);
                let eph = EphemeralSecret::random_from_rng(OsRng);
                let eph_pub = PublicKey::from(&eph);
                let shared = eph.diffie_hellman(&recipient_pub);
                let mut salt = [0u8; 16];
                let mut nonce = [0u8; 24];
                OsRng.fill_bytes(&mut salt);
                OsRng.fill_bytes(&mut nonce);
                let hk = Hkdf::<Sha256>::new(Some(&salt), shared.as_bytes());
                let mut key = [0u8; 32];
                hk.expand(KDF_INFO_RECIPIENT, &mut key)
                    .map_err(|_| Error::Crypto("HKDF expand failed".into()))?;
                slots.push(RecipientSlot {
                    recipient_pub: b64(recipient_pub.as_bytes()),
                    eph_pub: b64(eph_pub.as_bytes()),
                    salt: b64(&salt),
                    nonce: b64(&nonce),
                    ct: String::new(),
                });
                keys.push(key);
            }
            // First pass: header with empty ciphertexts fixes the AAD.
            header.crypto = CryptoHeader::Keys {
                recipients: slots.clone(),
            };
            let aad = aad_of(&header)?;
            // Second pass: encrypt per slot, then store the filled slots.
            for (slot, key) in slots.iter_mut().zip(keys.iter()) {
                let nonce = unb64(&slot.nonce)?;
                slot.ct = b64(&aead_encrypt(key, &nonce, &payload_json, &aad)?);
            }
            header.crypto = CryptoHeader::Keys { recipients: slots };
            Vec::new()
        }
    };

    if let Some(sk) = signer {
        let aad = aad_of(&header)?;
        let sig = sk.sign(&sign_input(&aad, &payload_blob));
        header.sig = Some(SignatureBlock {
            alg: "ed25519".to_string(),
            pubkey: b64(sk.verifying_key().as_bytes()),
            value: b64(&sig.to_bytes()),
        });
    }

    let header_bytes = canonical_header(&header)?;
    if header_bytes.len() > MAX_HEADER_LEN {
        return Err(Error::Crypto("header exceeds maximum size".into()));
    }

    let mut out = Vec::with_capacity(FRAME_OVERHEAD + header_bytes.len() + payload_blob.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&le_u32(header_bytes.len())?);
    out.extend_from_slice(&le_u32(payload_blob.len())?);
    out.extend_from_slice(&header_bytes);
    out.extend_from_slice(&payload_blob);
    Ok(out)
}

/// Peek at the lock mode without unlocking: `"passphrase"`, `"keys"` or
/// `"open"`.
pub fn peek_lock(bytes: &[u8]) -> Result<&'static str> {
    let (header, _) = parse_frame(bytes)?;
    Ok(match header.crypto {
        CryptoHeader::Passphrase { .. } => "passphrase",
        CryptoHeader::Keys { .. } => "keys",
        CryptoHeader::Open => "open",
    })
}

fn parse_frame(bytes: &[u8]) -> Result<(Header, &[u8])> {
    if bytes.len() < FRAME_OVERHEAD || bytes.len() > MAX_CONTAINER_LEN {
        return Err(Error::InvalidConfig("container size out of range".into()));
    }
    if &bytes[..6] != MAGIC {
        return Err(Error::InvalidConfig(
            "not an npvt2 container (magic mismatch)".into(),
        ));
    }
    let version = u16::from_le_bytes([bytes[6], bytes[7]]);
    if version != FORMAT_VERSION {
        return Err(Error::InvalidConfig(format!(
            "unsupported container version {version}"
        )));
    }
    let hdr_len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let pay_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if hdr_len == 0 || hdr_len > MAX_HEADER_LEN {
        return Err(Error::InvalidConfig("header length out of range".into()));
    }
    if FRAME_OVERHEAD
        .checked_add(hdr_len)
        .and_then(|n| n.checked_add(pay_len))
        != Some(bytes.len())
    {
        return Err(Error::InvalidConfig(
            "container framing is inconsistent".into(),
        ));
    }
    let header_bytes = &bytes[FRAME_OVERHEAD..FRAME_OVERHEAD + hdr_len];
    let payload = &bytes[FRAME_OVERHEAD + hdr_len..];
    let header: Header = serde_json::from_slice(header_bytes)
        .map_err(|e| Error::InvalidConfig(format!("container header is invalid: {e}")))?;
    if header.format != "npvt2" {
        return Err(Error::InvalidConfig(
            "container header format mismatch".into(),
        ));
    }
    Ok((header, payload))
}

/// Decode and unlock a v2 container.
///
/// Fails closed: a present signature that does not verify, a tampered payload,
/// or a wrong passphrase/key all yield [`Error::CryptoAuth`].
pub fn decode(bytes: &[u8], unlock: Unlock) -> Result<Decoded> {
    let (header, payload) = parse_frame(bytes)?;

    let verification = match &header.sig {
        Some(sig) => {
            if sig.alg != "ed25519" {
                return Err(Error::Crypto("unsupported signature algorithm".into()));
            }
            let pubkey_bytes: [u8; 32] = unb64(&sig.pubkey)?
                .try_into()
                .map_err(|_| Error::Crypto("bad signature pubkey length".into()))?;
            let vk = VerifyingKey::from_bytes(&pubkey_bytes)
                .map_err(|_| Error::Crypto("bad signature pubkey".into()))?;
            let sig_bytes: [u8; 64] = unb64(&sig.value)?
                .try_into()
                .map_err(|_| Error::Crypto("bad signature length".into()))?;
            let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
            vk.verify_strict(&sign_input(&aad_of(&header)?, payload), &signature)
                .map_err(|_| Error::CryptoAuth)?;
            Verification {
                signed: true,
                creator: Some(sig.pubkey.clone()),
                valid: true,
            }
        }
        None => Verification {
            signed: false,
            creator: None,
            valid: false,
        },
    };

    let aad = aad_of(&header)?;

    let plaintext: Vec<u8> = match (&header.crypto, unlock) {
        (CryptoHeader::Open, _) => {
            if header.payload.alg != "none" {
                return Err(Error::Crypto(
                    "open container with wrong payload alg".into(),
                ));
            }
            payload.to_vec()
        }
        (CryptoHeader::Passphrase { kdf, salt, nonce }, Unlock::Passphrase(passphrase)) => {
            kdf.validate()?;
            let key = kdf.derive(passphrase, &unb64(salt)?)?;
            aead_decrypt(&key, &unb64(nonce)?, payload, &aad)?
        }
        (CryptoHeader::Passphrase { .. }, _) => {
            return Err(Error::Crypto(
                "this container is passphrase-locked; supply a passphrase".into(),
            ))
        }
        (CryptoHeader::Keys { recipients }, Unlock::Keys(secrets)) => {
            if recipients.is_empty() || recipients.len() > MAX_RECIPIENTS {
                return Err(Error::Crypto("recipient slot count out of range".into()));
            }
            let mut slot: Option<(&RecipientSlot, &[u8; 32])> = None;
            for candidate in recipients {
                let slot_pub_bytes = unb64(&candidate.recipient_pub)?;
                for secret in secrets {
                    let our_pub = *PublicKey::from(&StaticSecret::from(*secret)).as_bytes();
                    if slot_pub_bytes.as_slice() == our_pub {
                        if slot.is_some() {
                            return Err(Error::Crypto("duplicate recipient slot".into()));
                        }
                        slot = Some((candidate, secret));
                    }
                }
            }
            let (slot, secret) = slot.ok_or(Error::Crypto(
                "this container is locked to other recipient keys".into(),
            ))?;
            let eph_pub: [u8; 32] = unb64(&slot.eph_pub)?
                .try_into()
                .map_err(|_| Error::Crypto("bad ephemeral pubkey".into()))?;
            let shared = StaticSecret::from(*secret).diffie_hellman(&PublicKey::from(eph_pub));
            let salt = unb64(&slot.salt)?;
            let hk = Hkdf::<Sha256>::new(Some(&salt), shared.as_bytes());
            let mut key = [0u8; 32];
            hk.expand(KDF_INFO_RECIPIENT, &mut key)
                .map_err(|_| Error::Crypto("HKDF expand failed".into()))?;
            aead_decrypt(&key, &unb64(&slot.nonce)?, &unb64(&slot.ct)?, &aad)?
        }
        (CryptoHeader::Keys { .. }, _) => {
            return Err(Error::Crypto(
                "this container is key-locked; supply a recipient key".into(),
            ))
        }
    };

    let secure: SecureData = serde_json::from_slice(&plaintext)
        .map_err(|e| Error::InvalidConfig(format!("container payload is invalid: {e}")))?;
    secure.profile.validate()?;
    Ok(Decoded {
        profile: secure.profile,
        policy: header.policy,
        verification,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Credential, Protocol, Transport};

    fn sample() -> Profile {
        Profile {
            id: "0123abcd0123abcd".into(),
            name: "shop bundle".into(),
            protocol: Protocol::Vless,
            server: "shop.example.com".into(),
            port: 443,
            credential: Credential::Uuid("11111111-2222-3333-4444-555555555555".into()),
            transport: Transport::Tls,
            sni: Some("shop.example.com".into()),
            path: None,
            allow_insecure: false,
            udp: true,
            group: None,
            created_at: 1_700_000_000,
        }
    }

    fn sign_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    #[test]
    fn passphrase_roundtrip_and_signature() {
        let profile = sample();
        let container = encode(
            &profile,
            &Policy::default(),
            &Lock::Passphrase("correct horse".into()),
            Some(&sign_key()),
        )
        .expect("encode");
        assert!(container.starts_with(MAGIC));
        let decoded = decode(&container, Unlock::Passphrase("correct horse")).expect("decode");
        assert_eq!(decoded.profile, profile);
        assert!(decoded.verification.signed);
        assert!(decoded.verification.valid);
    }

    #[test]
    fn wrong_passphrase_is_indistinguishable_from_tamper() {
        let container = encode(
            &sample(),
            &Policy::default(),
            &Lock::Passphrase("right".into()),
            Some(&sign_key()),
        )
        .expect("encode");
        let err = decode(&container, Unlock::Passphrase("wrong")).unwrap_err();
        assert!(matches!(err, Error::CryptoAuth));
    }

    #[test]
    fn tampered_payload_fails_closed() {
        let mut container = encode(
            &sample(),
            &Policy::default(),
            &Lock::Passphrase("pw".into()),
            Some(&sign_key()),
        )
        .expect("encode");
        let last = container.len() - 1;
        container[last] ^= 0x01;
        assert!(matches!(
            decode(&container, Unlock::Passphrase("pw")),
            Err(Error::CryptoAuth)
        ));
    }

    #[test]
    fn tampered_header_fails_closed() {
        let policy = Policy {
            expires_at: Some(1_800_000_000),
            metered_only: false,
            group_hint: None,
        };
        let mut container = encode(
            &sample(),
            &policy,
            &Lock::Passphrase("pw".into()),
            Some(&sign_key()),
        )
        .expect("encode");
        // Flip a byte near the end of the header (inside the signature
        // material) so the failure comes from verify_strict / signature
        // decoding, never from a successful decode.
        let hdr_len =
            u32::from_le_bytes([container[8], container[9], container[10], container[11]]) as usize;
        let idx = 6 + 2 + 4 + 4 + hdr_len - 10;
        container[idx] ^= 0x01;
        // Fail closed: a corrupted header is rejected outright (CryptoAuth
        // from verify_strict, or Crypto from the broken encoding - never a
        // successful decode).
        assert!(decode(&container, Unlock::Passphrase("pw")).is_err());
    }
    #[test]
    fn unsigned_container_reports_unsigned() {
        let container = encode(
            &sample(),
            &Policy::default(),
            &Lock::Passphrase("pw".into()),
            None,
        )
        .expect("encode");
        let decoded = decode(&container, Unlock::Passphrase("pw")).expect("decode");
        assert!(!decoded.verification.signed);
        assert!(!decoded.verification.valid);
    }

    #[test]
    fn key_lock_multi_recipient_roundtrip() {
        let (a_sec, a_pub) = generate_recipient_keypair();
        let (b_sec, b_pub) = generate_recipient_keypair();
        let (_c_sec, _c_pub) = generate_recipient_keypair();
        let container = encode(
            &sample(),
            &Policy::default(),
            &Lock::Keys(vec![a_pub, b_pub]),
            Some(&sign_key()),
        )
        .expect("encode");

        for sec in [&a_sec, &b_sec] {
            let decoded = decode(&container, Unlock::Keys(&[*sec])).expect("decode with own key");
            assert_eq!(decoded.profile, sample());
            assert!(decoded.verification.valid);
        }
        let (_d_sec, _d_pub) = generate_recipient_keypair();
        let err = decode(&container, Unlock::Keys(&[a_sec])).err();
        assert!(err.is_none());
    }

    #[test]
    fn open_mode_roundtrip() {
        let container = encode(
            &sample(),
            &Policy::default(),
            &Lock::Open,
            Some(&sign_key()),
        )
        .expect("encode");
        let decoded = decode(&container, Unlock::Any).expect("decode");
        assert_eq!(decoded.profile, sample());
        assert!(decoded.verification.valid);
        assert_eq!(peek_lock(&container).expect("peek"), "open");
    }

    #[test]
    fn policy_expiry() {
        let policy = Policy {
            expires_at: Some(1_000),
            metered_only: true,
            group_hint: Some("shop".into()),
        };
        assert!(policy.is_expired(1_001));
        assert!(!policy.is_expired(999));
        let container = encode(
            &sample(),
            &policy,
            &Lock::Passphrase("pw".into()),
            Some(&sign_key()),
        )
        .expect("encode");
        let decoded = decode(&container, Unlock::Passphrase("pw")).expect("decode");
        assert!(decoded.policy.metered_only);
        assert_eq!(decoded.policy.group_hint.as_deref(), Some("shop"));
    }

    #[test]
    fn rejects_truncated_and_foreign_input() {
        assert!(decode(b"NPVT2\0", Unlock::Any).is_err());
        assert!(decode(b"NOPE-NOPE-NOPE", Unlock::Any).is_err());
        let container = encode(
            &sample(),
            &Policy::default(),
            &Lock::Passphrase("pw".into()),
            None,
        )
        .expect("encode");
        let mut cut = container.clone();
        cut.pop();
        assert!(decode(&cut, Unlock::Passphrase("pw")).is_err());
    }

    #[test]
    fn open_mode_needs_no_unlock_but_passphrase_mode_rejects_any() {
        let open = encode(&sample(), &Policy::default(), &Lock::Open, None).expect("encode");
        assert!(decode(&open, Unlock::Any).is_ok());
        let locked = encode(
            &sample(),
            &Policy::default(),
            &Lock::Passphrase("pw".into()),
            None,
        )
        .expect("encode");
        assert!(decode(&locked, Unlock::Any).is_err());
        assert_eq!(peek_lock(&locked).expect("peek"), "passphrase");
    }
}
