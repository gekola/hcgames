use maud::{Markup, PreEscaped, html};
use std::path::Path;

/// `GITHUB_REPOSITORY` ("owner/repo") is auto-set by GitHub Actions; matches the default
/// project-pages URL when no custom domain (CNAME) is set. `BASE_URL` always overrides.
pub fn base_url() -> String {
    if let Ok(url) = std::env::var("BASE_URL") {
        return url;
    }
    if let Ok(repo) = std::env::var("GITHUB_REPOSITORY") {
        if let Some((owner, name)) = repo.split_once('/') {
            return format!("https://{owner}.github.io/{name}/");
        }
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
                 canvas {{ display: block; width: {w}px; height: {h}px; transform-origin: center; outline: none; }}"
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

pub fn description(name: &str) -> String {
    let title = title(name);
    match name {
        "snake" => "Watch an AI play Snake by itself. A pathfinding bot solves procedurally generated levels live in your browser.".into(),
        "game2048" => "A self-playing 2048 AI merges tiles with expectimax search, climbing toward the highest tile with no input from you.".into(),
        "klondike" => "Self-playing Klondike solitaire in your browser. Watch an AI deal, draw, and solve the classic card game automatically.".into(),
        "arrow-blocks" => "A browser puzzle game solved automatically by an AI, sliding arrow-marked blocks through procedurally generated levels.".into(),
        "minesweeper-hex" => "AI-solved Minesweeper on a hexagonal grid. Watch the bot flag mines and clear the board in your browser.".into(),
        "minesweeper-square" => "AI-solved classic square-grid Minesweeper, played automatically in your browser.".into(),
        _ => format!("Watch an AI play {title} automatically in your browser."),
    }
}

/// `<script>` tags loading + configuring Google Analytics, or empty markup when `GTAG_ID` is unset.
pub fn gtag_head() -> Markup {
    let Ok(gtag_id) = std::env::var("GTAG_ID") else {
        return html! {};
    };
    if gtag_id.is_empty() {
        return html! {};
    }
    html! {
        script async src=(format!("https://www.googletagmanager.com/gtag/js?id={gtag_id}")) {}
        script {
            (PreEscaped(format!(
                "window.dataLayer = window.dataLayer || [];\n\
                 function gtag(){{dataLayer.push(arguments);}}\n\
                 gtag('js', new Date());\n\
                 gtag('config', '{gtag_id}');",
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
    if let Some(preview) = preview {
        if dist.join(preview).exists() {
            return SocialImage {
                url: format!("{base_url}{preview}"),
                twitter_card: "summary_large_image",
            };
        }
    }
    if dist.join("favicon.png").exists() {
        return SocialImage { url: format!("{base_url}favicon.png"), twitter_card: "summary" };
    }
    SocialImage { url: format!("{base_url}favicon.svg"), twitter_card: "summary" }
}
