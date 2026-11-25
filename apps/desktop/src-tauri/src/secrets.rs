//! Secrets (today: the Steam Web API key) live in the operating system's keychain: macOS Keychain,
//! Windows Credential Manager, or the Secret Service (GNOME Keyring, KWallet) on Linux. Callers fall
//! back to the private settings database (folder 700, file 600) when no keychain is available.

pub const STEAM_API_KEY: &str = "steam-api-key";

pub use store::{delete, get, set};

#[cfg(not(test))]
mod store {
    const SERVICE: &str = "co.repakhub.desktop";

    fn entry(name: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, name).map_err(|e| e.to_string())
    }

    pub fn get(name: &str) -> Result<Option<String>, String> {
        match entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set(name: &str, value: &str) -> Result<(), String> {
        entry(name)?.set_password(value).map_err(|e| e.to_string())
    }

    pub fn delete(name: &str) -> Result<(), String> {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Tests never touch the real keychain: an in-memory stand-in that can also play "no keychain".
#[cfg(test)]
pub mod store {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    static VALUES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
    pub static AVAILABLE: AtomicBool = AtomicBool::new(true);

    fn check() -> Result<(), String> {
        if AVAILABLE.load(Ordering::SeqCst) { Ok(()) } else { Err("no keychain".into()) }
    }

    pub fn get(name: &str) -> Result<Option<String>, String> {
        check()?;
        Ok(VALUES.lock().unwrap().get_or_insert_with(HashMap::new).get(name).cloned())
    }

    pub fn set(name: &str, value: &str) -> Result<(), String> {
        check()?;
        VALUES.lock().unwrap().get_or_insert_with(HashMap::new).insert(name.into(), value.into());
        Ok(())
    }

    pub fn delete(name: &str) -> Result<(), String> {
        check()?;
        VALUES.lock().unwrap().get_or_insert_with(HashMap::new).remove(name);
        Ok(())
    }
}

