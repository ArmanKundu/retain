//! Provider API keys.
//!
//! ## The rule this module exists to enforce
//!
//! The macOS Keychain is the **only** place a key is ever persisted. Not SQLite,
//! not a config file, not a log line, not a crash report, and nothing in the
//! frontend — no Zustand store, no `localStorage`, no `sessionStorage`.
//!
//! The design that makes that enforceable rather than aspirational:
//!
//!   * There is no command that returns a key to the frontend. Look for one — it
//!     isn't here. The UI can ask *whether* a key exists (`has_key`) and never
//!     what it is.
//!   * Reading a key is `pub(crate)`, so only Rust in this app can call it, and
//!     only at the moment of making an API request (Checkpoint 3).
//!   * `SecretString` wraps the value with a `Debug` implementation that prints
//!     `SecretString(***)`. That matters because `{:?}` is how values end up in
//!     logs and panic messages by accident — this makes the accident harmless.
//!
//! ## The one wrinkle, since the app ships unsigned
//!
//! Keychain ties access permission to the calling app's code signature. Retain is
//! ad-hoc signed, so its signature changes with every build, and macOS will ask
//! "allow access to your keychain?" once after each app update. That is once per
//! release, not once per launch, and it is the honest cost of using real Keychain
//! rather than a file we encrypt ourselves.

use std::fmt;

use keyring::Entry;
use serde::{Deserialize, Serialize};

/// Keychain service name. Every entry is stored under this, keyed by provider.
const SERVICE: &str = "com.armankundu.retain";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Anthropic,
    OpenAi,
    Gemini,
    OpenRouter,
}

impl Provider {
    /// The Keychain account name for this provider.
    fn account(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::Gemini => "gemini",
            Provider::OpenRouter => "openrouter",
        }
    }

    /// Display name, used in provider-facing messages.
    pub fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::OpenAi => "OpenAI",
            Provider::Gemini => "Gemini",
            Provider::OpenRouter => "OpenRouter",
        }
    }
}

/// A key, wrapped so it cannot be logged by accident.
///
/// The wrapper is what makes "never plaintext in a log" structural rather than a
/// convention someone has to remember: `{:?}` and `{}` both print `***`.
pub struct SecretString(String);

impl SecretString {
    /// Hand out the raw value. `pub(crate)` limits this to code inside this
    /// binary; it is deliberately unreachable from a Tauri command.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

/// Both `Debug` (`{:?}`) and `Display` (`{}`) are overridden, because either one
/// left at its default would put the key into a log the first time someone
/// formatted the struct.
impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***)")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

fn entry(provider: Provider) -> anyhow::Result<Entry> {
    Ok(Entry::new(SERVICE, provider.account())?)
}

/// Store a key. Replaces any existing one for that provider.
pub fn set_key(provider: Provider, key: &str) -> anyhow::Result<()> {
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!("That key is empty.");
    }
    entry(provider)?.set_password(key)?;
    Ok(())
}

/// Whether a key exists — the only thing the frontend is allowed to learn.
pub fn has_key(provider: Provider) -> bool {
    matches!(entry(provider).map(|e| e.get_password()), Ok(Ok(_)))
}

/// Read a key, for making an API request. Not reachable from the frontend.
pub(crate) fn get_key(provider: Provider) -> anyhow::Result<SecretString> {
    Ok(SecretString(entry(provider)?.get_password()?))
}

pub fn delete_key(provider: Provider) -> anyhow::Result<()> {
    // Deleting a key that was never stored should be a no-op, not an error.
    // Checking first — rather than matching on a specific "not found" error
    // variant — keeps this working across keyring versions, which have moved
    // their error type between crates more than once.
    if !has_key(provider) {
        return Ok(());
    }
    entry(provider)?.delete_credential()?;
    Ok(())
}
