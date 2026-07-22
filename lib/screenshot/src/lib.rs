use macroquad::prelude::*;

/// RNG seed: `HCG_SEED` env override for reproducible screenshots, else wall-clock.
/// `std::time::SystemTime::now()` panics on WASM, so this always goes through miniquad's clock.
pub fn seed() -> u64 {
    std::env::var("HCG_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| macroquad::miniquad::date::now() as u64)
}

/// Saves a PNG screenshot after a few seconds of (real, wall-clock) play and exits the
/// process, when `HCG_SCREENSHOT` is set. No-op (near-zero cost) when unset, so it's safe
/// to leave wired into every game's loop.
///
/// Triggers on elapsed wall-clock time rather than frame count: headless/software-rendered
/// runs are unthrottled and can blow through hundreds of frames in milliseconds, which would
/// capture the game barely past its initial state instead of a representative mid-play frame.
pub struct Capture {
    start: f64,
    after_secs: f64,
    path: Option<String>,
}

impl Capture {
    pub fn from_env() -> Self {
        let path = std::env::var("HCG_SCREENSHOT").ok();
        let after_secs = std::env::var("HCG_SCREENSHOT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3.0);
        Self { start: macroquad::miniquad::date::now(), after_secs, path }
    }

    /// Call once per frame, after drawing, before `next_frame().await`.
    pub fn tick(&mut self) {
        let Some(path) = &self.path else { return };
        if macroquad::miniquad::date::now() - self.start >= self.after_secs {
            get_screen_data().export_png(path);
            std::process::exit(0);
        }
    }
}

/// `S` hotkey: save a screenshot of the current frame. Native only — writes a
/// timestamped PNG to the working directory via `std::fs`, which doesn't exist on WASM.
/// In the browser the page itself handles `S` (see `xtask::screenshot_bridge`) by reading
/// pixels straight off the canvas with `toBlob()` and prompting a download, which needs no
/// Rust involvement at all.
pub fn handle_hotkey() {
    #[cfg(not(target_arch = "wasm32"))]
    if is_key_pressed(KeyCode::S) {
        let filename = format!("screenshot-{}.png", macroquad::miniquad::date::now() as u64);
        get_screen_data().export_png(&filename);
        println!("Saved screenshot to {filename}");
    }
}
