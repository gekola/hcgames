mod game;
mod puzzle;

use macroquad::prelude::*;

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
    let mut debug = false;
    let mut once = false;
    let mut no_ui = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--debug" => debug = true,
            "--once" => once = true,
            "--no-ui" => no_ui = true,
            other => {
                eprintln!("unknown argument '{other}' (expected --debug, --once, --no-ui)");
                std::process::exit(2);
            }
        }
    }

    CliArgs { debug, once, no_ui }
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

        // Field border
        let fx = sw * 0.5 - cam_px;
        let fy = sh * 0.5 - cam_py + 15.0;
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

        // HUD
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

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}
