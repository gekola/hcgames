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

/// CSS + JS that pins the canvas to its native design resolution (so games drawing at
/// absolute pixel coordinates render correctly) and scales it uniformly to fit the
/// viewport via `transform: scale`, letterboxed and centered. A CSS transform doesn't
/// change `clientWidth`/`clientHeight`, so `mq_js_bundle.js`'s resize handling (which
/// syncs the canvas's backing resolution to its CSS box) never sees a mismatch.
pub fn native_size_style(name: &str) -> Markup {
    let (w, h) = native_size(name);
    html! {
        style {
            (PreEscaped(format!(
                "* {{ margin: 0; padding: 0; box-sizing: border-box; }}\n\
                 html, body {{ height: 100%; overflow: hidden; background: #000; }}\n\
                 body {{ display: flex; align-items: center; justify-content: center; }}\n\
                 canvas {{ display: block; width: {w}px; height: {h}px; transform-origin: center; outline: none; }}\n\
                 {POPUP_CSS}"
            )))
        }
        script {
            (PreEscaped(format!(
                "function fitCanvas() {{\n\
                 \x20 const k = Math.min(window.innerWidth / {w}, window.innerHeight / {h});\n\
                 \x20 document.querySelector('canvas').style.transform = `scale(${{k}})`;\n\
                 }}\n\
                 window.addEventListener('resize', fitCanvas);\n\
                 document.addEventListener('DOMContentLoaded', fitCanvas);"
            )))
        }
    }
}

const POPUP_CSS: &str = "\
#hotkeys { display: none; position: fixed; inset: 0; z-index: 10; \
background: rgba(0,0,0,0.75); align-items: center; justify-content: center; \
font-family: system-ui, sans-serif; }\n\
#hotkeys.open { display: flex; }\n\
#hotkeys .panel { background: #1a1a1f; color: #eee; border-radius: 8px; \
padding: 20px 28px; min-width: 220px; }\n\
#hotkeys h2 { font-size: 16px; margin-bottom: 12px; }\n\
#hotkeys dl { display: grid; grid-template-columns: auto 1fr; gap: 4px 16px; \
font-size: 14px; margin: 0; }\n\
#hotkeys dt { font-family: monospace; color: #8cf; }\n\
#hotkeys dd { margin: 0; color: #ccc; }";

/// The `?`-toggled, Esc-closed hotkey reference overlay. Pure HTML/CSS/JS — sits on top
/// of the canvas rather than being drawn by the game itself. Hotkeys listed here must
/// match what `control::Control` actually reads (`=`/`-`/`0`/`Space`), plus any
/// per-game hotkey the game's own `main.rs` reads directly (e.g. `V`).
pub fn hotkey_popup(name: &str) -> Markup {
    let has_variant_switch = matches!(name, "klondike" | "spider");
    html! {
        div id="hotkeys" {
            div class="panel" {
                h2 { "Hotkeys" }
                dl {
                    dt { "=" } dd { "speed up" }
                    dt { "-" } dd { "slow down" }
                    dt { "0" } dd { "reset speed" }
                    dt { "Space" } dd { "pause / resume" }
                    @if has_variant_switch {
                        dt { "V" } dd { "switch game variant" }
                    }
                    dt { "S" } dd { "save screenshot" }
                    dt { "?" } dd { "toggle this help" }
                    dt { "Esc" } dd { "close" }
                }
            }
        }
        script {
            (PreEscaped(
                "document.addEventListener('keydown', function(e) {\n\
                 \x20 if (e.key === '?') document.getElementById('hotkeys').classList.toggle('open');\n\
                 \x20 else if (e.key === 'Escape') document.getElementById('hotkeys').classList.remove('open');\n\
                 });"
            ))
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

pub fn description(name: &str) -> String {
    let title = title(name);
    match name {
        "snake" => "Watch an AI play Snake by itself. A pathfinding bot solves procedurally generated levels live in your browser.".into(),
        "game2048" => "A self-playing 2048 AI merges tiles with expectimax search, climbing toward the highest tile with no input from you.".into(),
        "klondike" => "Self-playing Klondike solitaire in your browser. Watch an AI deal, draw, and solve the classic card game automatically.".into(),
        "spider" => "Self-playing Spider solitaire. An AI clears all 10 columns automatically, cycling through 1-, 2-, and 4-suit variants each round.".into(),
        "arrow-blocks" => "A browser puzzle game solved automatically by an AI, sliding arrow-marked blocks through procedurally generated levels.".into(),
        "minesweeper-hex" => "AI-solved Minesweeper on a hexagonal grid. Watch the bot flag mines and clear the board in your browser.".into(),
        "minesweeper-square" => "AI-solved classic square-grid Minesweeper, played automatically in your browser.".into(),
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
