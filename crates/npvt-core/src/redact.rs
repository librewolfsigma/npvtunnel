//! Secret redaction helpers. Every log line and diagnostic that could
//! contain profile material goes through these before hitting disk or the
//! terminal (defect D12 in the blueprint: logs must never leak secrets).

use zeroize::Zeroize;

/// A set of secret strings to keep out of logs. Zeroized on drop.
#[derive(Default)]
pub struct SecretSet {
    entries: Vec<String>,
}

impl SecretSet {
    /// Empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a secret; empty strings are ignored.
    pub fn push(&mut self, secret: impl Into<String>) {
        let s = secret.into();
        if !s.is_empty() {
            self.entries.push(s);
        }
    }

    /// Number of tracked secrets.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Replace every occurrence of every tracked secret with `[redacted]`.
    pub fn redact(&self, text: &str) -> String {
        let mut out = text.to_string();
        for s in &self.entries {
            if text.contains(s.as_str()) {
                out = out.replace(s.as_str(), "[redacted]");
            }
        }
        out
    }

    /// Collect secrets from a profile into this set.
    pub fn extend_from_credential(&mut self, cred: &crate::profile::Credential) {
        use crate::profile::Credential as C;
        match cred {
            C::Uuid(v) | C::Password(v) | C::Token(v) => self.push(v.clone()),
            C::SsMethod {
                method: _,
                password,
            } => self.push(password.clone()),
            C::None => {}
        }
    }
}

impl Drop for SecretSet {
    fn drop(&mut self) {
        for e in &mut self.entries {
            e.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Credential;

    #[test]
    fn replaces_all_occurrences() {
        let mut s = SecretSet::new();
        s.push("hunter2");
        assert_eq!(
            s.redact("pass=hunter2 pass=hunter2 pass=plain"),
            "pass=[redacted] pass=[redacted] pass=plain"
        );
    }

    #[test]
    fn collects_from_credentials() {
        let mut s = SecretSet::new();
        s.extend_from_credential(&Credential::SsMethod {
            method: "2022-blake3-aes-256-gcm".into(),
            password: "sesame".into(),
        });
        assert_eq!(s.len(), 1);
        assert_eq!(s.redact("open sesame"), "open [redacted]");
    }

    #[test]
    fn ignores_empty_secrets() {
        let mut s = SecretSet::new();
        s.push("");
        assert!(s.is_empty());
    }
}
