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

    fn probe_available(&self) -> bool {
        let Ok(entry) = keyring::Entry::new(self.service, "__probe__") else {
            return false;
        };
        matches!(entry.get_password(), Ok(_) | Err(keyring::Error::NoEntry))
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
            .field("secrets", &"[REDACTED]")
            .finish()
    }
}

impl CredentialStore for OsKeyringStore {
    fn backend(&self) -> CredentialBackend {
        CredentialBackend::OsKeyring
    }

    fn get_password(&self, username: &str) -> Result<Option<String>, CredentialError> {
        let user = username.trim();
        if user.is_empty() {
            return Ok(None);
        }
        let entry = keyring::Entry::new(self.service, user)
            .map_err(|e| CredentialError::BackendUnavailable(e.to_string()))?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(keyring::Error::NoStorageAccess(e)) | Err(keyring::Error::PlatformFailure(e)) => {
                Err(CredentialError::BackendUnavailable(e.to_string()))
            }
            Err(e) => Err(CredentialError::OperationFailed(e.to_string())),
        }
    }

    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialError> {
        let user = username.trim();
        if user.is_empty() {
            return Err(CredentialError::OperationFailed(
                "NIM tidak boleh kosong".to_string(),
            ));
        }
        let entry = keyring::Entry::new(self.service, user)
            .map_err(|e| CredentialError::BackendUnavailable(e.to_string()))?;
        entry.set_password(password).map_err(|e| match e {
            keyring::Error::NoStorageAccess(inner) | keyring::Error::PlatformFailure(inner) => {
                CredentialError::BackendUnavailable(inner.to_string())
            }
            other => CredentialError::OperationFailed(other.to_string()),
        })
    }

    fn delete_password(&self, username: &str) -> Result<(), CredentialError> {
        let user = username.trim();
        if user.is_empty() {
            return Ok(());
        }
        let entry = keyring::Entry::new(self.service, user)
            .map_err(|e| CredentialError::BackendUnavailable(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(keyring::Error::NoStorageAccess(e)) | Err(keyring::Error::PlatformFailure(e)) => {
                Err(CredentialError::BackendUnavailable(e.to_string()))
            }
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
        let user = username.trim();
        if user.is_empty() {
            return Ok(None);
        }
        let guard = self
            .entries
            .lock()
            .map_err(|e| CredentialError::OperationFailed(e.to_string()))?;
        Ok(guard.get(user).cloned())
    }

    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialError> {
        let user = username.trim();
        if user.is_empty() {
            return Err(CredentialError::OperationFailed(
                "NIM tidak boleh kosong".to_string(),
            ));
        }
        let mut guard = self
            .entries
            .lock()
            .map_err(|e| CredentialError::OperationFailed(e.to_string()))?;
        guard.insert(user.to_string(), password.to_string());
        Ok(())
    }

    fn delete_password(&self, username: &str) -> Result<(), CredentialError> {
        let user = username.trim();
        if user.is_empty() {
            return Ok(());
        }
        let mut guard = self
            .entries
            .lock()
            .map_err(|e| CredentialError::OperationFailed(e.to_string()))?;
        guard.remove(user);
        Ok(())
    }
}

/// Open the OS keyring store if the platform backend responds cleanly to a read probe,
/// otherwise fall back to `MemoryFallbackStore` without panicking.
pub fn open_default_store() -> Box<dyn CredentialStore> {
    let store = OsKeyringStore::new();
    if store.probe_available() {
        Box::new(store)
    } else {
        Box::new(MemoryFallbackStore::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_fallback_crud_and_debug_redaction() {
        let store = MemoryFallbackStore::new();
        assert_eq!(store.backend(), CredentialBackend::MemoryFallback);
        assert_eq!(store.get_password("").unwrap(), None);
        assert!(store.set_password("   ", "pw").is_err());

        store.set_password("251511001", "ultra-secret#99").unwrap();
        let dbg = format!("{store:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("ultra-secret#99"));
        assert!(dbg.contains("251511001"));
        assert_eq!(
            store.get_password(" 251511001 ").unwrap().as_deref(),
            Some("ultra-secret#99")
        );

        store.set_password("251511001", "new-secret!").unwrap();
        assert_eq!(
            store.get_password("251511001").unwrap().as_deref(),
            Some("new-secret!")
        );

        store.delete_password("251511001").unwrap();
        assert_eq!(store.get_password("251511001").unwrap(), None);
        store.delete_password("nonexistent").unwrap();
    }

    #[test]
    fn os_keyring_store_debug_and_empty_username_guards() {
        let store = OsKeyringStore::new();
        assert_eq!(store.backend(), CredentialBackend::OsKeyring);
        let dbg = format!("{store:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(dbg.contains("polban-absen-gui"));

        assert_eq!(store.get_password("").unwrap(), None);
        assert!(store.set_password("", "pw").is_err());
        store.delete_password("").unwrap();
    }

    #[test]
    fn open_default_store_never_panics_and_returns_valid_backend() {
        let store = open_default_store();
        assert!(matches!(
            store.backend(),
            CredentialBackend::OsKeyring | CredentialBackend::MemoryFallback
        ));
        assert_eq!(store.get_password("").unwrap(), None);
    }
}
