use maud::{Markup, PreEscaped, html};
use std::path::Path;

/// `GITHUB_REPOSITORY` ("owner/repo") is auto-set by GitHub Actions; matches the default
/// project-pages URL when no custom domain (CNAME) is set. `BASE_URL` always overrides.
pub fn base_url() -> String {
    if let Ok(url) = std::env::var("BASE_URL") {
        return url;
    }
    if let Ok(repo) = std::env::var("GITHUB_REPOSITORY")
        && let Some((owner, name)) = repo.split_once('/')
    {
        return format!("https://{owner}.github.io/{name}/");
    }
    "http://localhost:8080/".to_owned() // matches `mise run serve`
}

/// A game's on-disk directory name ("game2048", "arrow-blocks") to a display title
/// ("2048", "Arrow Blocks"), matching the old Python `removeprefix("game").title()`.
pub fn title(name: &str) -> String {
    name.strip_prefix("game")
        .unwrap_or(name)
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each game's fixed logical canvas resolution (its `window_width`/`window_height` in
/// `Conf`). Games draw at absolute pixel coordinates rather than scaling to
/// `screen_width()`/`screen_height()`, so the canvas must stay at exactly this size —
/// see `native_size_style`.
pub fn native_size(name: &str) -> (u32, u32) {
    match name {
        "game2048" => (500, 610),
        _ => (900, 720),
    }
}

/// Upper bound on the viewport-fit scale factor (see `native_size_style`). Without a
/// cap, a portrait-ish canvas like game2048's (500x610) gets magnified more than a
/// landscape one on a widescreen monitor, since the fit is height-limited there — and
/// because portrait height (610) is close to a landscape game's already-capped on-screen
/// height, *any* single cap that isn't quite low still leaves it filling ~85-90% of the
/// viewport height on common desktop resolutions (verified: 1.5 barely changed anything
/// at 1600x1000 or 1920x1080). game2048 gets its own tighter 1.0 cap — never upscaled
/// past its native resolution at all, so it's always pixel-perfect regardless of
/// `high_dpi`/CSS-transform interaction, and reads as a modest fixed-size window rather
/// than something that grows to dominate the screen. The 900x720 games keep the looser
/// 1.5, which matched their existing on-screen size on a typical 1080p display, since
/// there's no report of them looking oversized.
fn max_fit_scale(name: &str) -> f64 {
    match name {
        "game2048" => 1.0,
        _ => 1.5,
    }
}

/// CSS + JS that pins the canvas to its native design resolution (so games drawing at
/// absolute pixel coordinates render correctly) and scales it uniformly to fit the
/// viewport via `transform: scale`, letterboxed and centered. A CSS transform doesn't
/// change `clientWidth`/`clientHeight`, so `mq_js_bundle.js`'s resize handling (which
/// syncs the canvas's backing resolution to its CSS box) never sees a mismatch.
///
/// `?stream=1` (see `stream_mode_class_script`) swaps the opaque black `html`/`body`
/// background for transparent instead — for dropping the page into OBS/Twitch as a
/// browser-source layer over other scene content. The letterboxed area around the
/// (still fixed-size, never stretched — see CLAUDE.md's "Canvas sizing is load-bearing")
/// canvas just becomes see-through padding rather than black bars.
pub fn native_size_style(name: &str) -> Markup {
    let (w, h) = native_size(name);
    let max_scale = max_fit_scale(name);
    html! {
        style {
            (PreEscaped(format!(
                "* {{ margin: 0; padding: 0; box-sizing: border-box; }}\n\
                 html, body {{ height: 100%; overflow: hidden; background: #000; }}\n\
                 html.stream-mode, html.stream-mode body {{ background: transparent; }}\n\
                 body {{ display: flex; align-items: center; justify-content: center; }}\n\
                 main {{ display: grid; }}\n\
                 canvas, .loading {{ grid-area: 1 / 1; width: {w}px; height: {h}px; transform-origin: center; }}\n\
                 canvas {{ display: block; outline: none; visibility: hidden; \
                 animation: reveal-canvas 0s 250ms forwards; }}\n\
                 @keyframes reveal-canvas {{ to {{ visibility: visible; }} }}\n\
                 .loading {{ display: flex; align-items: center; justify-content: center; text-align: center; \
                 padding: 0 2rem; color: rgba(255, 255, 255, 0.35); font: italic 15px system-ui, sans-serif; \
                 pointer-events: none; }}\n\
                 {POPUP_CSS}"
            )))
        }
        (stream_mode_class_script())
        script {
            (PreEscaped(format!(
                "function fitCanvas() {{\n\
                 \x20 const k = Math.min(window.innerWidth / {w}, window.innerHeight / {h}, {max_scale});\n\
                 \x20 document.querySelectorAll('canvas, .loading').forEach(function(el) {{\n\
                 \x20   el.style.transform = `scale(${{k}})`;\n\
                 \x20 }});\n\
                 }}\n\
                 window.addEventListener('resize', fitCanvas);\n\
                 document.addEventListener('DOMContentLoaded', fitCanvas);"
            )))
        }
    }
}

/// Adds the `stream-mode` class to `<html>` under `?stream=1`, for `native_size_style`'s
/// transparent-background rule to key off. A synchronous script (not deferred to
/// `DOMContentLoaded`) so the class lands before first paint — `document.documentElement`
/// already exists as soon as the parser reaches the `<html>` start tag, well before
/// `<body>`/the canvas/the WASM fetch.
fn stream_mode_class_script() -> Markup {
    html! {
        script {
            (PreEscaped(
                "if (new URLSearchParams(location.search).get('stream') === '1') {\n\
                 \x20 document.documentElement.classList.add('stream-mode');\n\
                 }"
            ))
        }
    }
}

/// True under either query param that asks a per-game page to hide chrome meant for a
/// human visitor — `?embed=1` (the ambient wall, `generate_index`'s `wall_page`, tiles
/// too small for a 48px popup button to make sense on) or `?stream=1` (an OBS/Twitch
/// browser-source layer, where the same button would show up on stream for no reason).
/// Shared by `hotkey_popup` and `orientation_hint`, the two things that check it.
const HIDE_CHROME_JS: &str = "(function() {\n\
     \x20 var qs = new URLSearchParams(location.search);\n\
     \x20 return qs.get('embed') === '1' || qs.get('stream') === '1';\n\
     }())";

/// Sarcastic one-liner shown behind the canvas while the WASM module fetches/inits.
/// Same "watch, don't judge" tone as the homepage quotes. Sits in the same CSS grid
/// cell as the canvas (see `native_size_style`) so once the game starts clearing the
/// canvas each frame, it's painted over automatically — no JS needed to hide it. The
/// canvas is also `visibility: hidden` for its first 250ms (`native_size_style`'s
/// `reveal-canvas` animation-delay) so a fast-loading/cached WASM module can't paint
/// over the line before it's had a chance to be read. This also gives the page a real
/// LCP-eligible text node: `<canvas>` itself isn't a valid Largest Contentful Paint
/// candidate per spec, so a canvas-only body reports NO_LCP.
const LOADING_LINES: &[&str] = &[
    "Make yourself comfortable. We'll start soon.",
    "The AI is warming up. You are not required to do anything.",
    "Loading. Try not to backseat drive.",
    "Spinning up the AI. It already knows how this ends.",
    "Nothing to do here. That's the whole point.",
    "The AI's already won. Now watch the AI beat the game.",
    "The AI's just better at this. Relax and watch.",
    "Sit down. The AI's got this handled.",
    "Nice game you have here. Sure you're okay watching the AI play?",
    "Your controller's decorative today.",
    "This one's on the AI. You just enjoy the show.",
    "The AI will take over from here.",
    "Hands off. Your help won't be needed.",
    "This game's just better when the AI plays it.",
];

pub fn loading_screen() -> Markup {
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0)
        % LOADING_LINES.len();
    html! {
        div class="loading" { (LOADING_LINES[idx]) }
    }
}

const POPUP_CSS: &str = "\
#hotkeys-btn { display: block; position: fixed; bottom: 14px; right: 14px; z-index: 10; \
width: 48px; height: 48px; border-radius: 50%; border: none; \
background: rgba(255,255,255,0.15); color: #fff; font: 20px system-ui, sans-serif; \
line-height: 48px; text-align: center; padding: 0; cursor: pointer; }\n\
#hotkeys-btn:hover { background: rgba(255,255,255,0.28); }\n\
#hotkeys { display: none; position: fixed; inset: 0; z-index: 11; \
background: rgba(0,0,0,0.75); align-items: center; justify-content: center; \
font-family: system-ui, sans-serif; }\n\
#hotkeys.open { display: flex; }\n\
#hotkeys .panel { position: relative; background: #1a1a1f; color: #eee; \
border-radius: 8px; padding: 20px 28px; min-width: 220px; }\n\
#hotkeys h2 { font-size: 16px; margin-bottom: 12px; }\n\
#hotkeys dl { display: grid; grid-template-columns: auto 1fr; gap: 4px 16px; \
font-size: 14px; margin: 0; }\n\
#hotkeys dt { font-family: monospace; color: #8cf; }\n\
#hotkeys dd { margin: 0; color: #ccc; }\n\
#hotkeys .stream-hint { margin: 14px 0 0; padding-top: 12px; \
border-top: 1px solid rgba(255,255,255,0.12); font-size: 12px; color: #999; \
line-height: 1.5; }\n\
#hotkeys .stream-hint a { color: #8cf; }\n\
#hotkeys-close { position: absolute; top: 8px; right: 8px; width: 32px; height: 32px; \
border-radius: 50%; border: none; background: rgba(255,255,255,0.1); color: #eee; \
font-size: 18px; line-height: 32px; text-align: center; padding: 0; cursor: pointer; }\n\
#hotkeys-close:hover { background: rgba(255,255,255,0.2); }";

/// The `?`-toggled, Esc-closed hotkey reference overlay, plus two touch-reachable
/// controls for it — an always-visible on-canvas `#hotkeys-btn` that opens it, and an
/// `#hotkeys-close` `×` in the panel's corner that closes it, since neither `?` nor Esc
/// is reachable without a physical keyboard. Both 32-48px (min. touch-target size) so
/// they're tappable without zooming. Pure HTML/CSS/JS — sits on top of the canvas rather
/// than being drawn by the game itself. Hotkeys listed here must match what
/// `control::Control` actually reads (`=`/`-`/`0`/`Space`/`F`), plus any per-game hotkey
/// the game's own `main.rs` reads directly (e.g. `V`). Hidden instead under `?embed=1`/
/// `?stream=1` (see `HIDE_CHROME_JS`) — a 48px popup button is visual clutter on an
/// ambient-wall tile or an OBS/Twitch browser-source layer. The panel also carries a
/// `?stream=1` link — otherwise stream mode is an undocumented URL param nobody would
/// ever find — so a streamer can turn it on from the same place they'd already look for
/// controls, without needing to know the query param exists ahead of time; clicking it
/// reloads into stream mode immediately, which also doubles as a live preview before they
/// copy the URL into OBS.
pub fn hotkey_popup(name: &str) -> Markup {
    let has_variant_switch = matches!(
        name,
        "klondike" | "spider" | "sudoku" | "minesweeper" | "tetris"
    );
    html! {
        button id="hotkeys-btn" aria-label="Show hotkeys" { "?" }
        div id="hotkeys" {
            div class="panel" {
                button id="hotkeys-close" aria-label="Close hotkeys" { "×" }
                h2 { "Hotkeys" }
                dl {
                    dt { "=" } dd { "speed up" }
                    dt { "-" } dd { "slow down" }
                    dt { "0" } dd { "reset speed" }
                    dt { "Space" } dd { "pause / resume" }
                    dt { "F" } dd { "toggle fullscreen (or double-click)" }
                    @if has_variant_switch {
                        dt { "V" } dd { "switch game variant" }
                    }
                    dt { "S" } dd { "save screenshot" }
                    dt { "2-finger slide" } dd { "adjust speed (touch)" }
                    @if has_variant_switch {
                        dt { "swipe" } dd { "switch game variant (touch)" }
                    }
                    dt { "?" } dd { "toggle this help (or tap the button)" }
                    dt { "Esc" } dd { "close (or tap ×)" }
                }
                p class="stream-hint" {
                    "🎥 Streaming? " a href="?stream=1" { "Open in stream mode" }
                    " for a clean, transparent OBS/Twitch layer — no HUD, no popup, see-through background."
                }
            }
        }
        script {
            (PreEscaped(format!(
                "if ({HIDE_CHROME_JS}) {{\n\
                 \x20 document.getElementById('hotkeys-btn').style.display = 'none';\n\
                 }} else {{\n\
                 \x20 document.addEventListener('keydown', function(e) {{\n\
                 \x20   if (e.key === '?') document.getElementById('hotkeys').classList.toggle('open');\n\
                 \x20   else if (e.key === 'Escape') document.getElementById('hotkeys').classList.remove('open');\n\
                 \x20 }});\n\
                 \x20 document.getElementById('hotkeys-btn').addEventListener('click', function() {{\n\
                 \x20   document.getElementById('hotkeys').classList.toggle('open');\n\
                 \x20 }});\n\
                 \x20 document.getElementById('hotkeys-close').addEventListener('click', function() {{\n\
                 \x20   document.getElementById('hotkeys').classList.remove('open');\n\
                 \x20 }});\n\
                 }}"
            )))
        }
    }
}

/// A dismissible banner nudging a visitor to rotate their device when the viewport's
/// orientation doesn't match this game's native one (e.g. a 900x720 landscape game
/// opened on a portrait phone) — the `fitCanvas` scale-to-fit in `native_size_style`
/// already handles this case technically (it just shrinks the canvas further to fit),
/// but on a badly-mismatched orientation that can leave the game a small fraction of the
/// screen. Pure page-level HTML/CSS/JS, same pattern as `hotkey_popup`/`screenshot_bridge`.
/// Dismissal is per-`sessionStorage` (not persisted across visits) so it can nudge again
/// next session rather than being silenced forever after one tap. Never shown at all
/// under `?embed=1`/`?stream=1` (see `HIDE_CHROME_JS`) — a tiny ambient-wall iframe
/// tile's own viewport dimensions are a meaningless orientation signal, and an OBS/Twitch
/// browser-source layer has no visitor around to rotate anything for either way.
pub fn orientation_hint(name: &str) -> Markup {
    let (w, h) = native_size(name);
    let game_is_landscape = w > h;
    let dismiss_key = format!("hcg-rotate-dismissed-{name}");
    html! {
        style {
            (PreEscaped(
                "#rotate-hint { display: none; position: fixed; top: 0; left: 0; right: 0; \
                 z-index: 12; background: rgba(20,20,24,0.92); color: #fff; \
                 font: 14px system-ui, sans-serif; padding: 10px 16px; \
                 align-items: center; justify-content: center; gap: 12px; text-align: center; }\n\
                 #rotate-hint.show { display: flex; }\n\
                 #rotate-hint button { background: rgba(255,255,255,0.15); color: #fff; \
                 border: none; border-radius: 50%; width: 28px; height: 28px; \
                 font-size: 16px; line-height: 28px; padding: 0; cursor: pointer; flex: none; }"
            ))
        }
        div id="rotate-hint" {
            span { "🔄 Rotate your device for a bigger screen" }
            button id="rotate-hint-close" aria-label="Dismiss" { "×" }
        }
        script {
            (PreEscaped(format!(
                "(function() {{\n\
                 \x20 if ({HIDE_CHROME_JS}) return;\n\
                 \x20 var key = '{dismiss_key}';\n\
                 \x20 var gameIsLandscape = {game_is_landscape};\n\
                 \x20 var el = document.getElementById('rotate-hint');\n\
                 \x20 function check() {{\n\
                 \x20   if (sessionStorage.getItem(key)) {{ el.classList.remove('show'); return; }}\n\
                 \x20   var viewportIsPortrait = window.innerHeight > window.innerWidth;\n\
                 \x20   el.classList.toggle('show', viewportIsPortrait === gameIsLandscape);\n\
                 \x20 }}\n\
                 \x20 window.addEventListener('resize', check);\n\
                 \x20 window.addEventListener('orientationchange', check);\n\
                 \x20 document.getElementById('rotate-hint-close').addEventListener('click', function() {{\n\
                 \x20   sessionStorage.setItem(key, '1');\n\
                 \x20   el.classList.remove('show');\n\
                 \x20 }});\n\
                 \x20 check();\n\
                 }})();"
            )))
        }
    }
}

/// Registers a miniquad plugin exposing `env.hcg_ga_event` to the wasm module, so
/// `control::Control::episode_complete` can fire `gtag('event', ...)` calls from Rust.
/// Must run after `mq_js_bundle.js` (needs its global `miniquad_add_plugin`/`UTF8ToString`)
/// but before `load(...)` (plugins register into the import object at instantiation time).
/// A no-op when `window.gtag` isn't defined (GTAG_ID unset locally).
pub fn analytics_bridge() -> Markup {
    html! {
        script {
            (PreEscaped(
                "miniquad_add_plugin({\n\
                 \x20 register_plugin: function(importObject) {\n\
                 \x20   importObject.env.hcg_ga_event = function(namePtr, nameLen, paramsPtr, paramsLen) {\n\
                 \x20     var name = UTF8ToString(namePtr, nameLen);\n\
                 \x20     var params = paramsLen > 0 ? JSON.parse(UTF8ToString(paramsPtr, paramsLen)) : {};\n\
                 \x20     if (window.gtag) window.gtag('event', name, params);\n\
                 \x20   };\n\
                 \x20 },\n\
                 \x20 version: 1,\n\
                 \x20 name: \"hcg_analytics\"\n\
                 });"
            ))
        }
    }
}

/// Registers a miniquad plugin exposing `env.hcg_initial_variant_is_hex`, letting the
/// wasm module read the page's `?variant=hex` query param at startup — used by the
/// `/minesweeper-hex` redirect stub (`static/minesweeper-hex/index.html`) so it lands
/// directly in Hex mode instead of the Square default. Must run before `load(...)`, same
/// ordering constraint as `analytics_bridge`.
pub fn variant_query_bridge() -> Markup {
    html! {
        script {
            (PreEscaped(
                "miniquad_add_plugin({\n\
                 \x20 register_plugin: function(importObject) {\n\
                 \x20   importObject.env.hcg_initial_variant_is_hex = function() {\n\
                 \x20     return new URLSearchParams(location.search).get('variant') === 'hex' ? 1 : 0;\n\
                 \x20   };\n\
                 \x20 },\n\
                 \x20 version: 1,\n\
                 \x20 name: \"hcg_variant_query\"\n\
                 });"
            ))
        }
    }
}

/// Registers a miniquad plugin exposing `env.hcg_is_stream_mode`, letting
/// `control::Control::stream_mode()` read the page's `?stream=1` query param at startup
/// so a game can skip drawing its own in-canvas HUD (score, speed label) for an OBS/Twitch
/// browser-source layer. Must run before `load(...)`, same ordering constraint as
/// `analytics_bridge`/`variant_query_bridge`. Registered unconditionally for every game
/// (unlike `variant_query_bridge`, which only minesweeper needs) since every game has a
/// HUD worth hiding.
pub fn stream_mode_query_bridge() -> Markup {
    html! {
        script {
            (PreEscaped(
                "miniquad_add_plugin({\n\
                 \x20 register_plugin: function(importObject) {\n\
                 \x20   importObject.env.hcg_is_stream_mode = function() {\n\
                 \x20     return new URLSearchParams(location.search).get('stream') === '1' ? 1 : 0;\n\
                 \x20   };\n\
                 \x20 },\n\
                 \x20 version: 1,\n\
                 \x20 name: \"hcg_stream_mode\"\n\
                 });"
            ))
        }
    }
}

/// `S` hotkey: grabs the current frame straight off the canvas (`toBlob`, no Rust
/// involvement — WASM has no filesystem, so `screenshot::handle_hotkey` is a native-only
/// no-op) and prompts the browser's own download flow for it.
pub fn screenshot_bridge(name: &str) -> Markup {
    html! {
        script {
            (PreEscaped(format!(
                "document.addEventListener('keydown', function(e) {{\n\
                 \x20 if (e.key !== 's' && e.key !== 'S') return;\n\
                 \x20 document.querySelector('canvas').toBlob(function(blob) {{\n\
                 \x20   var url = URL.createObjectURL(blob);\n\
                 \x20   var a = document.createElement('a');\n\
                 \x20   a.href = url;\n\
                 \x20   a.download = '{name}-screenshot.png';\n\
                 \x20   a.click();\n\
                 \x20   URL.revokeObjectURL(url);\n\
                 \x20 }});\n\
                 }});"
            )))
        }
    }
}

/// `F` or double-click/double-tap toggles fullscreen. Pure page-level JS rather than
/// `macroquad::window::set_fullscreen` (which on WASM calls `canvas.requestFullscreen()`
/// via `mq_js_bundle.js`) — browsers apply `:fullscreen { width: 100%; height: 100% }` as
/// a `!important` UA style, which cannot be overridden from author CSS at any specificity,
/// so fullscreening the canvas directly stomps the pinned native-resolution box
/// `native_size_style` relies on and leaves the game drawing at fixed pixel coordinates
/// into a canvas that's now some unrelated size — the visible symptom was drawing that
/// read as "shrunk"/"downscaled" once fullscreen, and on some pages looked like nothing
/// had happened at all. Fullscreening `<html>` instead leaves the canvas element itself
/// completely unconstrained by the UA style, so its own pinned size and `fitCanvas()`
/// scale-to-fit transform keep working unchanged — just re-evaluated against a bigger
/// viewport, which is also why `fullscreenchange` re-runs `fitCanvas` (the transition
/// doesn't reliably fire its own `resize` event). Native builds toggle fullscreen from
/// `control::Control` instead, straight through `macroquad::window::set_fullscreen` — no
/// DOM/canvas-target issue there since it's a real OS window, not a styled element.
///
/// The touch path is its own listener rather than relying on `dblclick` firing from a
/// double-tap: iOS Safari treats a double-tap on a plain (non-form, non-link) element as
/// its own double-tap-to-zoom gesture and consumes it before a `dblclick` event ever
/// reaches JS, so double-tap silently did nothing there. Detecting two `touchend`s within
/// 350ms ourselves and calling `preventDefault()` on the second suppresses that native
/// zoom and fires the toggle instead — scoped to the canvas only, so pinch-zoom/
/// double-tap-zoom elsewhere on the page (e.g. the hotkey popup text) is untouched, which
/// matters since disabling zoom via the viewport meta tag site-wide would be an
/// accessibility regression. `webkitRequestFullscreen`/`webkitExitFullscreen`/
/// `webkitFullscreenElement` fall back for older WebKit that predates the unprefixed API.
pub fn fullscreen_bridge() -> Markup {
    html! {
        script {
            (PreEscaped(
                "function hcgIsFullscreen() {\n\
                 \x20 return !!(document.fullscreenElement || document.webkitFullscreenElement);\n\
                 }\n\
                 function hcgToggleFullscreen() {\n\
                 \x20 if (hcgIsFullscreen()) {\n\
                 \x20   (document.exitFullscreen || document.webkitExitFullscreen).call(document);\n\
                 \x20 } else {\n\
                 \x20   var el = document.documentElement;\n\
                 \x20   (el.requestFullscreen || el.webkitRequestFullscreen).call(el);\n\
                 \x20 }\n\
                 }\n\
                 document.addEventListener('keydown', function(e) {\n\
                 \x20 if (e.key === 'f' || e.key === 'F') hcgToggleFullscreen();\n\
                 });\n\
                 var hcgCanvas = document.querySelector('canvas');\n\
                 hcgCanvas.addEventListener('dblclick', hcgToggleFullscreen);\n\
                 var hcgLastTouchEnd = 0;\n\
                 hcgCanvas.addEventListener('touchend', function(e) {\n\
                 \x20 var now = Date.now();\n\
                 \x20 if (now - hcgLastTouchEnd <= 350) {\n\
                 \x20   e.preventDefault();\n\
                 \x20   hcgToggleFullscreen();\n\
                 \x20 }\n\
                 \x20 hcgLastTouchEnd = now;\n\
                 }, { passive: false });\n\
                 ['fullscreenchange', 'webkitfullscreenchange'].forEach(function(ev) {\n\
                 \x20 document.addEventListener(ev, function() {\n\
                 \x20   if (typeof fitCanvas === 'function') fitCanvas();\n\
                 \x20 });\n\
                 });"
            ))
        }
    }
}

pub fn description(name: &str) -> String {
    let title = title(name);
    match name {
        "snake" => "Watch an AI play Snake by itself. A pathfinding bot solves procedurally generated levels live in your browser.".into(),
        "game2048" => "A self-playing 2048 AI merges tiles with expectimax search, climbing toward the highest tile with no input from you.".into(),
        "klondike" => "Self-playing Klondike solitaire in your browser. Watch an AI deal, draw, and solve the classic card game automatically.".into(),
        "spider" => "Self-playing Spider solitaire. An AI clears all 10 columns automatically, cycling through 1-, 2-, and 4-suit variants each round.".into(),
        "sudoku" => "Self-playing Sudoku. Watch an AI fill in sure cells with logical deduction, showing its candidate notes, before falling back to a guess.".into(),
        "arrow-blocks" => "A browser puzzle game solved automatically by an AI, sliding arrow-marked blocks through procedurally generated levels.".into(),
        "minesweeper" => "AI-solved Minesweeper, played automatically in your browser. Cycle between square and hexagonal grids.".into(),
        "tetris" => "Self-playing Tetris. An AI scores every drop by height, holes, and bumpiness with a known-next-piece lookahead, cycling between 7-bag, classic NES-style, TGM, and pure-random piece generators.".into(),
        _ => format!("Watch an AI play {title} automatically in your browser."),
    }
}

/// Sets up the `dataLayer`/`gtag()` stub eagerly (so early `episode_complete` calls still
/// queue), but defers actually fetching `gtag.js` itself until the first user interaction
/// — it's ~67 KiB of mostly-unused-at-load JS that Lighthouse flags, and a self-playing
/// game doesn't need analytics wired before first paint. Queued `dataLayer` entries are
/// processed by `gtag.js` once it does load. A no-op when `GTAG_ID` is unset locally.
pub fn gtag_head() -> Markup {
    let Ok(gtag_id) = std::env::var("GTAG_ID") else {
        return html! {};
    };
    if gtag_id.is_empty() {
        return html! {};
    }
    html! {
        script {
            (PreEscaped(format!(
                "window.dataLayer = window.dataLayer || [];\n\
                 function gtag(){{dataLayer.push(arguments);}}\n\
                 gtag('js', new Date());\n\
                 gtag('config', '{gtag_id}');\n\
                 function hcgLoadGtag() {{\n\
                 \x20 var s = document.createElement('script');\n\
                 \x20 s.async = true;\n\
                 \x20 s.src = 'https://www.googletagmanager.com/gtag/js?id={gtag_id}';\n\
                 \x20 document.head.appendChild(s);\n\
                 \x20 ['pointerdown', 'keydown', 'touchstart', 'scroll'].forEach(function(e) {{\n\
                 \x20   document.removeEventListener(e, hcgLoadGtag);\n\
                 \x20 }});\n\
                 }}\n\
                 ['pointerdown', 'keydown', 'touchstart', 'scroll'].forEach(function(e) {{\n\
                 \x20 document.addEventListener(e, hcgLoadGtag, {{ once: true, passive: true }});\n\
                 }});",
            )))
        }
    }
}

/// `<link rel="icon">` tags: the SVG always, plus the rasterized PNG (see `mise run
/// rasterize`) when present. Falls back to SVG-only locally, where rasterization is
/// skipped without resvg.
pub fn favicon_links(base_url: &str, dist: &Path) -> Markup {
    let svg_url = format!("{base_url}favicon.svg");
    let has_png = dist.join("favicon.png").exists();
    html! {
        link rel="icon" href=(svg_url) type="image/svg+xml";
        @if has_png {
            link rel="icon" href=(format!("{base_url}favicon.png")) type="image/png" sizes="192x192";
        }
    }
}

/// `<link rel="manifest">` + `<meta name="theme-color">`, shared by every generated page
/// (homepage and each game). Each page writes its own `manifest.webmanifest` alongside
/// its `index.html` (see `manifest_json`) — a game installs as its own home-screen app,
/// separate from the homepage's, matching aiideas.md's "PWA / installable" idea.
pub fn pwa_head(theme_color: &str) -> Markup {
    html! {
        link rel="manifest" href="manifest.webmanifest";
        meta name="theme-color" content=(theme_color);
    }
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Builds a page's `manifest.webmanifest` contents. `start_url`/`scope` are both "./" —
/// each generated page (homepage, or a game's own `dist/<name>/`) is installed as an
/// independent app scoped to just that directory. `icon_dir` is the relative path back to
/// wherever `favicon.svg`/`favicon.png`/`icon-512.png`/`icon-512-maskable.png` live
/// (`dist/`'s root) — "" from the homepage itself, "../" from a game one level down. The
/// raster sizes are included only when present, same fallback pattern as
/// `favicon_links`/`social_image` (skipped locally without resvg). The maskable icon
/// (`static/favicon-maskable.svg` — safe-zone-padded + opaque background, unlike the
/// edge-to-edge `favicon.svg`) declares `"purpose":"maskable"` so Android/adaptive-icon
/// installs don't letterbox-crop the artwork — Lighthouse's `maskable-icon` PWA audit
/// checks for exactly this.
pub fn manifest_json(
    dist: &Path,
    title: &str,
    description: &str,
    theme_color: &str,
    icon_dir: &str,
) -> String {
    let mut icons = vec![format!(
        r#"{{"src":"{icon_dir}favicon.svg","sizes":"any","type":"image/svg+xml"}}"#
    )];
    if dist.join("favicon.png").exists() {
        icons.push(format!(
            r#"{{"src":"{icon_dir}favicon.png","sizes":"192x192","type":"image/png"}}"#
        ));
    }
    if dist.join("icon-512.png").exists() {
        icons.push(format!(
            r#"{{"src":"{icon_dir}icon-512.png","sizes":"512x512","type":"image/png"}}"#
        ));
    }
    if dist.join("icon-512-maskable.png").exists() {
        icons.push(format!(
            r#"{{"src":"{icon_dir}icon-512-maskable.png","sizes":"512x512","type":"image/png","purpose":"maskable"}}"#
        ));
    }
    format!(
        r##"{{"name":"{}","short_name":"{}","description":"{}","start_url":"./","scope":"./","display":"standalone","background_color":"#000000","theme_color":"{theme_color}","orientation":"any","icons":[{}]}}"##,
        json_escape(title),
        json_escape(title),
        json_escape(description),
        icons.join(",")
    )
}

/// Registers the shared `dist/sw.js` (see `static/sw.js`) for offline static-asset
/// caching. Deferred to `load` so it doesn't compete with the game's own WASM fetch for
/// bandwidth/priority on first visit. `sw_path` is relative to the page doing the
/// registering — "./sw.js" from the homepage, "../sw.js" from a game page. A service
/// worker's default scope (when not passed explicitly) is resolved from *its own* script
/// location, not the registering page's — since `sw.js` always lives at `dist/`'s root,
/// every page ends up registering the same single worker at the same site-wide scope,
/// regardless of which relative path reached it. That's fine here: caching is per-URL
/// inside the worker's fetch handler (see `static/sw.js`), so each app's own assets still
/// get cached and served offline independently even though one worker controls the whole
/// site.
pub fn sw_register_bridge(sw_path: &str) -> Markup {
    html! {
        script {
            (PreEscaped(format!(
                "if ('serviceWorker' in navigator) {{\n\
                 \x20 window.addEventListener('load', function() {{\n\
                 \x20   navigator.serviceWorker.register('{sw_path}');\n\
                 \x20 }});\n\
                 }}"
            )))
        }
    }
}

pub struct SocialImage {
    pub url: String,
    pub twitter_card: &'static str,
}

/// Picks the best available image for `og:image`/`twitter:image`: a real in-game
/// screenshot (see `mise run screenshot`) beats the rasterized favicon, which beats the
/// bare favicon SVG that most crawlers won't render. All fall back locally, where those
/// build steps are skipped without xvfb-run/resvg.
pub fn social_image(base_url: &str, dist: &Path, preview: Option<&str>) -> SocialImage {
    if let Some(preview) = preview
        && dist.join(preview).exists()
    {
        return SocialImage {
            url: format!("{base_url}{preview}"),
            twitter_card: "summary_large_image",
        };
    }
    if dist.join("favicon.png").exists() {
        return SocialImage {
            url: format!("{base_url}favicon.png"),
            twitter_card: "summary",
        };
    }
    SocialImage {
        url: format!("{base_url}favicon.svg"),
        twitter_card: "summary",
    }
}
