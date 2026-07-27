use macroquad::prelude::*;

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    /// `rgba_ptr`/`rgba_len` is raw, top-row-first RGBA8 pixel data (`width * height * 4`
    /// bytes, already flipped from `get_screen_data()`'s bottom-up GL readback order —
    /// see `handle_hotkey`) ready to hand straight to `new ImageData(...)`. No encoding
    /// done in Rust at all — the JS side (`xtask::screenshot_bridge`) builds a 2D canvas
    /// from it and does the PNG encoding + download there, so this crate stays
    /// dependency-free.
    fn hcg_save_screenshot(
        rgba_ptr: *const u8,
        rgba_len: u32,
        width: u32,
        height: u32,
        name_ptr: *const u8,
        name_len: u32,
    );
}

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
        Self {
            start: macroquad::miniquad::date::now(),
            after_secs,
            path,
        }
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

/// `S` hotkey: save a screenshot of the current frame. Native writes a timestamped PNG
/// straight to the working directory via `std::fs`.
///
/// WASM has no filesystem, so it hands raw pixel bytes to a JS plugin
/// (`xtask::screenshot_bridge`) that encodes and downloads them instead — reading pixels
/// via `get_screen_data()` here (synchronously, inside this same Rust frame) rather than
/// the page listening for `S` itself and reading the canvas with `canvas.toBlob()`, which
/// this replaced. That approach depended on `preserveDrawingBuffer: true` on the WebGL
/// context, which `mq_js_bundle.js` (fetched, not ours to patch — see root CLAUDE.md's
/// "Site generation" section) doesn't set; without it the browser is free to discard the
/// drawing buffer as soon as it composites a frame, and by the time an async
/// `keydown`-triggered `toBlob()` callback ran, the buffer was reliably already gone —
/// every capture came back fully transparent/black (reproduced both mid-play and with
/// the game paused, so it wasn't about content still changing). `get_screen_data()`
/// reads pixels before any browser-side compositing/clearing can happen, sidestepping
/// the timing issue entirely — the same mechanism `Capture` (this file, used by the
/// build's own screenshot step) already relied on, just never wired to the live hotkey
/// on WASM before.
pub fn handle_hotkey() {
    if !is_key_pressed(KeyCode::S) {
        return;
    }
    let name = format!("screenshot-{}", macroquad::miniquad::date::now() as u64);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let filename = format!("{name}.png");
        get_screen_data().export_png(&filename);
        println!("Saved screenshot to {filename}");
    }
    #[cfg(target_arch = "wasm32")]
    {
        let img = get_screen_data();
        let (w, h) = (img.width as usize, img.height as usize);
        // `get_screen_data()` reads bottom-row-first (OpenGL's native order) — flip to
        // top-row-first, same as `Image::export_png` does before writing a file, so the
        // JS side can hand this straight to `new ImageData(...)` without its own flip.
        let mut flipped = vec![0u8; img.bytes.len()];
        let row_bytes = w * 4;
        for y in 0..h {
            let src = (h - y - 1) * row_bytes;
            let dst = y * row_bytes;
            flipped[dst..dst + row_bytes].copy_from_slice(&img.bytes[src..src + row_bytes]);
        }
        unsafe {
            hcg_save_screenshot(
                flipped.as_ptr(),
                flipped.len() as u32,
                w as u32,
                h as u32,
                name.as_ptr(),
                name.len() as u32,
            );
        }
    }
}
