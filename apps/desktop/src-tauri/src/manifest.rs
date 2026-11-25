//! Signed update manifests (M3). See docs/SOURCE_POLICY.md, "Technical enforcement".
//!
//! A manifest arrives as an envelope `{ keyId, payload, signature }`: `payload` is the base64 of the
//! manifest JSON and `signature` an Ed25519 signature over exactly those bytes. Only keys compiled in
//! from `keys/trusted.json` are accepted. After the signature check the manifest is validated:
//! HTTPS-only URLs on hosts the manifest itself lists, plain relative file paths, and no executables
//! unless the key belongs to the rights holder. Nothing is downloaded or applied here.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrustedKey {
    pub id: String,
    pub source: String,
    /// Base64 of the 32-byte Ed25519 public key.
    pub public_key: String,
    /// The publisher's own key: only these manifests may add or replace executables.
    #[serde(default)]
    pub rights_holder: bool,
}

/// Keys compiled into the app. Adding one requires a source-register row and owner review.
pub fn trusted_keys() -> Vec<TrustedKey> {
    serde_json::from_str(include_str!("../keys/trusted.json")).expect("keys/trusted.json is valid")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    key_id: String,
    payload: String,
    signature: String,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub source: String,
    pub game_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub changelog: String,
    /// The only hosts files may be fetched from.
    pub hosts: Vec<String>,
    /// The complete file set of this version.
    pub files: Vec<FileEntry>,
    /// Set from the verifying key, never from the payload: whether executables are allowed.
    #[serde(skip)]
    pub rights_holder: bool,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEntry {
    /// Relative to the game folder, `/`-separated.
    pub path: String,
    pub size: u64,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    pub url: String,
}

impl Manifest {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ManifestError {
    #[error("manifest is not a valid signed envelope")]
    Malformed,
    #[error("manifest is signed with an unknown key ({0})")]
    UnknownKey(String),
    #[error("manifest signature is invalid")]
    BadSignature,
    #[error("unsupported manifest schema {0}")]
    Schema(u32),
    #[error("manifest source {got:?} does not match its key's source {expected:?}")]
    WrongSource { expected: String, got: String },
    #[error("invalid manifest: {0}")]
    Invalid(String),
    #[error("{0} is an executable, which only the rights holder's key may ship")]
    Executable(String),
}

/// Verify an envelope against the compiled-in keys, then validate the manifest.
pub fn verify(envelope_json: &str) -> Result<Manifest, ManifestError> {
    verify_with(&trusted_keys(), envelope_json)
}

pub fn verify_with(keys: &[TrustedKey], envelope_json: &str) -> Result<Manifest, ManifestError> {
    let env: Envelope = serde_json::from_str(envelope_json).map_err(|_| ManifestError::Malformed)?;
    let key = keys.iter().find(|k| k.id == env.key_id).ok_or_else(|| ManifestError::UnknownKey(env.key_id.clone()))?;

    let payload = B64.decode(&env.payload).map_err(|_| ManifestError::Malformed)?;
    let sig: [u8; 64] = B64.decode(&env.signature).ok().and_then(|s| s.try_into().ok()).ok_or(ManifestError::Malformed)?;
    let pk: [u8; 32] = B64.decode(&key.public_key).ok().and_then(|k| k.try_into().ok()).ok_or(ManifestError::Malformed)?;
    let vk = VerifyingKey::from_bytes(&pk).map_err(|_| ManifestError::Malformed)?;
    vk.verify_strict(&payload, &Signature::from_bytes(&sig)).map_err(|_| ManifestError::BadSignature)?;

    // Only now is the payload trusted enough to parse.
    let mut m: Manifest = serde_json::from_slice(&payload).map_err(|e| ManifestError::Invalid(e.to_string()))?;
    validate(&m, key)?;
    m.rights_holder = key.rights_holder;
    Ok(m)
}

fn validate(m: &Manifest, key: &TrustedKey) -> Result<(), ManifestError> {
    let invalid = |msg: String| Err(ManifestError::Invalid(msg));
    if m.schema != 1 {
        return Err(ManifestError::Schema(m.schema));
    }
    if m.source != key.source {
        return Err(ManifestError::WrongSource { expected: key.source.clone(), got: m.source.clone() });
    }
    if m.game_id.trim().is_empty() || m.version.trim().is_empty() {
        return invalid("gameId and version are required".into());
    }
    let valid_host = |h: &String| !h.is_empty() && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    if m.hosts.is_empty() || !m.hosts.iter().all(valid_host) {
        return invalid("hosts must be a non-empty list of lowercase host names".into());
    }

    let mut seen = HashSet::new();
    for f in &m.files {
        if f.path.contains('\\') || !crate::discovery::is_plain_relative(&f.path) || !is_portable(&f.path) {
            return invalid(format!("unsafe file path {:?}", f.path));
        }
        // Windows and macOS file systems are case-insensitive: two entries must not collide there.
        if !seen.insert(f.path.to_lowercase()) {
            return invalid(format!("duplicate file path {:?}", f.path));
        }
        if f.sha256.len() != 64 || !f.sha256.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)) {
            return invalid(format!("{}: sha256 must be 64 lowercase hex characters", f.path));
        }
        let url = url::Url::parse(&f.url).map_err(|_| ManifestError::Invalid(format!("{}: bad url", f.path)))?;
        let host_ok = url.host_str().is_some_and(|h| m.hosts.iter().any(|allowed| allowed == h));
        if url.scheme() != "https" || !host_ok || url.port().is_some() || !url.username().is_empty() || url.password().is_some() {
            return invalid(format!("{}: url must be https on a listed host, without credentials or port", f.path));
        }
        if !key.rights_holder && is_executable_name(&f.path) {
            return Err(ManifestError::Executable(f.path.clone()));
        }
    }
    Ok(())
}

/// Safe as a file name on every OS a manifest may be applied on. Rejects Windows-invalid characters
/// (a `:` would write a hidden NTFS stream), trailing dots or spaces, and reserved device names.
fn is_portable(path: &str) -> bool {
    const RESERVED: &[&str] = &["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
        "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];
    path.split('/').all(|seg| {
        let stem = seg.split('.').next().unwrap_or(seg).to_ascii_lowercase();
        !seg.is_empty()
            && !seg.chars().any(|c| c.is_control() || "<>:\"|?*".contains(c))
            && !seg.ends_with(['.', ' '])
            && !RESERVED.contains(&stem.as_str())
    })
}

/// Executable by file name. Linux binaries often have no extension, so downloaded content is also
/// checked with `is_executable_content` before anything is applied.
pub fn is_executable_name(path: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        "exe", "dll", "sys", "com", "scr", "msi", "bat", "cmd", "ps1", "vbs", "js", "jar", "so", "dylib", "sh",
        "command", "appimage", "app", "x86_64", "x86",
    ];
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name.rsplit_once('.').is_some_and(|(_, ext)| EXTENSIONS.contains(&ext))
}

/// Executable by content: Windows (`MZ`), ELF, Mach-O (incl. universal) or a `#!` script.
pub fn is_executable_content(head: &[u8]) -> bool {
    const MAGIC: &[&[u8]] = &[
        b"MZ",
        b"\x7fELF",
        b"#!",
        &[0xfe, 0xed, 0xfa, 0xce],
        &[0xfe, 0xed, 0xfa, 0xcf],
        &[0xce, 0xfa, 0xed, 0xfe],
        &[0xcf, 0xfa, 0xed, 0xfe],
        &[0xca, 0xfe, 0xba, 0xbe],
    ];
    MAGIC.iter().any(|m| head.starts_with(m))
}

/// Whether `manifest_version` is newer than what is installed. `None` when either is unknown or the
/// two are not comparable: never guess.
pub fn is_newer(installed: Option<&str>, manifest_version: &str) -> Option<bool> {
    crate::version::compare(manifest_version, installed?).map(|o| o == Ordering::Greater)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    pub fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    pub fn trusted(rights_holder: bool) -> Vec<TrustedKey> {
        vec![TrustedKey {
            id: "test-key".into(),
            source: "test-source".into(),
            public_key: B64.encode(signing_key().verifying_key().to_bytes()),
            rights_holder,
        }]
    }

    pub fn envelope(manifest: &serde_json::Value) -> String {
        let payload = serde_json::to_vec(manifest).unwrap();
        let sig = signing_key().sign(&payload);
        serde_json::json!({ "keyId": "test-key", "payload": B64.encode(&payload), "signature": B64.encode(sig.to_bytes()) })
            .to_string()
    }

    pub fn manifest(files: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "schema": 1, "source": "test-source", "gameId": "g1", "name": "Game", "version": "1.1.0",
            "changelog": "Fixes.", "hosts": ["downloads.example.org"], "files": files
        })
    }

    pub fn file(path: &str) -> serde_json::Value {
        serde_json::json!({ "path": path, "size": 3, "sha256": "a".repeat(64), "url": format!("https://downloads.example.org/{path}") })
    }

    fn check(rights_holder: bool, m: serde_json::Value) -> Result<Manifest, ManifestError> {
        verify_with(&trusted(rights_holder), &envelope(&m))
    }

    #[test]
    fn accepts_a_valid_signed_manifest() {
        let m = check(false, manifest(serde_json::json!([file("data/a.pak"), file("data/b.pak")]))).unwrap();
        assert_eq!((m.version.as_str(), m.files.len(), m.total_size()), ("1.1.0", 2, 6));
    }

    #[test]
    fn rejects_tampering_and_unknown_keys() {
        let env = envelope(&manifest(serde_json::json!([file("a.pak")])));
        let mut tampered: serde_json::Value = serde_json::from_str(&env).unwrap();
        let other = serde_json::to_vec(&manifest(serde_json::json!([file("b.pak")]))).unwrap();
        tampered["payload"] = B64.encode(other).into();
        assert_eq!(verify_with(&trusted(false), &tampered.to_string()), Err(ManifestError::BadSignature));

        tampered["keyId"] = "someone-else".into();
        assert!(matches!(verify_with(&trusted(false), &tampered.to_string()), Err(ManifestError::UnknownKey(_))));
        assert_eq!(verify_with(&trusted(false), "not json"), Err(ManifestError::Malformed));
        assert!(matches!(verify(&env), Err(ManifestError::UnknownKey(_))), "no key is trusted by default");
    }

    #[test]
    fn enforces_source_policy_rules() {
        let with = |f: serde_json::Value| check(false, manifest(serde_json::json!([f])));
        let mut f = file("a.pak");
        f["url"] = "http://downloads.example.org/a.pak".into();
        assert!(matches!(with(f), Err(ManifestError::Invalid(_))), "http refused");
        let mut f = file("a.pak");
        f["url"] = "https://evil.example.com/a.pak".into();
        assert!(matches!(with(f), Err(ManifestError::Invalid(_))), "unlisted host refused");
        let mut f = file("a.pak");
        f["url"] = "https://user:pw@downloads.example.org/a.pak".into();
        assert!(matches!(with(f), Err(ManifestError::Invalid(_))), "credentials refused");
        for bad in ["../escape.pak", "/abs.pak", "a\\b.pak", "", "data/a.pak:hidden", "nul.pak", "dir./a.pak", "a?.pak"] {
            assert!(matches!(with(file(bad)), Err(ManifestError::Invalid(_))), "{bad:?} refused");
        }
        let mut f = file("a.pak");
        f["sha256"] = "A".repeat(64).into();
        assert!(matches!(with(f), Err(ManifestError::Invalid(_))), "uppercase hash refused");
        assert!(matches!(
            check(false, manifest(serde_json::json!([file("A.pak"), file("a.pak")]))),
            Err(ManifestError::Invalid(_))
        ));
        let mut m = manifest(serde_json::json!([file("a.pak")]));
        m["source"] = "other-source".into();
        assert!(matches!(check(false, m), Err(ManifestError::WrongSource { .. })));
        let mut m = manifest(serde_json::json!([file("a.pak")]));
        m["postInstall"] = "run.sh".into();
        assert!(matches!(check(false, m), Err(ManifestError::Invalid(_))), "unknown fields refused");
    }

    #[test]
    fn executables_need_the_rights_holder_key() {
        let m = manifest(serde_json::json!([file("bin/Game.EXE")]));
        assert_eq!(check(false, m.clone()), Err(ManifestError::Executable("bin/Game.EXE".into())));
        assert!(check(true, m).is_ok());
        assert!(is_executable_content(b"\x7fELF\x02\x01"));
        assert!(is_executable_content(b"#!/bin/sh"));
        assert!(!is_executable_content(b"PK\x03\x04"));
        assert!(!is_executable_name("data/readme.txt"));
    }

    #[test]
    fn newer_only_when_comparable() {
        assert_eq!(is_newer(Some("1.0"), "1.1.0"), Some(true));
        assert_eq!(is_newer(Some("1.1"), "1.1.0"), Some(false));
        assert_eq!(is_newer(Some("build-77"), "1.1.0"), None);
        assert_eq!(is_newer(None, "1.1.0"), None);
    }
}
