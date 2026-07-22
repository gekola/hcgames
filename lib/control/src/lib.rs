use macroquad::prelude::*;

const STEP: f32 = 1.1;
const MIN_MULT: f32 = 0.1;
const MAX_MULT: f32 = 10.0;

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn hcg_ga_event(name_ptr: *const u8, name_len: u32, params_ptr: *const u8, params_len: u32);
}

/// Fires a Google Analytics event (`gtag('event', name, params)`) via the small JS plugin
/// the page registers before the wasm module loads (see `xtask::analytics_bridge`).
/// `params_json` is a JSON object literal body, e.g. `{"episode":3,"score":140}`.
/// No-op on native, and harmless (silently dropped by the bridge) when `GTAG_ID` is unset.
fn ga_event(name: &str, params_json: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        hcg_ga_event(name.as_ptr(), name.len() as u32, params_json.as_ptr(), params_json.len() as u32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (name, params_json);
    }
}

/// Simulation-speed multiplier, adjustable via hotkeys (`=`/`-` step by 10%, `0` resets
/// to 1x, `Space` pauses), plus episode-progress tracking that reports a
/// `episode_complete` GA event each time a game round ends.
pub struct Control {
    mult: f32,
    paused: bool,
    episode: u64,
}

impl Control {
    pub fn new() -> Self {
        Self { mult: 1.0, paused: false, episode: 0 }
    }

    pub fn handle_keys(&mut self) {
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            self.mult = (self.mult * STEP).min(MAX_MULT);
        }
        if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
            self.mult = (self.mult / STEP).max(MIN_MULT);
        }
        if is_key_pressed(KeyCode::Key0) || is_key_pressed(KeyCode::Kp0) {
            self.mult = 1.0;
        }
        if is_key_pressed(KeyCode::Space) {
            self.paused = !self.paused;
        }
    }

    /// Zero while paused, otherwise `dt` scaled by the speed multiplier.
    pub fn scale(&self, dt: f32) -> f32 {
        if self.paused { 0.0 } else { dt * self.mult }
    }

    /// `x1.000`-style label for the in-canvas HUD, or `PAUSED` when paused.
    pub fn label(&self) -> String {
        if self.paused { "PAUSED".to_owned() } else { format!("x{:.3}", self.mult) }
    }

    /// Call when a game round ends. Bumps the episode counter and reports it, with the
    /// round's final `score`, as a GA event.
    pub fn episode_complete(&mut self, game: &str, score: i64) {
        self.episode += 1;
        ga_event(
            "episode_complete",
            &format!("{{\"game\":\"{game}\",\"episode\":{},\"score\":{score}}}", self.episode),
        );
    }
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}
