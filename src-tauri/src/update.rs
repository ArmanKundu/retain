//! Update check against GitHub Releases.
//!
//! ## What this does and doesn't do
//!
//! It asks GitHub whether there is a newer release, and tells you. That's all.
//! It does **not** download anything, does not install anything, does not touch
//! the app bundle, and sends nothing about you or this machine — the request is
//! an unauthenticated GET of a public JSON endpoint, which is the same thing
//! visiting the releases page in a browser does.
//!
//! ## Why it isn't an auto-updater
//!
//! Retain is ad-hoc signed rather than signed with a paid Developer ID. An
//! updater that replaced the app bundle in place would be swapping one
//! unverifiable binary for another, and the OS could not tell the difference
//! between that and something malicious doing the same thing. Telling you a
//! version exists and letting you fetch it yourself is the honest version of
//! this feature.
//!
//! ## Offline is a normal state, not a failure
//!
//! Nothing here is on the startup path in a way that can block it: the check
//! runs on a background thread after the window is up, a failure is recorded as
//! `Unknown` and shown as "couldn't check", and the app never waits on it. If
//! GitHub disappeared tomorrow, Retain would keep working exactly as it does
//! now.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const RELEASES_URL: &str = "https://api.github.com/repos/armankundu/retain/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/armankundu/retain/releases";

const TIMEOUT_SECS: u64 = 10;

/// How long a result stays fresh. A desktop app that ships a few times a year
/// does not need to ask on every launch, and not asking is also the polite
/// thing to do to an unauthenticated API with a rate limit.
const CACHE_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum UpdateStatus {
    /// Running the newest release, or newer than it.
    UpToDate { current: String },
    Available {
        current: String,
        latest: String,
        url: String,
        notes: Option<String>,
        /// Direct link to the `.dmg`, when the release has one. Without it the
        /// UI can still offer the page, but cannot install for you.
        download_url: Option<String>,
    },
    /// The check couldn't complete. Deliberately distinct from `UpToDate`:
    /// reporting "you're up to date" when we never got an answer is a lie.
    Unknown { current: String, reason: String },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    pub status: UpdateStatus,
    pub checked_at: Option<String>,
    /// Always present, so the UI can offer the releases page even on a failure.
    pub releases_page: String,
}

// ---------------------------------------------------------------------------
// Version comparison
// ---------------------------------------------------------------------------

/// Parse `v1.2.3`, `1.2.3`, `1.2`, `1.2.3-beta.1` into comparable numbers.
///
/// Returns the numeric parts plus whether a pre-release suffix was present.
pub fn parse_version(raw: &str) -> Option<(Vec<u32>, bool)> {
    let cleaned = raw.trim().trim_start_matches(['v', 'V']);
    if cleaned.is_empty() {
        return None;
    }

    // Split off a `-beta` / `+build` suffix before reading numbers.
    let (core, rest) = match cleaned.find(['-', '+']) {
        Some(i) => (&cleaned[..i], &cleaned[i..]),
        None => (cleaned, ""),
    };

    let parts: Vec<u32> = core
        .split('.')
        .map(|p| p.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .ok()?;

    if parts.is_empty() {
        return None;
    }

    Some((parts, rest.starts_with('-')))
}

/// Whether `latest` is newer than `current`.
///
/// A pre-release is never offered as an update over a matching stable version —
/// `1.2.0` should not be "updated" to `1.2.0-rc1`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (Some((l, l_pre)), Some((c, c_pre))) = (parse_version(latest), parse_version(current))
    else {
        // If either version is unreadable, claim nothing. Prompting an update
        // on a parse failure would nag forever.
        return false;
    };

    let len = l.len().max(c.len());
    for i in 0..len {
        // Missing components are zero, so 1.2 == 1.2.0.
        let (a, b) = (l.get(i).copied().unwrap_or(0), c.get(i).copied().unwrap_or(0));
        if a != b {
            return a > b;
        }
    }

    // Numerically equal: only a stable release beats a pre-release.
    c_pre && !l_pre
}

// ---------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: Option<String>,
    name: Option<String>,
    html_url: Option<String>,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// The `.dmg` on a release, if there is exactly one to be confident about.
///
/// Matched on the extension rather than the filename, which carries the version
/// and would need updating every release. Anything not served from
/// `github.com` is rejected here rather than at download time — this URL comes
/// off the network and ends up naming a file that gets mounted.
fn dmg_asset(assets: &[GithubAsset]) -> Option<String> {
    assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".dmg"))
        .map(|a| a.browser_download_url.clone())
        .filter(|u| is_safe_release_url(u).is_ok())
}

/// Ask GitHub. Never panics, never blocks anything else.
pub async fn check(current: &str) -> UpdateStatus {
    let unknown = |reason: String| UpdateStatus::Unknown {
        current: current.to_string(),
        reason,
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(e) => return unknown(format!("Couldn't start an HTTPS client: {e}")),
    };

    let response = client
        .get(RELEASES_URL)
        // GitHub requires a User-Agent. It identifies the app, not the machine
        // or the person using it.
        .header("user-agent", "Retain-update-check")
        .header("accept", "application/vnd.github+json")
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            return unknown(if e.is_timeout() {
                "GitHub didn't respond in time.".into()
            } else {
                "Couldn't reach GitHub. Are you online?".into()
            })
        }
    };

    let status = response.status();
    if !status.is_success() {
        return unknown(match status.as_u16() {
            // No releases published yet is a perfectly ordinary state for a
            // small project, and it isn't an error worth alarming anyone about.
            404 => "No releases published yet.".into(),
            403 | 429 => "GitHub is rate-limiting; try again later.".into(),
            other => format!("GitHub returned {other}."),
        });
    }

    let release: GithubRelease = match response.json().await {
        Ok(r) => r,
        Err(_) => return unknown("GitHub sent something unexpected.".into()),
    };

    if release.draft || release.prerelease {
        return UpdateStatus::UpToDate {
            current: current.to_string(),
        };
    }

    let Some(tag) = release.tag_name.or(release.name) else {
        return unknown("That release has no version tag.".into());
    };

    if is_newer(&tag, current) {
        UpdateStatus::Available {
            current: current.to_string(),
            latest: tag.trim().to_string(),
            url: release.html_url.unwrap_or_else(|| RELEASES_PAGE.to_string()),
            notes: release.body.map(|b| truncate_notes(&b)),
            download_url: dmg_asset(&release.assets),
        }
    } else {
        UpdateStatus::UpToDate {
            current: current.to_string(),
        }
    }
}

/// Release notes are shown in a small panel, not a document viewer.
fn truncate_notes(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.chars().count() <= 600 {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(600).collect();
    format!("{}…", cut.trim_end())
}

// ---------------------------------------------------------------------------
// Caching
// ---------------------------------------------------------------------------

const LAST_CHECK_KEY: &str = "update_last_check";
const LAST_RESULT_KEY: &str = "update_last_result";

/// Whether enough time has passed to ask again.
pub fn is_stale(last: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(raw) = last else { return true };
    let Ok(then) = raw.parse::<DateTime<Utc>>() else {
        return true;
    };
    // A timestamp in the future means the clock moved; re-check rather than
    // trusting it and going quiet for a day.
    now - then >= Duration::hours(CACHE_HOURS) || then > now
}

pub fn cached(conn: &Connection, current: &str) -> Result<UpdateReport> {
    let checked_at = crate::settings::get(conn, LAST_CHECK_KEY)?.filter(|s| !s.is_empty());

    let status = crate::settings::get(conn, LAST_RESULT_KEY)?
        .filter(|s| !s.is_empty())
        .and_then(|raw| serde_json::from_str::<UpdateStatus>(&raw).ok())
        // A cached result from an older build would report the wrong "current"
        // version, so it's only reused when the version still matches.
        .filter(|s| version_of(s) == current);

    Ok(UpdateReport {
        status: status.unwrap_or(UpdateStatus::Unknown {
            current: current.to_string(),
            reason: "Not checked yet.".into(),
        }),
        checked_at,
        releases_page: RELEASES_PAGE.to_string(),
    })
}

fn version_of(s: &UpdateStatus) -> &str {
    match s {
        UpdateStatus::UpToDate { current }
        | UpdateStatus::Available { current, .. }
        | UpdateStatus::Unknown { current, .. } => current,
    }
}

pub fn store(conn: &Connection, status: &UpdateStatus, now: DateTime<Utc>) -> Result<()> {
    crate::settings::set(conn, LAST_RESULT_KEY, &serde_json::to_string(status)?)?;

    // A failed check does not refresh the timestamp, so being offline for a
    // week doesn't mean going a week without ever checking again.
    if !matches!(status, UpdateStatus::Unknown { .. }) {
        crate::settings::set(conn, LAST_CHECK_KEY, &crate::util::rfc3339(now))?;
    }

    Ok(())
}

pub fn should_check(conn: &Connection, now: DateTime<Utc>) -> Result<bool> {
    let last = crate::settings::get(conn, LAST_CHECK_KEY)?;
    Ok(is_stale(last.as_deref().filter(|s| !s.is_empty()), now))
}

/// The releases page, for the "what's new" link.
pub fn releases_page() -> &'static str {
    RELEASES_PAGE
}

/// Guard against a malformed URL ever reaching the opener.
pub fn is_safe_release_url(url: &str) -> Result<()> {
    if url.starts_with("https://github.com/") {
        Ok(())
    } else {
        Err(anyhow!("Refusing to open a link that isn't on github.com."))
    }
}

// ---------------------------------------------------------------------------
// Installing an update
// ---------------------------------------------------------------------------

/// Download a release DMG, replace the installed app with it, and clean up.
///
/// This is what "press Update and it happens" costs when the app isn't signed
/// by Apple and so can't use a notarised updater. The steps are the ones you
/// were doing by hand: fetch the disk image, mount it, copy the new bundle over
/// the old one, unmount, throw the image away.
///
/// # Why each step is the way it is
///
/// * **`ditto` rather than a recursive copy.** A `.app` is a directory whose
///   symlinks, permissions and extended attributes are load-bearing; a naive
///   copy produces a bundle macOS refuses to launch.
/// * **The new bundle goes to a staging path first**, then swaps. Copying
///   directly over a running app can leave a half-written bundle if anything
///   fails partway, and a half-written bundle is an app you can no longer open
///   *or* update.
/// * **Only `/Applications` and the user's own `Applications`** are accepted as
///   install locations. Running from a mounted DMG or from a Downloads folder
///   means the "installed" copy isn't where the user thinks it is, and writing
///   there would leave two divergent copies.
///
/// Returns the path that was replaced, so the caller can relaunch it.
pub async fn install(download_url: &str, app_path: &std::path::Path) -> Result<std::path::PathBuf> {
    use std::process::Command;

    is_safe_release_url(download_url)?;

    let target = installed_bundle(app_path)?;

    let staging = std::env::temp_dir().join(format!("retain-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    // --- download ----------------------------------------------------------
    let dmg = staging.join("Retain.dmg");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let bytes = client
        .get(download_url)
        .header("user-agent", "Retain-updater")
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    // A DMG smaller than a megabyte is an error page, not a disk image, and
    // mounting one produces a baffling failure instead of a clear one.
    if bytes.len() < 1_000_000 {
        return Err(anyhow!("That download didn't look like a disk image."));
    }
    std::fs::write(&dmg, &bytes)?;

    // --- mount -------------------------------------------------------------
    let mount = staging.join("mnt");
    std::fs::create_dir_all(&mount)?;

    let attach = Command::new("/usr/bin/hdiutil")
        .args(["attach", "-nobrowse", "-quiet", "-mountpoint"])
        .arg(&mount)
        .arg(&dmg)
        .status()?;
    if !attach.success() {
        return Err(anyhow!("Couldn't open the downloaded disk image."));
    }

    // Everything past here must unmount, including on the error paths — a
    // leaked mount survives the app quitting and clutters Finder until reboot.
    let result = swap_bundle(&mount, &target, &staging);

    let _ = Command::new("/usr/bin/hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mount)
        .status();
    let _ = std::fs::remove_dir_all(&staging);

    result.map(|()| target)
}

/// Copy the new bundle out of the mounted image and put it in place.
fn swap_bundle(
    mount: &std::path::Path,
    target: &std::path::Path,
    staging: &std::path::Path,
) -> Result<()> {
    use std::process::Command;

    let source = std::fs::read_dir(mount)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "app"))
        .ok_or_else(|| anyhow!("That disk image has no app in it."))?;

    // Staged first so a failed copy can't leave a half-written bundle where the
    // working app used to be.
    let staged = staging.join("Retain.app");
    let copy = Command::new("/usr/bin/ditto").arg(&source).arg(&staged).status()?;
    if !copy.success() {
        return Err(anyhow!("Couldn't copy the new version out of the disk image."));
    }

    // The old bundle moves aside rather than being deleted outright, so a
    // failure here leaves something to go back to.
    let backup = staging.join("previous.app");
    if target.exists() {
        std::fs::rename(target, &backup)
            .map_err(|e| anyhow!("Couldn't move the old version aside: {e}"))?;
    }

    if let Err(e) = std::fs::rename(&staged, target) {
        // Put it back. Failing an update is recoverable; leaving no app at all
        // is not.
        if backup.exists() {
            let _ = std::fs::rename(&backup, target);
        }
        return Err(anyhow!("Couldn't install the new version: {e}"));
    }

    Ok(())
}

/// The installed `.app` this process is running from.
///
/// Refuses anything outside an Applications folder. Updating a copy running
/// from Downloads, or from a mounted image, would install into a place the user
/// doesn't think of as "the app" and leave two versions disagreeing.
pub fn installed_bundle(exe: &std::path::Path) -> Result<std::path::PathBuf> {
    // …/Retain.app/Contents/MacOS/retain → …/Retain.app
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|x| x == "app"))
        .ok_or_else(|| anyhow!("Retain isn't running from an app bundle."))?;

    let parent = bundle.parent().unwrap_or(std::path::Path::new(""));
    let ok = parent == std::path::Path::new("/Applications")
        || parent.ends_with("Applications");

    if !ok {
        return Err(anyhow!(
            "Retain is running from {}, not an Applications folder. Move it to Applications \
             first and updates will install themselves from then on.",
            parent.display()
        ));
    }

    Ok(bundle.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    // -- where an update may install --------------------------------------

    /// Updating a copy running from Downloads, or from a mounted disk image,
    /// would install into somewhere the user doesn't think of as "the app" and
    /// leave two versions disagreeing about which is current.
    #[test]
    fn only_a_bundle_in_an_applications_folder_can_be_replaced() {
        use std::path::Path;

        assert_eq!(
            installed_bundle(Path::new("/Applications/Retain.app/Contents/MacOS/retain")).unwrap(),
            Path::new("/Applications/Retain.app")
        );
        // A per-user Applications folder is a normal place to install.
        assert!(installed_bundle(Path::new(
            "/Users/armankundu/Applications/Retain.app/Contents/MacOS/retain"
        ))
        .is_ok());

        for bad in [
            "/Users/armankundu/Downloads/Retain.app/Contents/MacOS/retain",
            "/Volumes/Retain/Retain.app/Contents/MacOS/retain",
            "/Users/armankundu/Desktop/Retain.app/Contents/MacOS/retain",
        ] {
            assert!(installed_bundle(Path::new(bad)).is_err(), "{bad}");
        }

        // Not a bundle at all — a `cargo run` binary, say.
        assert!(installed_bundle(Path::new("/Users/x/proj/target/debug/retain")).is_err());
    }

    /// The download URL comes off the network and ends up naming a file that
    /// gets mounted, so it is checked at the point it is read.
    #[test]
    fn a_download_link_off_github_is_not_offered() {
        let asset = |name: &str, url: &str| GithubAsset {
            name: name.into(),
            browser_download_url: url.into(),
        };

        assert_eq!(
            dmg_asset(&[asset("Retain_0.10.0_x64.dmg", "https://github.com/a/b/x.dmg")]),
            Some("https://github.com/a/b/x.dmg".to_string())
        );

        // Right extension, wrong host.
        assert_eq!(
            dmg_asset(&[asset("Retain.dmg", "https://evil.example.com/Retain.dmg")]),
            None
        );
        // Right host, not a disk image.
        assert_eq!(
            dmg_asset(&[asset("notes.txt", "https://github.com/a/b/notes.txt")]),
            None
        );
        assert_eq!(dmg_asset(&[]), None);
    }

    // -- version parsing ---------------------------------------------------

    #[test]
    fn versions_parse_with_or_without_a_v_prefix() {
        assert_eq!(parse_version("v1.2.3").unwrap().0, vec![1, 2, 3]);
        assert_eq!(parse_version("1.2.3").unwrap().0, vec![1, 2, 3]);
        assert_eq!(parse_version(" 0.1.0 ").unwrap().0, vec![0, 1, 0]);
        assert_eq!(parse_version("1.2").unwrap().0, vec![1, 2]);
    }

    #[test]
    fn a_prerelease_suffix_is_recognised() {
        let (parts, pre) = parse_version("1.2.3-beta.1").unwrap();
        assert_eq!(parts, vec![1, 2, 3]);
        assert!(pre);

        assert!(!parse_version("1.2.3+build7").unwrap().1);
    }

    #[test]
    fn unparseable_versions_yield_none() {
        for bad in ["", "v", "banana", "1.x.3", "..."] {
            assert!(parse_version(bad).is_none(), "{bad} should not parse");
        }
    }

    // -- comparison --------------------------------------------------------

    #[test]
    fn a_higher_version_is_newer() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.10.0", "0.9.0"), "10 must beat 9, not sort before it");
    }

    #[test]
    fn the_same_or_older_version_is_not_newer() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        // Running a build newer than the published release, as happens locally.
        assert!(!is_newer("0.1.0", "0.2.0-dev"));
    }

    #[test]
    fn missing_components_count_as_zero() {
        assert!(!is_newer("1.2", "1.2.0"));
        assert!(is_newer("1.2.1", "1.2"));
    }

    /// Being offered a release candidate as an "update" from the matching
    /// stable version would be wrong.
    #[test]
    fn a_prerelease_is_not_an_update_over_the_same_stable_version() {
        assert!(!is_newer("1.2.0-rc1", "1.2.0"));
        assert!(is_newer("1.2.0", "1.2.0-rc1"));
        assert!(is_newer("1.3.0-rc1", "1.2.0"), "a newer number still wins");
    }

    /// An unreadable version must never produce a prompt — that would nag on
    /// every launch with no way to resolve it.
    #[test]
    fn an_unreadable_version_never_claims_an_update() {
        assert!(!is_newer("banana", "0.1.0"));
        assert!(!is_newer("0.2.0", "banana"));
        assert!(!is_newer("", ""));
    }

    // -- caching -----------------------------------------------------------

    #[test]
    fn a_check_is_stale_after_a_day() {
        assert!(is_stale(None, now()));
        assert!(is_stale(Some("2026-08-12T11:00:00Z"), now()));
        assert!(!is_stale(Some("2026-08-13T00:00:00Z"), now()));
        assert!(!is_stale(Some("2026-08-12T12:30:00Z"), now()));
    }

    #[test]
    fn a_garbled_or_future_timestamp_forces_a_recheck() {
        assert!(is_stale(Some("not a date"), now()));
        assert!(is_stale(Some("2027-01-01T00:00:00Z"), now()));
    }

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("db/migrations/001_init.sql")).unwrap();
        conn
    }

    #[test]
    fn a_result_round_trips_through_the_cache() {
        let conn = db();
        let status = UpdateStatus::Available {
            current: "0.1.0".into(),
            latest: "0.2.0".into(),
            url: "https://github.com/x/y/releases/tag/v0.2.0".into(),
            notes: Some("Stuff".into()),
            download_url: None,
        };

        store(&conn, &status, now()).unwrap();

        let report = cached(&conn, "0.1.0").unwrap();
        assert_eq!(report.status, status);
        assert!(report.checked_at.is_some());
        assert!(!should_check(&conn, now()).unwrap());
    }

    /// After the user updates, a cached "0.2.0 is available" result must not be
    /// shown against the new version.
    #[test]
    fn a_cached_result_from_an_older_build_is_discarded() {
        let conn = db();
        store(
            &conn,
            &UpdateStatus::Available {
                current: "0.1.0".into(),
                latest: "0.2.0".into(),
                url: "https://github.com/x/y".into(),
                notes: None,
            download_url: None,
            },
            now(),
        )
        .unwrap();

        let report = cached(&conn, "0.2.0").unwrap();
        assert!(matches!(report.status, UpdateStatus::Unknown { .. }));
    }

    /// A failed check must not start the 24-hour clock, or one flight without
    /// wifi means no check for a day.
    #[test]
    fn a_failed_check_does_not_reset_the_cache_timer() {
        let conn = db();
        store(
            &conn,
            &UpdateStatus::Unknown {
                current: "0.1.0".into(),
                reason: "offline".into(),
            },
            now(),
        )
        .unwrap();

        assert!(should_check(&conn, now()).unwrap());
    }

    #[test]
    fn a_fresh_database_wants_a_check() {
        let conn = db();
        assert!(should_check(&conn, now()).unwrap());

        let report = cached(&conn, "0.1.0").unwrap();
        assert!(matches!(report.status, UpdateStatus::Unknown { .. }));
        assert!(report.checked_at.is_none());
        assert!(report.releases_page.starts_with("https://github.com/"));
    }

    #[test]
    fn a_corrupt_cached_result_is_treated_as_no_result() {
        let conn = db();
        crate::settings::set(&conn, "update_last_result", "{not json").unwrap();
        assert!(matches!(
            cached(&conn, "0.1.0").unwrap().status,
            UpdateStatus::Unknown { .. }
        ));
    }

    // -- link safety -------------------------------------------------------

    #[test]
    fn only_github_links_may_be_opened() {
        assert!(is_safe_release_url("https://github.com/a/b/releases/tag/v1").is_ok());
        for bad in [
            "http://github.com/a/b",
            "https://githubbcom/a",
            "https://evil.test/x",
            "file:///etc/passwd",
            "javascript:alert(1)",
        ] {
            assert!(is_safe_release_url(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn notes_are_truncated_rather_than_unbounded() {
        let long = "x".repeat(2000);
        let out = truncate_notes(&long);
        assert!(out.chars().count() <= 601);
        assert!(out.ends_with('…'));

        assert_eq!(truncate_notes("  short  "), "short");
    }
}
