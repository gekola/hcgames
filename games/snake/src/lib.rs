use macroquad::prelude::*;
use render_cache::RenderCache;

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

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

pub fn conf() -> Conf {
    Conf {
        window_title: "Snake".to_owned(),
        window_width: 900,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

/// `(cell, ow, oh, ox, oy)` for a given canvas size — shared by the main draw loop and
/// `board_render_cache` so the cache's rect and the actual drawn geometry can never
/// drift apart.
fn board_geometry(sw: f32, sh: f32) -> (f32, f32, f32, f32, f32) {
    let cell = (sw / COLS as f32)
        .min((sh - 40.0) / ROWS as f32)
        .floor()
        .max(1.0);
    let ow = COLS as f32 * cell;
    let oh = ROWS as f32 * cell;
    let ox = ((sw - ow) * 0.5).floor();
    let oy = ((sh - oh - 30.0) * 0.5 + 30.0).floor();
    (cell, ow, oh, ox, oy)
}

fn board_render_cache(size: (f32, f32)) -> RenderCache {
    let (_, ow, oh, ox, oy) = board_geometry(size.0, size.1);
    RenderCache::new(Rect::new(ox - 1.0, oy - 1.0, ow + 2.0, oh + 2.0))
}

// ── CLI args (native only — meaningless in a browser tab) ───────────────────────

pub struct CliArgs {
    /// `--debug`: print every tick's chosen direction/score to stderr.
    pub debug: bool,
    /// `--once`: play one episode to game-over, print a result line, then exit
    /// instead of looping into a new generation forever.
    pub once: bool,
    /// `--no-ui`: run with no window, no GL context, and no miniquad involvement at
    /// all (see `run_headless`).
    #[cfg(not(target_arch = "wasm32"))]
    pub no_ui: bool,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn parse_cli_args() -> CliArgs {
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
pub fn parse_cli_args() -> CliArgs {
    CliArgs {
        debug: false,
        once: false,
    }
}

/// The `CliArgs` a browser build gets — i.e. what `#[cfg(target_arch = "wasm32")]
/// parse_cli_args()` produces, spelled so it also compiles natively.
fn bundled_cli() -> CliArgs {
    CliArgs {
        debug: false,
        once: false,
        #[cfg(not(target_arch = "wasm32"))]
        no_ui: false,
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
pub fn run_headless(cli: CliArgs) -> ! {
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

/// Entry point for the standalone per-game binary — same window/`--no-ui` branching
/// `main()` used to do.
#[cfg(not(target_arch = "wasm32"))]
pub fn start() {
    let cli = parse_cli_args();
    if cli.no_ui {
        run_headless(cli);
    } else {
        macroquad::Window::from_config(conf(), async move {
            amain(cli).await;
        });
    }
}

/// Entry point for the standalone per-game binary — same window/`--no-ui` branching
/// `main()` used to do.
#[cfg(target_arch = "wasm32")]
pub fn start() {
    macroquad::Window::from_config(conf(), async move {
        amain(parse_cli_args()).await;
    });
}

/// Entry point for the merged multi-game binary (see `bundle/`): no argv parsing —
/// the bundled build gets the same `CliArgs` the browser build does.
pub async fn play() {
    amain(bundled_cli()).await;
}

/// Entry point for the standalone shell (see
/// `.notes/steam-standalone-menu-handoff.md`): runs until the player asks to leave
/// (Esc) or closes the window, then returns instead of looping forever. `play()` above
/// stays as-is for the browser, where there is nothing to return to.
#[cfg(not(target_arch = "wasm32"))]
pub async fn play_until_exit() -> control::ExitReason {
    amain(bundled_cli()).await
}

pub async fn amain(cli: CliArgs) -> control::ExitReason {
    let mut control = control::Control::new();
    rand::srand(control.seed());
    let mut game = Game::new(1);
    let mut accum = 0.0f32;
    let mut shot = screenshot::Capture::from_env();
    // Set once the daily-challenge run ends (see `control::Control::daily_mode`) — the
    // board freezes on its final frame instead of starting a new episode, and the tick
    // loop below is skipped entirely so a game-over `Game` never sees another `tick()`.
    let mut daily_done = false;

    // The board (border, static blocks, food, body) only actually changes once per
    // tick (`TICK`, ~12/sec) but was being fully re-drawn every render frame (~60/sec)
    // regardless. See `render_cache::RenderCache` for why this matters — measured
    // impact stripping snake's drawing entirely showed ~5s of its ~6.3s mobile
    // Lighthouse Total Blocking Time was this redundant redraw.
    let mut cached_size = (screen_width(), screen_height());
    let mut board_cache = board_render_cache(cached_size);

    loop {
        control.handle_keys();
        if let Some(reason) = control.exit_requested() {
            break reason;
        }
        if let Some(seed) = control.take_reseed() {
            rand::srand(seed);
            game = Game::new(1);
            accum = 0.0;
            daily_done = false;
            board_cache.mark_dirty();
        }

        let n = game.body.len().max(1) as f32;
        let hunger = if game.score >= 10 {
            ((game.ticks_hungry as f32 - n) / n).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Second stage past `hunger`==1.0: rides the same two thresholds `choose_dir`
        // uses to widen its own desperation (see game.rs) so the color a player sees
        // tracks the AI's actual risk-taking, not an arbitrary separate scale.
        let starving = if game.score >= 10 {
            let desperate_at = n * game::DESPERATE_TICKS_MULT as f32;
            let starving_at = n * game::STARVING_TICKS_MULT as f32;
            ((game.ticks_hungry as f32 - desperate_at) / (starving_at - desperate_at))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let tick_interval = TICK * (1.0 - 0.5 * hunger);

        accum += control.scale(get_frame_time());
        while !daily_done && accum >= tick_interval {
            accum -= tick_interval;
            if game.tick() {
                log_tick(cli.debug, &game);
                board_cache.mark_dirty();
            } else {
                if cli.debug {
                    eprintln!(
                        "game_over score={} generation={}",
                        game.score, game.generation
                    );
                }
                control.episode_complete("snake", game.score as i64);
                if control.daily_mode() {
                    control::share_result(&control::daily_verdict_text(
                        "Snake",
                        control::daily_puzzle_number(),
                        &format!("score {}", game.score),
                    ));
                    daily_done = true;
                } else if cli.once {
                    println!(
                        "result=game_over score={} generation={}",
                        game.score, game.generation
                    );
                    std::process::exit(0);
                } else {
                    game = Game::new(game.generation + 1);
                }
                board_cache.mark_dirty();
                break;
            }
        }

        clear_background(Color {
            r: 0.07,
            g: 0.07,
            b: 0.12,
            a: 1.0,
        });

        let cur_size = (screen_width(), screen_height());
        if cur_size != cached_size {
            board_cache = board_render_cache(cur_size);
            cached_size = cur_size;
        }

        let (cell, ow, oh, ox, oy) = board_geometry(screen_width(), screen_height());

        board_cache.draw(|| {
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

            // Head-only hunger tint: blue (fed) → white (past `DESPERATE_TICKS_MULT`*n
            // ticks hungry) → purple (past `STARVING_TICKS_MULT`*n, the AI's own
            // livelock-escape point). Body stays plain blue — only the head telegraphs risk.
            let head_base = lerp_color(
                Color {
                    r: 0.08,
                    g: 0.6,
                    b: 0.95,
                    a: 1.0,
                },
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                hunger,
            );
            let head_color = lerp_color(
                head_base,
                Color {
                    r: 0.6,
                    g: 0.15,
                    b: 0.85,
                    a: 1.0,
                },
                starving,
            );

            for (i, &seg) in game.body.iter().enumerate() {
                let color = if i == 0 {
                    head_color
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
        });

        if !control.stream_mode() {
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
        }

        shot.tick();
        screenshot::handle_hotkey();
        control.draw_overlay();
        next_frame().await;
    }
}
