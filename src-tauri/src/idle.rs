//! System idle detection.
//!
//! The brief is blunt about why this exists: without it, "the data is fiction" —
//! a timer left running while you make dinner records three hours of study that
//! never happened.
//!
//! macOS exposes exactly the number we need through Core Graphics. No entitlement,
//! no Accessibility permission, no prompt. (An event *tap*, which is what people
//! usually reach for, would need Accessibility approval. Reading the idle counter
//! does not.)

// The macOS API we're calling. This block declares a function that lives in the
// CoreGraphics system framework rather than in our own code.
//
// `#[link(...)]` tells the linker which framework to bind against, and
// `extern "C"` says to use the C calling convention. Nothing is being
// reimplemented here — this is the same function any native Mac app would call.
//
// (Plain `//` comments rather than `///` doc comments: rustdoc doesn't generate
// documentation for extern blocks, and warns if you try.)
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    // Seconds since the last input event of the given type.
    //
    // Signature in C:
    //     CFTimeInterval CGEventSourceSecondsSinceLastEventType(
    //         CGEventSourceStateID sourceState, CGEventType eventType);
    fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
}

/// `kCGEventSourceStateHIDSystemState` — count only real hardware input.
///
/// The alternative, `kCGEventSourceStateCombinedSessionState` (0), also counts
/// synthetic events posted by software. We want the honest one: a script moving
/// the cursor should not make an idle machine look busy.
#[cfg(target_os = "macos")]
const HID_SYSTEM_STATE: i32 = 1;

/// `kCGAnyInputEventType` — any input at all (key, mouse move, click, scroll,
/// trackpad gesture). Defined as UINT32_MAX in CGEventTypes.h.
#[cfg(target_os = "macos")]
const ANY_INPUT_EVENT: u32 = u32::MAX;

/// How long the machine has been without input, in seconds.
///
/// `unsafe` is required because Rust cannot verify the behaviour of code outside
/// its own compilation — every foreign function call is `unsafe` by definition.
/// The call itself is a plain read of a system counter: it takes no pointers,
/// allocates nothing, and cannot fail, so there is no invariant for us to uphold
/// beyond passing the documented constants.
#[cfg(target_os = "macos")]
pub fn seconds_since_last_input() -> f64 {
    unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) }
}

/// Non-macOS builds get a stub that reports "always active".
///
/// This app ships for macOS only, but keeping the signature available on other
/// platforms means `cargo check` on a Linux CI box wouldn't fall over on this file.
#[cfg(not(target_os = "macos"))]
pub fn seconds_since_last_input() -> f64 {
    0.0
}
