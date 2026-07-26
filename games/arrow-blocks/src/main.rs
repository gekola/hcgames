mod game;
mod puzzle;

use macroquad::prelude::*;
use render_cache::RenderCache;

pub const FIELD_W: i32 = 120;
pub const FIELD_H: i32 = 90;
pub const VIEW_COLS: i32 = 30;
pub const VIEW_ROWS: i32 = 22;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::Up => (0, -1),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
            Dir::Right => (1, 0),
        }
    }
}

fn conf() -> Conf {
    Conf {
        window_title: "Arrow Blocks".to_owned(),
        window_width: 900,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

fn draw_arrow(cx: f32, cy: f32, dir: Dir, size: f32, color: Color) {
    let (v1, v2, v3) = match dir {
        Dir::Up => (
            Vec2::new(cx, cy - size),
            Vec2::new(cx - size * 0.6, cy + size * 0.5),
            Vec2::new(cx + size * 0.6, cy + size * 0.5),
        ),
        Dir::Down => (
            Vec2::new(cx, cy + size),
            Vec2::new(cx - size * 0.6, cy - size * 0.5),
            Vec2::new(cx + size * 0.6, cy - size * 0.5),
        ),
        Dir::Left => (
            Vec2::new(cx - size, cy),
            Vec2::new(cx + size * 0.5, cy - size * 0.6),
            Vec2::new(cx + size * 0.5, cy + size * 0.6),
        ),
        Dir::Right => (
            Vec2::new(cx + size, cy),
            Vec2::new(cx - size * 0.5, cy - size * 0.6),
            Vec2::new(cx - size * 0.5, cy + size * 0.6),
        ),
    };
    draw_triangle(v1, v2, v3, color);
}

// ── CLI args (native only — meaningless in a browser tab) ───────────────────────

struct CliArgs {
    /// `--debug`: print each block that finishes exiting, plus level completions, to stderr.
    debug: bool,
    /// `--once`: solve one figure, print a result line, then exit instead of cycling
    /// through figures forever.
    once: bool,
    /// `--no-ui`: run with no window, no GL context, and no miniquad involvement at
    /// all (see `run_headless`).
    #[cfg(not(target_arch = "wasm32"))]
    no_ui: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_cli_args() -> CliArgs {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (base, rest) = game_common::parse_base_args(&args);
    if let Some(other) = rest.first() {
        eprintln!("unknown argument '{other}' (expected --debug, --once, --no-ui)");
        std::process::exit(2);
    }
    CliArgs {
        debug: base.debug,
        once: base.once,
        no_ui: base.no_ui,
    }
}

#[cfg(target_arch = "wasm32")]
fn parse_cli_args() -> CliArgs {
    CliArgs {
        debug: false,
        once: false,
    }
}

/// `game::Game::tick` is driven by real `dt`/`now` values (block-exit animation speed,
/// and the `Considering`/`Pause`/`Done` phase timers), not a discrete move list — so
/// headless mode drives a virtual clock forward by this fixed step each iteration
/// instead of reading `get_frame_time()`/`miniquad::date::now()`, matching the same
/// 0.05s cap the windowed loop applies to `dt`, just without waiting on real time
/// between iterations.
#[cfg(not(target_arch = "wasm32"))]
const HEADLESS_DT: f32 = 0.05;

/// Runs the game with no window, no GL context, and no miniquad involvement at all —
/// `game::Game` has no rendering dependency (`macroquad::rand` is a pure `no_std` PRNG,
/// safe to call standalone), and miniquad has no headless backend to opt into, so the
/// only way to guarantee zero window creation is to never call
/// `miniquad::start`/`Window::from_config` in the first place.
#[cfg(not(target_arch = "wasm32"))]
fn run_headless(cli: CliArgs) -> ! {
    rand::srand(screenshot::seed());
    let mut game = game::Game::new(0);
    let mut now: f64 = 0.0;
    let mut last_remaining = game.remaining();

    loop {
        now += HEADLESS_DT as f64;
        game.tick(HEADLESS_DT, now);

        let remaining = game.remaining();
        if cli.debug && remaining != last_remaining {
            eprintln!("block_exited remaining={remaining} level={}", game.level);
        }
        last_remaining = remaining;

        if let game::Phase::Done { since } = game.phase
            && now - since > 0.4
        {
            if cli.debug {
                eprintln!(
                    "level_complete level={} blocks={}",
                    game.level,
                    game.blocks.len()
                );
            }
            if cli.once {
                println!(
                    "result=solved level={} blocks={}",
                    game.level,
                    game.blocks.len()
                );
                std::process::exit(0);
            }
            let next = (game.level + 1) % puzzle::NFIGURES;
            game = game::Game::new(next);
            now = 0.0;
            last_remaining = game.remaining();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = parse_cli_args();
    if cli.no_ui {
        run_headless(cli);
    } else {
        macroquad::Window::from_config(conf(), amain(cli));
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    macroquad::Window::from_config(conf(), amain(parse_cli_args()));
}

async fn amain(cli: CliArgs) {
    rand::srand(screenshot::seed());
    let mut game = game::Game::new(0);
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();
    let mut last_remaining = game.remaining();

    // The field border + blocks only need redrawing while the camera is still
    // panning toward its target or a block is mid-animation (exiting/returning) —
    // once both settle, the view is static until the next AI action, but was being
    // redrawn every render frame regardless. See `render_cache::RenderCache`. Covers
    // the whole canvas (blocks can pan anywhere in view); rebuilt if the canvas
    // itself resizes (native window resize only — the deployed WASM canvas is
    // pinned to a fixed backing resolution).
    let mut cached_size = (screen_width(), screen_height());
    let mut board_cache = RenderCache::new(Rect::new(0.0, 0.0, cached_size.0, cached_size.1));

    loop {
        control.handle_keys();
        let dt = control.scale(get_frame_time().min(0.05));
        let now = macroquad::miniquad::date::now();

        game.tick(dt, now);

        let remaining = game.remaining();
        if cli.debug && remaining != last_remaining {
            eprintln!("block_exited remaining={remaining} level={}", game.level);
        }
        last_remaining = remaining;

        if let game::Phase::Done { since } = game.phase
            && now - since > 0.4
        {
            if cli.debug {
                eprintln!(
                    "level_complete level={} blocks={}",
                    game.level,
                    game.blocks.len()
                );
            }
            control.episode_complete("arrow-blocks", game.blocks.len() as i64);
            if cli.once {
                println!(
                    "result=solved level={} blocks={}",
                    game.level,
                    game.blocks.len()
                );
                std::process::exit(0);
            }
            let next = (game.level + 1) % puzzle::NFIGURES;
            game = game::Game::new(next);
            last_remaining = game.remaining();
            board_cache.mark_dirty();
        }

        // --- render ---
        let sw = screen_width();
        let sh = screen_height();
        let cell = (sw / VIEW_COLS as f32)
            .min((sh - 30.0) / VIEW_ROWS as f32)
            .floor()
            .max(1.0);

        // Camera is tracked in block-coordinate space; convert to pixels for draw
        let cam_px = game.cam_x * cell;
        let cam_py = game.cam_y * cell;

        clear_background(Color {
            r: 0.07,
            g: 0.07,
            b: 0.12,
            a: 1.0,
        });

        let cur_size = (sw, sh);
        if cur_size != cached_size {
            board_cache = RenderCache::new(Rect::new(0.0, 0.0, cur_size.0, cur_size.1));
            cached_size = cur_size;
        }

        // Field border
        let fx = sw * 0.5 - cam_px;
        let fy = sh * 0.5 - cam_py + 15.0;

        let draw_field = |game: &game::Game| {
            draw_rectangle_lines(
                fx,
                fy,
                FIELD_W as f32 * cell,
                FIELD_H as f32 * cell,
                2.0,
                Color {
                    r: 0.25,
                    g: 0.25,
                    b: 0.35,
                    a: 1.0,
                },
            );

            for block in &game.blocks {
                if block.state == game::BlockState::Gone {
                    continue;
                }

                let (ox, oy) = block.vis_offset(cell);
                let sx = sw * 0.5 + block.col as f32 * cell - cam_px + ox;
                let sy = sh * 0.5 + block.row as f32 * cell - cam_py + oy + 15.0;

                // Skip if fully off-screen
                if sx + cell < 0.0 || sx > sw || sy + cell < 0.0 || sy > sh {
                    continue;
                }

                let block_color = if block.state == game::BlockState::Considered {
                    Color {
                        r: 0.7,
                        g: 0.7,
                        b: 0.25,
                        a: 0.5,
                    }
                } else {
                    Color {
                        r: 0.2,
                        g: 0.75,
                        b: 0.65,
                        a: 1.0,
                    }
                };

                draw_rectangle(sx + 1.0, sy + 1.0, cell - 2.0, cell - 2.0, block_color);

                let arrow_color = Color {
                    r: 0.04,
                    g: 0.08,
                    b: 0.1,
                    a: 0.85,
                };
                draw_arrow(
                    sx + cell * 0.5,
                    sy + cell * 0.5,
                    block.dir,
                    cell * 0.28,
                    arrow_color,
                );
            }
        };

        // Cache-eligible only once the camera has stopped easing toward its target
        // and no block is mid-animation — otherwise every block's screen position
        // (baked from cam_px/cam_py at draw time) would go stale the instant the
        // camera moves on. Both are common (an AI action roughly every 0.2-0.5s), so
        // this doesn't win as much as the other games, but it's free when it applies.
        let cam_settled =
            (game.cam_x - game.cam_tx).abs() < 0.01 && (game.cam_y - game.cam_ty).abs() < 0.01;
        let all_idle = game
            .blocks
            .iter()
            .all(|b| matches!(b.state, game::BlockState::Idle | game::BlockState::Gone));
        if cam_settled && all_idle {
            board_cache.draw(|| draw_field(&game));
        } else {
            draw_field(&game);
            board_cache.mark_dirty();
        }

        // HUD
        if !control.stream_mode() {
            let font_size = 16.0f32.max(cell * 0.7);
            let remaining = game.remaining();
            let hud = format!(
                "Arrow Blocks   figure {}/{}   {} blocks",
                game.level + 1,
                puzzle::NFIGURES,
                remaining,
            );
            draw_text(
                &hud,
                fx + 4.0,
                20.0,
                font_size,
                Color {
                    r: 0.6,
                    g: 0.6,
                    b: 0.7,
                    a: 1.0,
                },
            );

            let speed_label = control.label();
            let sd = measure_text(&speed_label, None, font_size as u16, 1.0);
            draw_text(
                &speed_label,
                sw - 8.0 - sd.width,
                20.0,
                font_size,
                Color {
                    r: 0.6,
                    g: 0.6,
                    b: 0.7,
                    a: 1.0,
                },
            );
        }

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}
