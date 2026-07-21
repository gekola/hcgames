//! Generates dist/<name>/index.html for a game.
use maud::{DOCTYPE, html};
use std::path::Path;
use xtask::{base_url, description, favicon_links, gtag_head, social_image, title};

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
                (gtag_head())
                style {
                    "* { margin: 0; padding: 0; box-sizing: border-box; }\n"
                    "body { background: #000; overflow: hidden; }\n"
                    "canvas { display: block; width: 100vw; height: 100vh; }"
                }
            }
            body {
                canvas id="glcanvas" tabindex="1" {}
                script src="mq_js_bundle.js" {}
                script { (maud::PreEscaped(format!("load(\"{name}.wasm\");"))) }
            }
        }
    };

    let dir = dist.join(&name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), page.into_string()).unwrap();
}
