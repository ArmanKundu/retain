//! Checking an API key against its provider before we store it.
//!
//! When you paste a key, Retain asks the provider whether it actually works and
//! then accepts or rejects it. A key that only fails the first time you try to
//! use a feature — potentially weeks later — is a bad trade for the two seconds
//! this takes.
//!
//! ## Three outcomes, not two
//!
//! The important design point is that "this key is wrong" and "I couldn't reach
//! the provider to ask" are different answers and must not be collapsed:
//!
//!   * `Valid`       — the provider accepted it. Store it.
//!   * `Invalid`     — the provider rejected it. Do NOT store it.
//!   * `Unreachable` — no answer. Offer to store it unchecked.
//!
//! Collapsing the third case into `Invalid` would mean a perfectly good key gets
//! refused because the wifi dropped, and the brief is explicit that the app has
//! to work offline.
//!
//! ## Endpoints
//!
//! Each provider is asked the cheapest question that still requires
//! authentication — a metadata endpoint, never a completion. Checking a key
//! costs no tokens and no money.
//!
//! | Provider   | Endpoint                                          | Auth header      |
//! |------------|---------------------------------------------------|------------------|
//! | Anthropic  | `GET /v1/models`                                  | `x-api-key`      |
//! | OpenAI     | `GET /v1/models`                                  | `Authorization`  |
//! | Gemini     | `GET /v1beta/models`                              | `x-goog-api-key` |
//! | OpenRouter | `GET /api/v1/key`                                 | `Authorization`  |
//!
//! Gemini also accepts its key as a `?key=` query parameter. We deliberately use
//! the header instead: query strings end up in server logs, proxy logs and
//! browser history in a way headers do not, and a credential does not belong in
//! a URL.

use std::time::Duration;

use serde::Serialize;

use crate::secrets::Provider;

/// How long to wait before calling it unreachable.
///
/// Deliberately generous. Measured against the real endpoints, OpenAI can take
/// close to twenty seconds to reject a bad key, so a "sensible" 5- or 10-second
/// timeout would report a perfectly definite rejection as a network problem.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The result of asking a provider about a key.
///
/// `#[serde(tag = "status")]` makes this arrive in TypeScript as a discriminated
/// union — `{status: "valid", ...} | {status: "invalid", ...} | ...` — which the
/// UI can switch on exhaustively.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum KeyCheck {
    Valid { detail: Option<String> },
    Invalid { message: String },
    Unreachable { message: String },
}

fn endpoint(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "https://api.anthropic.com/v1/models",
        Provider::OpenAi => "https://api.openai.com/v1/models",
        Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/models",
        Provider::OpenRouter => "https://openrouter.ai/api/v1/key",
    }
}

/// Ask the provider whether this key works.
///
/// The key is passed in, used once, and dropped. It is never logged, never
/// written to disk here, and never included in any message this function
/// returns — an error string containing the key would defeat the whole point of
/// keeping it in the Keychain.
pub async fn check(provider: Provider, key: &str) -> KeyCheck {
    let key = key.trim();
    if key.is_empty() {
        return KeyCheck::Invalid {
            message: "That's an empty key.".into(),
        };
    }

    let client = match reqwest::Client::builder().timeout(TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            return KeyCheck::Unreachable {
                message: format!("Couldn't start an HTTPS client: {e}"),
            }
        }
    };

    let mut request = client.get(endpoint(provider));

    request = match provider {
        Provider::Anthropic => request
            .header("x-api-key", key)
            // Anthropic requires an API version on every request. Without it the
            // call is rejected for a reason that has nothing to do with the key.
            .header("anthropic-version", "2023-06-01"),
        Provider::OpenAi | Provider::OpenRouter => {
            request.header("authorization", format!("Bearer {key}"))
        }
        Provider::Gemini => request.header("x-goog-api-key", key),
    };

    match request.send().await {
        Ok(response) => classify(provider, response.status().as_u16()),

        // No HTTP status at all: DNS failure, no route, TLS failure, timeout.
        // We learned nothing about the key, so we must not claim it's wrong.
        Err(e) => {
            let reason = if e.is_timeout() {
                format!("{} didn't respond within 30 seconds.", provider.label())
            } else if e.is_connect() {
                format!("Couldn't reach {}. Are you online?", provider.label())
            } else {
                format!("Couldn't reach {}.", provider.label())
            };
            KeyCheck::Unreachable { message: reason }
        }
    }
}

/// Turn an HTTP status into one of the three outcomes.
///
/// Two cases here are easy to get wrong, and both were confirmed against the
/// live endpoints rather than assumed:
///
///   * **Gemini rejects a bad key with 400, not 401.** Treating 401 as the only
///     rejection would report a plainly invalid Gemini key as a server fault and
///     invite the user to save it anyway.
///   * **429 means the key worked.** Being rate-limited or over quota requires
///     having been authenticated first, so it is a pass, not a failure.
fn classify(provider: Provider, status: u16) -> KeyCheck {
    let name = provider.label();

    match status {
        200..=299 => KeyCheck::Valid { detail: None },

        // Authenticated, but throttled or out of credit. The key is fine.
        429 => KeyCheck::Valid {
            detail: Some(format!(
                "{name} accepted the key but is rate-limiting or out of quota right now."
            )),
        },

        // The provider looked at the key and said no.
        //
        // The message is ours, not the provider's. OpenRouter answers a bad key
        // with "Missing Authentication header", which would tell someone who just
        // pasted a key that they hadn't provided one.
        400 | 401 => KeyCheck::Invalid {
            message: format!("{name} didn't accept that key. Check for a stray space or a partial paste."),
        },

        403 => KeyCheck::Invalid {
            message: format!(
                "{name} recognised the key but it isn't allowed to use this account. \
                 It may be restricted or revoked."
            ),
        },

        404 => KeyCheck::Unreachable {
            message: format!(
                "{name}'s API moved — Retain is checking an endpoint that no longer exists. \
                 This is a bug in Retain, not a problem with your key."
            ),
        },

        // The provider is broken, not the key.
        500..=599 => KeyCheck::Unreachable {
            message: format!("{name} is having trouble right now ({status}). Nothing to do with your key."),
        },

        other => KeyCheck::Unreachable {
            message: format!("{name} replied with an unexpected status ({other})."),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Short helpers so each assertion reads as one line.
    fn is_valid(status: u16) -> bool {
        matches!(classify(Provider::Anthropic, status), KeyCheck::Valid { .. })
    }
    fn is_invalid(status: u16) -> bool {
        matches!(classify(Provider::Anthropic, status), KeyCheck::Invalid { .. })
    }
    fn is_unreachable(status: u16) -> bool {
        matches!(classify(Provider::Anthropic, status), KeyCheck::Unreachable { .. })
    }

    #[test]
    fn success_is_valid() {
        assert!(is_valid(200));
    }

    /// The counterintuitive one. Being rate-limited or out of quota means the
    /// provider authenticated the key first — so it's a pass. Classifying 429 as
    /// a rejection would tell someone their working key is broken at exactly the
    /// moment they're using it heavily.
    #[test]
    fn rate_limited_means_the_key_worked() {
        assert!(is_valid(429));
    }

    /// Gemini rejects a bad key with 400, not 401 — confirmed against the live
    /// endpoint. If this ever regresses to "only 401 counts", a plainly invalid
    /// Gemini key gets reported as a server fault and offered for saving.
    #[test]
    fn four_hundred_is_a_rejection_not_a_server_fault() {
        assert!(is_invalid(400));
        assert!(is_invalid(401));
        assert!(is_invalid(403));
    }

    /// Anything where we didn't get a verdict must stay unreachable, so the UI
    /// offers "save without checking" rather than refusing a possibly-good key.
    #[test]
    fn no_verdict_is_unreachable() {
        assert!(is_unreachable(500));
        assert!(is_unreachable(503));
        assert!(is_unreachable(404)); // our endpoint is wrong, not their key
        assert!(is_unreachable(302));
    }

    /// No message we produce may echo the key back — that's the whole point of
    /// keeping it out of logs and out of the frontend.
    #[test]
    fn messages_never_contain_the_key() {
        for status in [200u16, 400, 401, 403, 404, 429, 500] {
            let rendered = format!("{:?}", classify(Provider::Anthropic, status));
            assert!(!rendered.contains("sk-"), "status {status} leaked a key prefix");
        }
    }
}
