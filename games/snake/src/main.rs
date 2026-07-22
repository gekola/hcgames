use macroquad::prelude::*;

mod blocks;
mod game;

use game::Game;

pub const COLS: i32 = 40;
pub const ROWS: i32 = 30;
pub const TICK: f32 = 0.08;
pub const GRID: usize = (COLS * ROWS) as usize;
pub const BLOCK_SENTINEL: u16 = u16::MAX - 1;
pub const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pt {
    pub x: i32,
    pub y: i32,
}

impl Pt {
    pub fn shifted(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
    pub fn in_bounds(self) -> bool {
        self.x >= 0 && self.x < COLS && self.y >= 0 && self.y < ROWS
    }
    pub fn idx(self) -> usize {
        self.y as usize * COLS as usize + self.x as usize
    }
}

fn conf() -> Conf {
    Conf {
        window_title: "Snake".to_owned(),
        window_width: 900,
        window_height: 720,
        ..Default::default()
    }
}

// ── CLI args (native only — meaningless in a browser tab) ───────────────────────

struct CliArgs {
    /// `--debug`: print every tick's chosen direction/score to stderr.
    debug: bool,
    /// `--once`: play one episode to game-over, print a result line, then exit
    /// instead of looping into a new generation forever.
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

fn log_tick(debug: bool, game: &Game) {
    if !debug {
        return;
    }
    eprintln!(
        "tick head=({},{}) dir={:?} score={} len={} generation={}",
        game.body[0].x,
        game.body[0].y,
        game.dir,
        game.score,
        game.body.len(),
        game.generation,
    );
}

/// Runs the game with no window, no GL context, and no miniquad involvement at all —
/// `Game` has no rendering dependency (`macroquad::rand` is a pure `no_std` PRNG, safe
/// to call standalone), and miniquad has no headless backend to opt into, so the only
/// way to guarantee zero window creation is to never call
/// `miniquad::start`/`Window::from_config` in the first place.
#[cfg(not(target_arch = "wasm32"))]
fn run_headless(cli: CliArgs) -> ! {
    rand::srand(screenshot::seed());
    let mut game = Game::new(1);

    loop {
        if game.tick() {
            log_tick(cli.debug, &game);
        } else {
            if cli.debug {
                eprintln!(
                    "game_over score={} generation={}",
                    game.score, game.generation
                );
            }
            if cli.once {
                println!(
                    "result=game_over score={} generation={}",
                    game.score, game.generation
                );
                std::process::exit(0);
            }
            game = Game::new(game.generation + 1);
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
    let mut game = Game::new(1);
    let mut accum = 0.0f32;
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();

    loop {
        control.handle_keys();

        let n = game.body.len().max(1) as f32;
        let hunger = if game.score >= 10 {
            ((game.ticks_hungry as f32 - n) / n).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let tick_interval = TICK * (1.0 - 0.5 * hunger);

        accum += control.scale(get_frame_time());
        while accum >= tick_interval {
            accum -= tick_interval;
            if game.tick() {
                log_tick(cli.debug, &game);
            } else {
                if cli.debug {
                    eprintln!(
                        "game_over score={} generation={}",
                        game.score, game.generation
                    );
                }
                control.episode_complete("snake", game.score as i64);
                if cli.once {
                    println!(
                        "result=game_over score={} generation={}",
                        game.score, game.generation
                    );
                    std::process::exit(0);
                }
                game = Game::new(game.generation + 1);
                break;
            }
        }

        clear_background(Color {
            r: 0.07,
            g: 0.07,
            b: 0.12,
            a: 1.0,
        });

        let sw = screen_width();
        let sh = screen_height();
        let cell = (sw / COLS as f32)
            .min((sh - 40.0) / ROWS as f32)
            .floor()
            .max(1.0);
        let ow = COLS as f32 * cell;
        let oh = ROWS as f32 * cell;
        let ox = ((sw - ow) * 0.5).floor();
        let oy = ((sh - oh - 30.0) * 0.5 + 30.0).floor();

        draw_rectangle(
            ox - 1.0,
            oy - 1.0,
            ow + 2.0,
            oh + 2.0,
            Color {
                r: 0.15,
                g: 0.17,
                b: 0.25,
                a: 1.0,
            },
        );

        for i in 0..GRID {
            if game.blocks[i] {
                let x = (i % COLS as usize) as f32;
                let y = (i / COLS as usize) as f32;
                draw_rectangle(
                    ox + x * cell + 1.0,
                    oy + y * cell + 1.0,
                    cell - 2.0,
                    cell - 2.0,
                    Color {
                        r: 0.3,
                        g: 0.3,
                        b: 0.35,
                        a: 1.0,
                    },
                );
            }
        }

        let f = game.food;
        let pad = (cell * 0.12).max(2.0);
        draw_rectangle(
            ox + f.x as f32 * cell + pad,
            oy + f.y as f32 * cell + pad,
            cell - 2.0 * pad,
            cell - 2.0 * pad,
            Color {
                r: 0.95,
                g: 0.25,
                b: 0.25,
                a: 1.0,
            },
        );

        for (i, &seg) in game.body.iter().enumerate() {
            let color = if i == 0 {
                // blue → white/gray as hunger rises
                Color {
                    r: 0.08 + 0.92 * hunger,
                    g: 0.6 + 0.4 * hunger,
                    b: 0.95 + 0.05 * hunger,
                    a: 1.0,
                }
            } else {
                let t = 1.0 - (i as f32 / n) * 0.65;
                Color {
                    r: 0.08,
                    g: 0.6 * t,
                    b: 0.95 * t,
                    a: 1.0,
                }
            };
            draw_rectangle(
                ox + seg.x as f32 * cell + 1.0,
                oy + seg.y as f32 * cell + 1.0,
                cell - 2.0,
                cell - 2.0,
                color,
            );
        }

        let font_size = (cell * 0.9).max(14.0);
        let hud = format!("Score: {:>4}   Gen: {}", game.score, game.generation);
        let hud_y = oy - font_size * 0.35;
        draw_text(
            &hud,
            ox,
            hud_y,
            font_size,
            Color {
                r: 0.65,
                g: 0.65,
                b: 0.82,
                a: 1.0,
            },
        );

        let speed_label = control.label();
        let sd = measure_text(&speed_label, None, font_size as u16, 1.0);
        draw_text(
            &speed_label,
            ox + ow - sd.width,
            hud_y,
            font_size,
            Color {
                r: 0.65,
                g: 0.65,
                b: 0.82,
                a: 1.0,
            },
        );

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}
