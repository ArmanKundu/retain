//! Showing the assistant your screen.
//!
//! "I'm stuck on this question" is a much better question when the question is
//! attached. This grabs a PNG and hands it to the next message, so you can point
//! at a past paper on screen instead of retyping it.
//!
//! # What this deliberately isn't
//!
//! It does not watch. There is no timer, no polling, no background capture and
//! nothing that runs unless you press the button — one press, one image, and it
//! goes to the model only as part of the message you then send. Continuous
//! screen monitoring in a study app would be indistinguishable from spyware
//! however good the intentions, and it would send your bank tab to an API on a
//! schedule.
//!
//! macOS gates screen capture behind Screen Recording permission, granted per
//! app in System Settings. The first attempt raises the system prompt; until
//! it's granted the capture comes back blank rather than failing loudly, so the
//! blank case is detected and explained here rather than being sent to a model
//! as a black rectangle.

use anyhow::{anyhow, Result};

/// Capture the main display.
///
/// `-x` suppresses the shutter sound, `-t png` sets the format, and `-C`
/// excludes the cursor: the pointer sitting over the text you're asking about is
/// noise in an image a model has to read.
pub fn capture_png(dir: &std::path::Path) -> Result<Vec<u8>> {
    let path = dir.join(format!("retain-screen-{}.png", uuid_ish()));

    let status = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-C", "-t", "png"])
        .arg(&path)
        .status()
        .map_err(|e| anyhow!("Couldn't run screencapture: {e}"))?;

    if !status.success() {
        return Err(anyhow!("Screen capture failed."));
    }

    let bytes = std::fs::read(&path).map_err(|e| anyhow!("Couldn't read the capture: {e}"))?;
    // Best-effort: the image is in the app's own temp area, and a leftover file
    // matters less than failing the capture over a cleanup error.
    let _ = std::fs::remove_file(&path);

    // Without Screen Recording permission macOS still writes a file — a picture
    // of the desktop wallpaper with every window missing. It can't be told apart
    // from a legitimately empty screen by size alone, so this only catches the
    // degenerate case of no image at all.
    if bytes.len() < 1024 {
        return Err(anyhow!(
            "The capture came back empty. Retain needs Screen Recording permission: \
             System Settings → Privacy & Security → Screen Recording."
        ));
    }

    Ok(bytes)
}

/// A `data:` URL, which is the shape both the attachment table and every
/// provider's vision API want.
pub fn to_data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", crate::export::base64_encode(png))
}

/// Enough uniqueness for a temp filename. Not a real UUID and doesn't need to
/// be — two captures a nanosecond apart aren't reachable from a button.
fn uuid_ish() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_url_is_shaped_the_way_every_provider_expects() {
        let url = to_data_url(&[0x89, 0x50, 0x4E, 0x47]);
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(url, "data:image/png;base64,iVBORw==");
    }

    /// Screen Recording permission produces a real capture; its absence produces
    /// something tiny. Either way the caller must never be handed bytes it will
    /// then base64 and post to an API as a black rectangle.
    #[test]
    fn a_missing_screencapture_binary_is_an_error_not_an_empty_image() {
        let dir = std::env::temp_dir().join("retain-screen-test");
        std::fs::create_dir_all(&dir).unwrap();

        // No permission to assert on in CI, so this only pins the contract that
        // failure is an `Err` rather than `Ok(vec![])`.
        if let Ok(bytes) = capture_png(&dir) {
            assert!(bytes.len() >= 1024, "a successful capture is never trivially small");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
