//! Generates dist/<name>/index.html for a game.
use maud::{DOCTYPE, html};
use std::path::Path;
use xtask::{
    analytics_bridge, base_url, description, favicon_links, gtag_head, hotkey_popup,
    native_size_style, screenshot_bridge, social_image, title,
};

fn main() {
    let name = std::env::args().nth(1).expect("usage: generate_game_html <name>");
    let dist = Path::new("dist");

    let base_url = base_url();
    let title = title(&name);
    let description = description(&name);
    let page_url = format!("{base_url}{name}/");
    let og = social_image(&base_url, dist, Some(&format!("{name}/preview.png")));

    let page = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1, user-scalable=no";
                (favicon_links(&base_url, dist))
                title { (title) " — Hotel Chair Games" }
                meta name="description" content=(description);
                link rel="canonical" href=(page_url);
                meta property="og:type" content="website";
                meta property="og:title" content=(format!("{title} — Hotel Chair Games"));
                meta property="og:description" content=(description);
                meta property="og:url" content=(page_url);
                meta property="og:image" content=(og.url);
                meta name="twitter:card" content=(og.twitter_card);
                meta name="twitter:image" content=(og.url);
                link rel="preload" href=(format!("{name}.wasm")) as="fetch" crossorigin="anonymous";
                (gtag_head())
                (native_size_style(&name))
            }
            body {
                main {
                    canvas id="glcanvas" tabindex="1" {}
                }
                script src="../mq_js_bundle.js" {}
                (analytics_bridge())
                script { (maud::PreEscaped(format!("load(\"{name}.wasm\");"))) }
                (hotkey_popup(&name))
                (screenshot_bridge(&name))
            }
        }
    };

    let dir = dist.join(&name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), page.into_string()).unwrap();
}
