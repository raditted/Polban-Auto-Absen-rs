//! Secure credential storage abstraction backed by OS keyring with memory fallback.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use thiserror::Error;

const KEYRING_SERVICE: &str = "polban-absen-gui";

/// Active backend providing credential persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialBackend {
    OsKeyring,
    MemoryFallback,
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("keyring sistem tidak tersedia: {0}")]
    BackendUnavailable(String),
    #[error("operasi kredensial gagal: {0}")]
    OperationFailed(String),
}

/// Thread-safe credential store interface.
pub trait CredentialStore: Send + Sync + fmt::Debug {
    fn backend(&self) -> CredentialBackend;
    fn get_password(&self, username: &str) -> Result<Option<String>, CredentialError>;
    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialError>;
    fn delete_password(&self, username: &str) -> Result<(), CredentialError>;
}

/// OS native credential store (`keyring` crate: Windows Credential Manager, macOS Keychain, Secret Service).
pub struct OsKeyringStore {
    service: &'static str,
}

impl OsKeyringStore {
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE,
        }
    }
}

impl Default for OsKeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for OsKeyringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OsKeyringStore")
            .field("service", &self.service)
            .finish()
    }
}

impl CredentialStore for OsKeyringStore {
    fn backend(&self) -> CredentialBackend {
        CredentialBackend::OsKeyring
    }

    fn get_password(&self, username: &str) -> Result<Option<String>, CredentialError> {
        if username.trim().is_empty() {
            return Ok(None);
        }
        let entry = keyring::Entry::new(self.service, username)
            .map_err(|e| CredentialError::BackendUnavailable(e.to_string()))?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::OperationFailed(e.to_string())),
        }
    }

    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialError> {
        let entry = keyring::Entry::new(self.service, username)
            .map_err(|e| CredentialError::BackendUnavailable(e.to_string()))?;
        entry
            .set_password(password)
            .map_err(|e| CredentialError::OperationFailed(e.to_string()))
    }

    fn delete_password(&self, username: &str) -> Result<(), CredentialError> {
        if username.trim().is_empty() {
            return Ok(());
        }
        let entry = keyring::Entry::new(self.service, username)
            .map_err(|e| CredentialError::BackendUnavailable(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::OperationFailed(e.to_string())),
        }
    }
}

/// In-memory fallback store used when OS keyring is unavailable or in offline tests.
/// Custom `Debug` implementation redacts all stored secrets.
#[derive(Default)]
pub struct MemoryFallbackStore {
    entries: Mutex<HashMap<String, String>>,
}

impl MemoryFallbackStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Debug for MemoryFallbackStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<String> = self
            .entries
            .lock()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default();
        f.debug_struct("MemoryFallbackStore")
            .field("accounts", &keys)
            .field("secrets", &"[REDACTED]")
            .finish()
    }
}

impl CredentialStore for MemoryFallbackStore {
    fn backend(&self) -> CredentialBackend {
        CredentialBackend::MemoryFallback
    }

    fn get_password(&self, username: &str) -> Result<Option<String>, CredentialError> {
        let guard = self
            .entries
            .lock()
            .map_err(|e| CredentialError::OperationFailed(e.to_string()))?;
        Ok(guard.get(username).cloned())
    }

    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialError> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|e| CredentialError::OperationFailed(e.to_string()))?;
        guard.insert(username.to_string(), password.to_string());
        Ok(())
    }

    fn delete_password(&self, username: &str) -> Result<(), CredentialError> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|e| CredentialError::OperationFailed(e.to_string()))?;
        guard.remove(username);
        Ok(())
    }
}

/// Open the OS keyring store if entry creation succeeds, otherwise fall back to memory.
pub fn open_default_store() -> Box<dyn CredentialStore> {
    if keyring::Entry::new(KEYRING_SERVICE, "__probe__").is_ok() {
        Box::new(OsKeyringStore::new())
    } else {
        Box::new(MemoryFallbackStore::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_fallback_redacts_secrets_in_debug() {
        let store = MemoryFallbackStore::new();
        store.set_password("251511001", "ultra-secret#99").unwrap();
        let dbg = format!("{store:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("ultra-secret#99"));
        assert_eq!(
            store.get_password("251511001").unwrap().as_deref(),
            Some("ultra-secret#99")
        );
        store.delete_password("251511001").unwrap();
        assert_eq!(store.get_password("251511001").unwrap(), None);
    }
}
