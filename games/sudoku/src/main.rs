use macroquad::prelude::*;

mod game;
mod solver;

use game::{Difficulty, Game, Move, Phase, Technique, col, row};
use solver::Solver;

const TICK: f32 = 0.14;
const HIGHLIGHT_FADE: f32 = 0.5;
const RESTART_DELAY: f64 = 2.8;

const CELL: f32 = 62.0;
const OX: f32 = (900.0 - CELL * 9.0) / 2.0;
const OY: f32 = 80.0;

// ── Variant mode ──────────────────────────────────────────────────────────────
// `V` cycles Easy → Medium → Hard → Auto → Easy …; Auto rotates the three
// difficulties by generation, same pattern as Klondike's Draw-1/Draw-3/Auto cycle.

#[derive(Clone, Copy, PartialEq)]
enum VariantMode {
    Easy,
    Medium,
    Hard,
    Auto,
}

impl VariantMode {
    fn next(self) -> Self {
        match self {
            VariantMode::Easy => VariantMode::Medium,
            VariantMode::Medium => VariantMode::Hard,
            VariantMode::Hard => VariantMode::Auto,
            VariantMode::Auto => VariantMode::Easy,
        }
    }

    fn difficulty(self, generation: u32) -> Difficulty {
        match self {
            VariantMode::Easy => Difficulty::Easy,
            VariantMode::Medium => Difficulty::Medium,
            VariantMode::Hard => Difficulty::Hard,
            VariantMode::Auto => match generation % 3 {
                0 => Difficulty::Easy,
                1 => Difficulty::Medium,
                _ => Difficulty::Hard,
            },
        }
    }

    fn label(self) -> &'static str {
        match self {
            VariantMode::Auto => " (auto)",
            _ => "",
        }
    }
}

fn new_game_for(mode: VariantMode, generation: u32) -> Game {
    Game::new(mode.difficulty(generation), generation)
}

// ── CLI args (native only) ───────────────────────────────────────────────────

struct CliArgs {
    debug: bool,
    once: bool,
    variant: Option<VariantMode>,
    no_ui: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_cli_args() -> CliArgs {
    let mut debug = false;
    let mut once = false;
    let mut variant = None;
    let mut no_ui = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--debug" => debug = true,
            "--once" => once = true,
            "--no-ui" => no_ui = true,
            "--variant" => {
                i += 1;
                let v = args.get(i).unwrap_or_else(|| {
                    eprintln!("--variant requires a value: easy, medium, hard, or auto");
                    std::process::exit(2);
                });
                variant = Some(match v.as_str() {
                    "easy" => VariantMode::Easy,
                    "medium" => VariantMode::Medium,
                    "hard" => VariantMode::Hard,
                    "auto" => VariantMode::Auto,
                    other => {
                        eprintln!(
                            "unknown --variant value '{other}': expected easy, medium, hard, or auto"
                        );
                        std::process::exit(2);
                    }
                });
            }
            other => {
                eprintln!(
                    "unknown argument '{other}' (expected --debug, --once, --no-ui, --variant <easy|medium|hard|auto>)"
                );
                std::process::exit(2);
            }
        }
        i += 1;
    }

    CliArgs {
        debug,
        once,
        variant,
        no_ui,
    }
}

#[cfg(target_arch = "wasm32")]
fn parse_cli_args() -> CliArgs {
    CliArgs {
        debug: false,
        once: false,
        variant: None,
        no_ui: false,
    }
}

fn technique_label(t: Technique) -> &'static str {
    match t {
        Technique::NakedSingle => "naked single",
        Technique::HiddenSingle => "hidden single",
        Technique::LockedCandidate => "locked candidate",
        Technique::Guess => "guess",
    }
}

fn log_move(game: &Game, m: Move) {
    match m {
        Move::Place {
            idx,
            digit,
            technique,
        } => eprintln!(
            "place r{} c{}={} via {} moves={} gen={}",
            row(idx) + 1,
            col(idx) + 1,
            digit,
            technique_label(technique),
            game.moves,
            game.generation + 1,
        ),
        Move::Narrow {
            idx,
            digit,
            technique,
        } => eprintln!(
            "narrow r{} c{} -{} via {} moves={} gen={}",
            row(idx) + 1,
            col(idx) + 1,
            digit,
            technique_label(technique),
            game.moves,
            game.generation + 1,
        ),
    }
}

fn print_result(game: &Game) {
    println!(
        "result=solved difficulty={:?} clues={} moves={}",
        game.difficulty,
        game.clue_count(),
        game.moves,
    );
}

/// `--no-ui`: same solver loop as `run_ui` but with no window/GL context and no per-tick
/// pacing, so scripted runs (`--once`, soak testing) aren't limited by `TICK`.
fn run_headless(cli: CliArgs) {
    macroquad::rand::srand(screenshot::seed());

    let mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut game = new_game_for(mode, 0);
    let mut solver = Solver::new();

    loop {
        match game.phase {
            Phase::Playing => {
                if let Some(m) = solver.choose_move(&game) {
                    if cli.debug {
                        log_move(&game, m);
                    }
                    game.apply(m);
                }
            }
            Phase::Solved => {
                print_result(&game);
                if cli.once {
                    return;
                }
                game = new_game_for(mode, game.generation + 1);
                solver = Solver::new();
            }
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn conf() -> Conf {
    Conf {
        window_title: "Sudoku".to_owned(),
        window_width: 900,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

fn main() {
    let cli = parse_cli_args();
    if cli.no_ui {
        run_headless(cli);
        return;
    }
    macroquad::Window::from_config(conf(), run_ui(cli));
}

async fn run_ui(cli: CliArgs) {
    macroquad::rand::srand(screenshot::seed());

    let mut mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut game = new_game_for(mode, 0);
    let mut solver = Solver::new();
    let mut accum = 0.0f32;
    let mut highlight: Option<(Move, f32)> = None;
    let mut end_time: Option<f64> = None;
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();

    loop {
        control.handle_keys();
        let now = macroquad::miniquad::date::now();
        let dt = control.scale(get_frame_time().min(0.1));

        if is_key_pressed(KeyCode::V) {
            mode = mode.next();
            game = new_game_for(mode, game.generation + 1);
            solver = Solver::new();
            accum = 0.0;
            highlight = None;
            end_time = None;
        }

        if let Some((_, t)) = &mut highlight {
            *t += dt;
        }

        match game.phase {
            Phase::Playing => {
                accum += dt;
                if accum >= TICK {
                    accum -= TICK;
                    if let Some(m) = solver.choose_move(&game) {
                        if cli.debug {
                            log_move(&game, m);
                        }
                        game.apply(m);
                        highlight = Some((m, 0.0));
                    }
                }
            }
            Phase::Solved => {
                if cli.once {
                    print_result(&game);
                    std::process::exit(0);
                }
                let t = *end_time.get_or_insert(now);
                if now - t > RESTART_DELAY {
                    control.episode_complete("sudoku", game.moves as i64);
                    game = new_game_for(mode, game.generation + 1);
                    solver = Solver::new();
                    accum = 0.0;
                    highlight = None;
                    end_time = None;
                }
            }
        }

        clear_background(Color::new(0.09, 0.09, 0.13, 1.0));
        draw_hud(&game, mode.label(), &control.label());
        draw_board(&game, highlight);

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}

// ── HUD ───────────────────────────────────────────────────────────────────────

fn draw_hud(game: &Game, mode_label: &str, speed_label: &str) {
    let sw = screen_width();
    let (txt_col, status) = match game.phase {
        Phase::Solved => (
            Color::new(0.28, 1.0, 0.52, 1.0),
            "  - SOLVED! Restarting...",
        ),
        Phase::Playing => (Color::new(0.68, 0.68, 0.85, 1.0), ""),
    };
    draw_rectangle(0.0, 0.0, sw, 34.0, Color::new(0.05, 0.05, 0.09, 1.0));
    let msg = format!(
        "Sudoku  {}{}   Clues: {}   Moves: {}   Gen: {}{}",
        game.difficulty.label(),
        mode_label,
        game.clue_count(),
        game.moves,
        game.generation + 1,
        status,
    );
    draw_text(&msg, 10.0, 24.0, 20.0, txt_col);

    let sd = measure_text(speed_label, None, 20, 1.0);
    draw_text(speed_label, sw - 8.0 - sd.width, 24.0, 20.0, txt_col);

    let legend = "green=naked single   blue=hidden single   red=locked   orange=guess";
    draw_text(legend, 10.0, 56.0, 15.0, Color::new(0.5, 0.5, 0.6, 0.9));
}

fn technique_color(t: Technique) -> Color {
    match t {
        Technique::NakedSingle => Color::new(0.35, 0.95, 0.45, 1.0),
        Technique::HiddenSingle => Color::new(0.40, 0.70, 1.0, 1.0),
        Technique::LockedCandidate => Color::new(1.0, 0.45, 0.45, 1.0),
        Technique::Guess => Color::new(1.0, 0.65, 0.20, 1.0),
    }
}

// ── Board ─────────────────────────────────────────────────────────────────────

fn cell_pos(idx: usize) -> (f32, f32) {
    (OX + col(idx) as f32 * CELL, OY + row(idx) as f32 * CELL)
}

fn draw_board(game: &Game, highlight: Option<(Move, f32)>) {
    let grid_w = CELL * 9.0;

    // Cell backgrounds + digits/candidates.
    for idx in 0..game::CELLS {
        let (x, y) = cell_pos(idx);
        let shaded = (row(idx) / 3 + col(idx) / 3) % 2 == 1;
        let bg = if shaded {
            Color::new(0.15, 0.15, 0.21, 1.0)
        } else {
            Color::new(0.12, 0.12, 0.17, 1.0)
        };
        draw_rectangle(x, y, CELL, CELL, bg);

        let digit = game.grid[idx];
        if digit != 0 {
            let color = if game.given[idx] {
                Color::new(0.85, 0.85, 0.92, 1.0)
            } else {
                technique_color(game.filled_by[idx].unwrap_or(Technique::NakedSingle))
            };
            let s = digit.to_string();
            let d = measure_text(&s, None, 34, 1.0);
            draw_text(
                &s,
                x + CELL * 0.5 - d.width * 0.5,
                y + CELL * 0.5 + d.height * 0.4,
                34.0,
                color,
            );
        } else {
            draw_candidates(x, y, game.candidates[idx]);
        }
    }

    // Highlight the cell the current move touched (fades over HIGHLIGHT_FADE).
    if let Some((m, t)) = highlight
        && t < HIGHLIGHT_FADE
    {
        let alpha = 1.0 - t / HIGHLIGHT_FADE;
        let (idx, color) = match m {
            Move::Place { idx, technique, .. } => (idx, technique_color(technique)),
            Move::Narrow { idx, technique, .. } => (idx, technique_color(technique)),
        };
        let (x, y) = cell_pos(idx);
        let mut c = color;
        c.a = alpha * 0.85;
        draw_rectangle_lines(x + 1.0, y + 1.0, CELL - 2.0, CELL - 2.0, 4.0, c);
    }

    // Grid lines: thin every cell, thick every box boundary.
    for i in 0..=9 {
        let thick = i % 3 == 0;
        let w = if thick { 3.0 } else { 1.0 };
        let col_gray = if thick { 0.75 } else { 0.35 };
        let c = Color::new(col_gray, col_gray, col_gray, 1.0);
        draw_line(
            OX + i as f32 * CELL,
            OY,
            OX + i as f32 * CELL,
            OY + grid_w,
            w,
            c,
        );
        draw_line(
            OX,
            OY + i as f32 * CELL,
            OX + grid_w,
            OY + i as f32 * CELL,
            w,
            c,
        );
    }
}

/// Pencil marks: a 3x3 mini-grid of the digits still possible in this cell, always kept
/// current (the solver's naked/hidden-single and locked-candidate deductions all read
/// straight off this same bitmask), so what's drawn is exactly what the algorithm is
/// weighing for its next move — not a separate display-only computation.
fn draw_candidates(x: f32, y: f32, mask: u16) {
    let sub = CELL / 3.0;
    for d in 1..=9u8 {
        if mask & game::bit(d) == 0 {
            continue;
        }
        let i = (d - 1) as f32;
        let cx = x + (i % 3.0) * sub + sub * 0.5;
        let cy = y + (i / 3.0).floor() * sub + sub * 0.5;
        let s = d.to_string();
        let dm = measure_text(&s, None, 15, 1.0);
        draw_text(
            &s,
            cx - dm.width * 0.5,
            cy + dm.height * 0.4,
            15.0,
            Color::new(0.5, 0.55, 0.65, 0.85),
        );
    }
}
