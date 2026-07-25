use macroquad::prelude::*;

const STEP: f32 = 1.1;
const MIN_MULT: f32 = 0.1;
const MAX_MULT: f32 = 10.0;
#[cfg(not(target_arch = "wasm32"))]
const DOUBLE_CLICK_SECS: f64 = 0.4;
/// Horizontal drag distance (px) per `STEP` multiplier change during a two-finger slide.
const TWO_FINGER_PX_PER_STEP: f32 = 80.0;
/// Minimum straight-line distance (px) for a one-finger touch to count as a swipe.
const SWIPE_MIN_DIST: f32 = 60.0;
/// Swipes slower than this (start to release) are treated as a drag/tap, not a swipe.
const SWIPE_MAX_SECS: f64 = 0.6;

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
        hcg_ga_event(
            name.as_ptr(),
            name.len() as u32,
            params_json.as_ptr(),
            params_json.len() as u32,
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (name, params_json);
    }
}

/// Simulation-speed multiplier, adjustable via hotkeys (`=`/`-` step by 10%, `0` resets
/// to 1x, `Space` pauses; on native, `F`/double-click also toggles fullscreen — on WASM
/// that's `xtask::fullscreen_bridge` instead, see its doc comment for why), plus
/// episode-progress tracking that reports a `episode_complete` GA event each time a game
/// round ends. Touch equivalents for the two hotkeys that matter on a spectator page with
/// no keyboard: a two-finger slide scrubs speed the same way `=`/`-` do, and a one-finger
/// swipe (`variant_swipe()`) stands in for `V`'s variant cycle on the games that have one.
pub struct Control {
    mult: f32,
    paused: bool,
    episode: u64,
    #[cfg(not(target_arch = "wasm32"))]
    fullscreen: bool,
    #[cfg(not(target_arch = "wasm32"))]
    last_click: f64,
    /// (avg x of the two touches, `mult` at gesture start) while a two-finger drag is live.
    two_finger_anchor: Option<(f32, f32)>,
    /// (x, y, start time) of an in-progress single-finger touch.
    one_finger_start: Option<(f32, f32, f64)>,
    variant_swipe: bool,
}

impl Control {
    pub fn new() -> Self {
        Self {
            mult: 1.0,
            paused: false,
            episode: 0,
            #[cfg(not(target_arch = "wasm32"))]
            fullscreen: false,
            #[cfg(not(target_arch = "wasm32"))]
            last_click: f64::NEG_INFINITY,
            two_finger_anchor: None,
            one_finger_start: None,
            variant_swipe: false,
        }
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

        // WASM fullscreen is handled entirely by page-level JS (`xtask::fullscreen_bridge`)
        // instead: `macroquad::window::set_fullscreen` on WASM fullscreens the canvas
        // itself, which the browser then forces to `width/height: 100%` via an
        // unoverridable `!important` UA style, breaking the pinned-native-resolution
        // canvas this whole rendering setup depends on (see that function's doc comment).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut toggle_fullscreen = is_key_pressed(KeyCode::F);
            if is_mouse_button_pressed(MouseButton::Left) {
                let now = get_time();
                if now - self.last_click < DOUBLE_CLICK_SECS {
                    toggle_fullscreen = true;
                    // Consumed, so a third click starts a fresh pair instead of re-firing.
                    self.last_click = f64::NEG_INFINITY;
                } else {
                    self.last_click = now;
                }
            }
            if toggle_fullscreen {
                self.fullscreen = !self.fullscreen;
                set_fullscreen(self.fullscreen);
            }
        }

        self.handle_touch();
    }

    /// Two-finger horizontal slide scrubs `mult` (anchored to the multiplier at gesture
    /// start, so re-crossing the same drag distance always lands on the same speed); a
    /// quick one-finger swipe sets the one-frame `variant_swipe()` flag. Touch phases with
    /// a finger count other than 1 or 2 (none, or someone's third finger) reset both
    /// gestures so a new drag/swipe starts clean.
    fn handle_touch(&mut self) {
        self.variant_swipe = false;
        let touches = touches();

        match touches.len() {
            2 => {
                self.one_finger_start = None;
                let avg_x = (touches[0].position.x + touches[1].position.x) / 2.0;
                match self.two_finger_anchor {
                    Some((start_x, start_mult)) => {
                        let steps = (avg_x - start_x) / TWO_FINGER_PX_PER_STEP;
                        self.mult = (start_mult * STEP.powf(steps)).clamp(MIN_MULT, MAX_MULT);
                    }
                    None => self.two_finger_anchor = Some((avg_x, self.mult)),
                }
            }
            1 => {
                self.two_finger_anchor = None;
                let touch = &touches[0];
                match touch.phase {
                    TouchPhase::Started => {
                        self.one_finger_start =
                            Some((touch.position.x, touch.position.y, get_time()));
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if let Some((sx, sy, start_time)) = self.one_finger_start.take() {
                            let dx = touch.position.x - sx;
                            let dy = touch.position.y - sy;
                            let dist = (dx * dx + dy * dy).sqrt();
                            if dist >= SWIPE_MIN_DIST && get_time() - start_time <= SWIPE_MAX_SECS {
                                self.variant_swipe = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                self.two_finger_anchor = None;
                self.one_finger_start = None;
            }
        }
    }

    /// True for one frame when a one-finger swipe just completed — the touch equivalent
    /// of the `V` variant-cycle hotkey. Games without a variant cycle can ignore it.
    pub fn variant_swipe(&self) -> bool {
        self.variant_swipe
    }

    /// Zero while paused, otherwise `dt` scaled by the speed multiplier.
    pub fn scale(&self, dt: f32) -> f32 {
        if self.paused { 0.0 } else { dt * self.mult }
    }

    /// `x1.000`-style label for the in-canvas HUD, or `PAUSED` when paused.
    pub fn label(&self) -> String {
        if self.paused {
            "PAUSED".to_owned()
        } else {
            format!("x{:.3}", self.mult)
        }
    }

    /// Call when a game round ends. Bumps the episode counter and reports it, with the
    /// round's final `score`, as a GA event.
    pub fn episode_complete(&mut self, game: &str, score: i64) {
        self.episode += 1;
        ga_event(
            "episode_complete",
            &format!(
                "{{\"game\":\"{game}\",\"episode\":{},\"score\":{score}}}",
                self.episode
            ),
        );
    }
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}
