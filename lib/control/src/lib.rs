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
    fn hcg_is_stream_mode() -> i32;
    fn hcg_is_daily_mode() -> i32;
    fn hcg_offer_share(text_ptr: *const u8, text_len: u32);
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

/// Reveals the page's hidden "Share Result" button with `text` pre-filled (see
/// `xtask::share_result_bridge`) — call once, right when a daily-challenge run ends.
/// Doesn't share directly: `navigator.share()` requires a real user gesture (a click) to
/// fire, which a call from inside the wasm frame loop isn't, so the JS side stores `text`
/// and waits for the button click instead. No-op on native (no page to show a button on).
pub fn share_result(text: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        hcg_offer_share(text.as_ptr(), text.len() as u32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = text;
    }
}

/// Whole UTC days since the Unix epoch, per `macroquad::miniquad::date::now()` (real
/// wall-clock time on both native and WASM — `std::time::SystemTime` panics on WASM, same
/// reason `Control::seed`'s own wall-clock fallback goes through the miniquad clock too).
fn today_day_index() -> u64 {
    macroquad::miniquad::date::now() as u64 / 86_400
}

/// Stable, ever-increasing puzzle number for a daily-challenge share (see
/// `daily_verdict_text`) — independent of `Control::seed`'s hash, just a human-readable
/// day count since a fixed epoch (2024-01-01T00:00:00Z).
pub fn daily_puzzle_number() -> u64 {
    const EPOCH_DAY: u64 = 19_723; // 2024-01-01 in days-since-epoch
    today_day_index().saturating_sub(EPOCH_DAY)
}

/// Wording for a daily-challenge share (see `share_result`) — one shared template so
/// every game's daily result reads in the same voice instead of being hand-written per
/// game. Deliberately not framed as "your score": there's no player input anywhere in
/// this workspace, so every visitor watching today's run sees the identical outcome —
/// there's nothing personal to have achieved. First-person "I watched" keeps it readable
/// as something a visitor is actually saying when they share it. `result_clause` is the
/// game-specific bit, completing "I watched {title} ___ today" (e.g. `"score 40"`,
/// `"solve it in 87 ticks"`).
pub fn daily_verdict_text(title: &str, puzzle_number: u64, result_clause: &str) -> String {
    format!("Case #{puzzle_number}: I watched {title} {result_clause} today.")
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
    stream_mode: bool,
    daily_mode: bool,
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
            #[cfg(target_arch = "wasm32")]
            stream_mode: unsafe { hcg_is_stream_mode() != 0 },
            #[cfg(not(target_arch = "wasm32"))]
            stream_mode: false,
            #[cfg(target_arch = "wasm32")]
            daily_mode: unsafe { hcg_is_daily_mode() != 0 },
            #[cfg(not(target_arch = "wasm32"))]
            daily_mode: false,
        }
    }

    /// True under the page's `?stream=1` query param (see
    /// `xtask::stream_mode_query_bridge`) — an OBS/Twitch browser-source layer has no
    /// visitor around to show score/speed HUD text to. Read once at startup (the query
    /// string doesn't change mid-session); always `false` natively, since there's no
    /// browser URL to read. Games should skip their own in-canvas HUD draw calls when
    /// this is true — `Control` only carries the flag, it doesn't touch rendering itself.
    pub fn stream_mode(&self) -> bool {
        self.stream_mode
    }

    /// True under the page's `?daily=1` query param (see `xtask::daily_mode_query_bridge`
    /// / `xtask::daily_challenge_button`) — a visitor asked for today's shared-seed board
    /// instead of a random one. Read once at startup, same as `stream_mode`. Games should
    /// seed their RNG with `control.seed()` instead of `screenshot::seed()`, and freeze on
    /// episode end (calling `share_result`) instead of starting a new episode, when this
    /// is true.
    pub fn daily_mode(&self) -> bool {
        self.daily_mode
    }

    /// RNG seed for this run. `HCG_SEED` (native testing override) wins if set — same
    /// precedence as `screenshot::seed()`. Otherwise, when `daily_mode()` is on, every
    /// visitor loading the page on the same UTC day gets the identical hash of
    /// `today_day_index()` (splitmix64 — cheap, well-distributed), so they all see the
    /// same board: the mechanic behind Wordle-style daily-return puzzles (see
    /// `.notes/aiideas.md`). Falls back to wall-clock for a normal, non-daily run — the
    /// same fallback `screenshot::seed()` uses, duplicated here (rather than adding a
    /// dependency on that crate for three lines) since seeding is a `Control`-level
    /// concern once daily mode is involved: `Control` is what owns `daily_mode` itself,
    /// so it should own the seed decision built on top of it rather than splitting that
    /// decision across two crates.
    pub fn seed(&self) -> u64 {
        if let Some(n) = std::env::var("HCG_SEED").ok().and_then(|s| s.parse().ok()) {
            return n;
        }
        if !self.daily_mode {
            return macroquad::miniquad::date::now() as u64;
        }
        let mut x = today_day_index().wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^ (x >> 31)
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
    /// round's final `score`, as a GA event. `daily` is tagged on from `self.daily_mode`
    /// so a completed daily-challenge run (vs. a regular random one) is filterable in GA
    /// without a second event — see `daily_mode` for why there's no separate
    /// "daily run started" event: `Control::new()` doesn't know the game's name yet, and
    /// `daily_challenge_button`'s own click already fires `daily_challenge_click` for the
    /// entering-daily-mode signal instead.
    pub fn episode_complete(&mut self, game: &str, score: i64) {
        self.episode += 1;
        ga_event(
            "episode_complete",
            &format!(
                "{{\"game\":\"{game}\",\"episode\":{},\"score\":{score},\"daily\":{}}}",
                self.episode, self.daily_mode
            ),
        );
    }
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}
