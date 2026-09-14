// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Device-local, non-secret account catalogue.
//!
//! `SpacetimeDB` tokens remain in the SDK credential store. This file contains
//! only stable lookup IDs and presentation metadata so several Poche processes
//! can select different authenticated identities from one installation.

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

const VAULT_SCHEMA_VERSION: u16 = 1;
const LOCK_RETRY: Duration = Duration::from_millis(10);
const LOCK_ATTEMPTS: usize = 200;
const STALE_LOCK_AGE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IdentityAccount {
    pub account_id: String,
    pub label: String,
    pub display_name: String,
    pub authority_uri: String,
    pub database: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
}

impl IdentityAccount {
    #[must_use]
    pub fn belongs_to(&self, authority_uri: &str, database: &str) -> bool {
        self.authority_uri.trim_end_matches('/') == authority_uri.trim_end_matches('/')
            && self.database == database
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct VaultFile {
    schema_version: u16,
    accounts: Vec<IdentityAccount>,
}

impl Default for VaultFile {
    fn default() -> Self {
        Self {
            schema_version: VAULT_SCHEMA_VERSION,
            accounts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Resource)]
pub struct IdentityVault {
    path: PathBuf,
    accounts: Vec<IdentityAccount>,
}

impl IdentityVault {
    /// Loads the account catalogue at `path`, or an empty catalogue when it does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error when the existing file cannot be read, decoded, or uses an unsupported
    /// schema version.
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let accounts = read_vault(&path)?.accounts;
        Ok(Self { path, accounts })
    }

    /// Resolves the process-independent default catalogue path.
    ///
    /// # Errors
    ///
    /// Returns an error when neither an explicit override nor a platform application-data
    /// directory is available.
    pub fn default_path() -> Result<PathBuf, String> {
        if let Some(explicit) = std::env::var_os("POCHE_IDENTITY_VAULT") {
            return Ok(PathBuf::from(explicit));
        }
        let root = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .map(PathBuf::from)
            .ok_or("LOCALAPPDATA or APPDATA is required unless POCHE_IDENTITY_VAULT is set")?;
        Ok(root.join("Poche").join("identity-vault-v1.json"))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn accounts_for(&self, authority_uri: &str, database: &str) -> Vec<&IdentityAccount> {
        let mut accounts = self
            .accounts
            .iter()
            .filter(|account| account.belongs_to(authority_uri, database))
            .collect::<Vec<_>>();
        accounts.sort_by(|left, right| {
            left.label
                .to_lowercase()
                .cmp(&right.label.to_lowercase())
                .then_with(|| left.account_id.cmp(&right.account_id))
        });
        accounts
    }

    #[must_use]
    pub fn account(&self, account_id: &str) -> Option<&IdentityAccount> {
        self.accounts
            .iter()
            .find(|account| account.account_id == account_id)
    }

    /// Reloads the catalogue from disk so another running Poche process becomes visible.
    ///
    /// # Errors
    ///
    /// Returns an error when the catalogue cannot be read or decoded.
    pub fn reload(&mut self) -> Result<(), String> {
        self.accounts = read_vault(&self.path)?.accounts;
        Ok(())
    }

    /// Creates and durably records an identity for one authority and database.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or duplicate label, unavailable randomness, or a failed
    /// read, lock, or atomic write.
    pub fn create(
        &mut self,
        label: &str,
        authority_uri: &str,
        database: &str,
    ) -> Result<IdentityAccount, String> {
        validate_label(label)?;
        self.reload()?;
        if self
            .accounts_for(authority_uri, database)
            .iter()
            .any(|account| account.label.trim().eq_ignore_ascii_case(label.trim()))
        {
            return Err("an identity with that label already exists for this authority".into());
        }
        let account = IdentityAccount {
            account_id: random_account_id()?,
            label: label.trim().to_owned(),
            display_name: label.trim().to_owned(),
            authority_uri: authority_uri.trim_end_matches('/').to_owned(),
            database: database.to_owned(),
            principal: None,
        };
        self.upsert(account.clone())?;
        Ok(account)
    }

    /// Binds the public authority principal observed for an account after authentication.
    ///
    /// # Errors
    ///
    /// Returns an error when the account is absent, when it was previously bound to another
    /// principal, or when the updated catalogue cannot be persisted.
    pub fn record_principal(&mut self, account_id: &str, principal: &str) -> Result<(), String> {
        let Some(mut account) = self.account(account_id).cloned() else {
            return Err("selected identity is absent from the local vault".into());
        };
        if let Some(existing) = &account.principal
            && existing != principal
        {
            return Err("the protected credential resolved to a different identity".into());
        }
        account.principal = Some(principal.to_owned());
        self.upsert(account)
    }

    fn upsert(&mut self, account: IdentityAccount) -> Result<(), String> {
        let _lock = VaultLock::acquire(&self.path)?;
        let mut merged = read_vault(&self.path)?
            .accounts
            .into_iter()
            .map(|account| (account.account_id.clone(), account))
            .collect::<BTreeMap<_, _>>();
        for cached in self.accounts.drain(..) {
            merged.entry(cached.account_id.clone()).or_insert(cached);
        }
        merged.insert(account.account_id.clone(), account);
        self.accounts = merged.into_values().collect();
        write_vault(&self.path, &self.accounts)
    }
}

fn validate_label(label: &str) -> Result<(), String> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 32 || label.chars().any(char::is_control) {
        return Err("identity label must contain 1–32 visible characters".into());
    }
    Ok(())
}

fn random_account_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("randomness unavailable: {error}"))?;
    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Ok(format!("account-{encoded}"))
}

fn read_vault(path: &Path) -> Result<VaultFile, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(VaultFile::default());
        }
        Err(error) => return Err(format!("could not read identity vault: {error}")),
    };
    let vault: VaultFile = serde_json::from_slice(&bytes)
        .map_err(|error| format!("identity vault is invalid: {error}"))?;
    if vault.schema_version != VAULT_SCHEMA_VERSION {
        return Err(format!(
            "identity vault schema {} is unsupported",
            vault.schema_version
        ));
    }
    Ok(vault)
}

fn write_vault(path: &Path, accounts: &[IdentityAccount]) -> Result<(), String> {
    let parent = path.parent().ok_or("identity vault path has no parent")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create identity vault directory: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("could not create identity vault temporary file: {error}"))?;
    serde_json::to_writer_pretty(
        temporary.as_file_mut(),
        &VaultFile {
            schema_version: VAULT_SCHEMA_VERSION,
            accounts: accounts.to_vec(),
        },
    )
    .map_err(|error| format!("could not encode identity vault: {error}"))?;
    temporary
        .as_file_mut()
        .write_all(b"\n")
        .map_err(|error| format!("could not finish identity vault: {error}"))?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| format!("could not flush identity vault: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("could not replace identity vault: {}", error.error))?;
    Ok(())
}

struct VaultLock(PathBuf);

impl VaultLock {
    fn acquire(vault_path: &Path) -> Result<Self, String> {
        let parent = vault_path
            .parent()
            .ok_or("identity vault path has no parent")?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create identity vault directory: {error}"))?;
        let lock_path = vault_path.with_extension("lock");
        for _ in 0..LOCK_ATTEMPTS {
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    return Ok(Self(lock_path));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if lock_is_stale(&lock_path) {
                        let _ = fs::remove_file(&lock_path);
                    } else {
                        thread::sleep(LOCK_RETRY);
                    }
                }
                Err(error) => {
                    return Err(format!("could not lock identity vault: {error}"));
                }
            }
        }
        Err("timed out waiting for another Poche window to update identities".into())
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn lock_is_stale(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_LOCK_AGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_keeps_tokens_out_and_merges_concurrent_catalogues() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("identities.json");
        let mut first = IdentityVault::load(path.clone()).expect("first vault");
        let mut second = IdentityVault::load(path.clone()).expect("second vault");
        let alice = first
            .create("Alice", "https://maincloud.spacetimedb.com", "poche")
            .expect("Alice account");
        let bob = second
            .create("Bob", "https://maincloud.spacetimedb.com", "poche")
            .expect("Bob account");

        let vault = IdentityVault::load(path.clone()).expect("merged vault");
        let accounts = vault.accounts_for("https://maincloud.spacetimedb.com/", "poche");
        assert_eq!(accounts.len(), 2);
        assert_ne!(alice.account_id, bob.account_id);
        let json = fs::read_to_string(path).expect("vault JSON");
        assert!(!json.contains("token"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn principal_is_bound_once_to_an_immutable_account() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("identities.json");
        let mut vault = IdentityVault::load(path).expect("vault");
        let account = vault
            .create("Alice", "http://127.0.0.1:3000", "poche")
            .expect("account");
        vault
            .record_principal(&account.account_id, "01ab")
            .expect("first binding");
        vault
            .record_principal(&account.account_id, "01ab")
            .expect("same binding");
        assert!(
            vault
                .record_principal(&account.account_id, "different")
                .unwrap_err()
                .contains("different identity")
        );
    }
}
