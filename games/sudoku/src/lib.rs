use macroquad::prelude::*;
use render_cache::RenderCache;

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
const GRID_W: f32 = CELL * 9.0;

/// Digit label + measured width/height for '1'..'9' at both sizes the board draws
/// (filled cells at 34px, candidate pencil-marks at 15px), computed once instead of
/// calling `measure_text` from inside `draw_board`/`draw_candidates` every frame — with
/// up to 81 cells x 9 candidates, that was up to ~700 redundant layout calls/frame,
/// showing up as recurring 100+ms main-thread tasks in Lighthouse traces of this page.
struct DigitMetrics {
    text: [&'static str; 9],
    filled: [TextDimensions; 9],
    candidate: [TextDimensions; 9],
}

impl DigitMetrics {
    fn compute() -> Self {
        let text = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
        let mut filled = [TextDimensions::default(); 9];
        let mut candidate = [TextDimensions::default(); 9];
        for (i, s) in text.iter().enumerate() {
            filled[i] = measure_text(s, None, 34, 1.0);
            candidate[i] = measure_text(s, None, 15, 1.0);
        }
        Self {
            text,
            filled,
            candidate,
        }
    }
}

// ── Variant mode ──────────────────────────────────────────────────────────────
// `V` cycles Easy → Medium → Hard → Expert → Master → Auto → Easy …; Auto rotates
// the five difficulties by generation, same pattern as Klondike's Draw-1/Draw-3/Auto
// cycle.

#[derive(Clone, Copy, PartialEq)]
enum VariantMode {
    Easy,
    Medium,
    Hard,
    Expert,
    Master,
    Auto,
}

impl VariantMode {
    fn next(self) -> Self {
        match self {
            VariantMode::Easy => VariantMode::Medium,
            VariantMode::Medium => VariantMode::Hard,
            VariantMode::Hard => VariantMode::Expert,
            VariantMode::Expert => VariantMode::Master,
            VariantMode::Master => VariantMode::Auto,
            VariantMode::Auto => VariantMode::Easy,
        }
    }

    fn difficulty(self, generation: u32) -> Difficulty {
        match self {
            VariantMode::Easy => Difficulty::Easy,
            VariantMode::Medium => Difficulty::Medium,
            VariantMode::Hard => Difficulty::Hard,
            VariantMode::Expert => Difficulty::Expert,
            VariantMode::Master => Difficulty::Master,
            VariantMode::Auto => match generation % 5 {
                0 => Difficulty::Easy,
                1 => Difficulty::Medium,
                2 => Difficulty::Hard,
                3 => Difficulty::Expert,
                _ => Difficulty::Master,
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

pub struct CliArgs {
    debug: bool,
    once: bool,
    variant: Option<VariantMode>,
    no_ui: bool,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn parse_cli_args() -> CliArgs {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (base, rest) = game_common::parse_base_args(&args);

    let mut variant = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--variant" => {
                i += 1;
                let v = rest.get(i).unwrap_or_else(|| {
                    eprintln!(
                        "--variant requires a value: easy, medium, hard, expert, master, or auto"
                    );
                    std::process::exit(2);
                });
                variant = Some(match v.as_str() {
                    "easy" => VariantMode::Easy,
                    "medium" => VariantMode::Medium,
                    "hard" => VariantMode::Hard,
                    "expert" => VariantMode::Expert,
                    "master" => VariantMode::Master,
                    "auto" => VariantMode::Auto,
                    other => {
                        eprintln!(
                            "unknown --variant value '{other}': expected easy, medium, hard, expert, master, or auto"
                        );
                        std::process::exit(2);
                    }
                });
            }
            other => {
                eprintln!(
                    "unknown argument '{other}' (expected --debug, --once, --no-ui, --variant <easy|medium|hard|expert|master|auto>)"
                );
                std::process::exit(2);
            }
        }
        i += 1;
    }

    CliArgs {
        debug: base.debug,
        once: base.once,
        variant,
        no_ui: base.no_ui,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn parse_cli_args() -> CliArgs {
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
pub fn run_headless(cli: CliArgs) {
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

pub fn conf() -> Conf {
    Conf {
        window_title: "Sudoku".to_owned(),
        window_width: 900,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

/// Entry point for the standalone per-game binary — same window/`--no-ui` branching
/// `main()` used to do. Manual rather than `#[macroquad::main(conf)]`: that attribute
/// unconditionally calls `Window::from_config` before any of our code runs — too late to
/// skip window/GL setup for `--no-ui`.
pub fn start() {
    let cli = parse_cli_args();
    if cli.no_ui {
        run_headless(cli);
        return;
    }
    macroquad::Window::from_config(conf(), run_ui(cli));
}

/// The `CliArgs` a browser build gets: no argv to parse, so every flag is off and
/// `variant` stays `None` (i.e. the game keeps its Auto difficulty rotation).
fn bundled_cli() -> CliArgs {
    CliArgs {
        debug: false,
        once: false,
        variant: None,
        no_ui: false,
    }
}

/// Entry point for the merged multi-game binary (see `bundle/`): no argv parsing —
/// the bundled build gets the same `CliArgs` the browser build does.
pub async fn play() {
    run_ui(bundled_cli()).await;
}

pub async fn run_ui(cli: CliArgs) {
    let mut control = control::Control::new();
    macroquad::rand::srand(control.seed());

    let mut mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut game = new_game_for(mode, 0);
    let mut solver = Solver::new();
    let mut accum = 0.0f32;
    let mut highlight: Option<(Move, f32)> = None;
    let mut end_time: Option<f64> = None;
    let mut shot = screenshot::Capture::from_env();
    let metrics = DigitMetrics::compute();
    // Set once the daily-challenge run ends (see `control::Control::daily_mode`) — the
    // board freezes in its solved state instead of starting a new episode.
    let mut daily_done = false;

    // The board (cell backgrounds, digits/candidates, grid lines) only actually
    // changes once per solver tick (`TICK`, ~7/sec), but was being fully re-drawn
    // every render frame (~60/sec) — up to ~700 draw_text calls apiece. See
    // `render_cache::RenderCache` for why/impact; the move highlight fades
    // continuously so it's drawn fresh every frame on top instead, via `draw_highlight`.
    let mut board_cache = RenderCache::new(Rect::new(OX, OY, GRID_W, GRID_W));

    loop {
        control.handle_keys();
        let now = macroquad::miniquad::date::now();
        let dt = control.scale(get_frame_time().min(0.1));

        if is_key_pressed(KeyCode::V) || control.variant_swipe() {
            mode = mode.next();
            game = new_game_for(mode, game.generation + 1);
            solver = Solver::new();
            accum = 0.0;
            highlight = None;
            end_time = None;
            board_cache.mark_dirty();
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
                        board_cache.mark_dirty();
                    }
                }
            }
            Phase::Solved => {
                if cli.once {
                    print_result(&game);
                    std::process::exit(0);
                }
                let t = *end_time.get_or_insert(now);
                if !daily_done && now - t > RESTART_DELAY {
                    control.episode_complete("sudoku", game.moves as i64);
                    if control.daily_mode() {
                        control::share_result(&control::daily_verdict_text(
                            "Sudoku",
                            control::daily_puzzle_number(),
                            &format!("solve it in {} moves", game.moves),
                        ));
                        daily_done = true;
                    } else {
                        game = new_game_for(mode, game.generation + 1);
                        solver = Solver::new();
                        accum = 0.0;
                        highlight = None;
                        end_time = None;
                        board_cache.mark_dirty();
                    }
                }
            }
        }

        clear_background(Color::new(0.09, 0.09, 0.13, 1.0));
        if !control.stream_mode() {
            draw_hud(&game, mode.label(), &control.label(), control.daily_mode());
        }
        board_cache.draw(|| draw_board_static(&game, &metrics));
        draw_highlight(highlight);

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}

// ── HUD ───────────────────────────────────────────────────────────────────────

fn draw_hud(game: &Game, mode_label: &str, speed_label: &str, daily_mode: bool) {
    let sw = screen_width();
    // Daily-challenge runs freeze once solved rather than restarting (see
    // `control::Control::daily_mode`) — "Restarting..." would be an outright lie there.
    let (txt_col, status) = match game.phase {
        Phase::Solved if daily_mode => (Color::new(0.28, 1.0, 0.52, 1.0), "  - SOLVED!"),
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

    let legend = [
        ("only choice", technique_color(Technique::NakedSingle)),
        ("only spot", technique_color(Technique::HiddenSingle)),
        ("elimination", technique_color(Technique::LockedCandidate)),
        ("guess", technique_color(Technique::Guess)),
    ];
    let mut x = 10.0;
    for (i, (label, color)) in legend.iter().enumerate() {
        draw_text(label, x, 56.0, 15.0, *color);
        x += measure_text(label, None, 15, 1.0).width;
        if i + 1 < legend.len() {
            draw_text("  |  ", x, 56.0, 15.0, Color::new(0.5, 0.5, 0.6, 0.9));
            x += measure_text("  |  ", None, 15, 1.0).width;
        }
    }
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

/// Cell backgrounds, digits/candidates, and grid lines — everything about the board
/// that only changes once per solver tick (`TICK`, ~7/sec). Cached into a render
/// target by the caller instead of being re-run every render frame (~60/sec); see
/// `board_dirty` in `run_ui`. Does NOT draw the move highlight, which fades
/// continuously and must stay a per-frame draw — see `draw_highlight`.
fn draw_board_static(game: &Game, metrics: &DigitMetrics) {
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
            let i = (digit - 1) as usize;
            let s = metrics.text[i];
            let d = metrics.filled[i];
            let tx = x + CELL * 0.5 - d.width * 0.5;
            let ty = y + CELL * 0.5 + d.height * 0.4;
            if game.given[idx] {
                // No bold weight in macroquad's default font — fake it by drawing the
                // glyph a few times at sub-pixel offsets to thicken the strokes.
                let color = Color::new(0.85, 0.85, 0.92, 1.0);
                for (ox, oy) in [(0.0, 0.0), (0.7, 0.0), (0.0, 0.7), (0.7, 0.7)] {
                    draw_text(s, tx + ox, ty + oy, 34.0, color);
                }
            } else {
                let color = technique_color(game.filled_by[idx].unwrap_or(Technique::NakedSingle));
                draw_text(s, tx, ty, 34.0, color);
            }
        } else {
            draw_candidates(x, y, game.candidates[idx], metrics);
        }
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
            OY + GRID_W,
            w,
            c,
        );
        draw_line(
            OX,
            OY + i as f32 * CELL,
            OX + GRID_W,
            OY + i as f32 * CELL,
            w,
            c,
        );
    }
}

/// Highlight the cell the current move touched (fades over `HIGHLIGHT_FADE`). Drawn
/// fresh every frame directly to screen — unlike `draw_board_static`, its alpha
/// changes continuously so it can't be baked into the cached board texture.
fn draw_highlight(highlight: Option<(Move, f32)>) {
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
}

/// Pencil marks: a 3x3 mini-grid of the digits still possible in this cell, always kept
/// current (the solver's naked/hidden-single and locked-candidate deductions all read
/// straight off this same bitmask), so what's drawn is exactly what the algorithm is
/// weighing for its next move — not a separate display-only computation.
fn draw_candidates(x: f32, y: f32, mask: u16, metrics: &DigitMetrics) {
    let sub = CELL / 3.0;
    for d in 1..=9u8 {
        if mask & game::bit(d) == 0 {
            continue;
        }
        let i = (d - 1) as usize;
        let fi = i as f32;
        let cx = x + (fi % 3.0) * sub + sub * 0.5;
        let cy = y + (fi / 3.0).floor() * sub + sub * 0.5;
        let s = metrics.text[i];
        let dm = metrics.candidate[i];
        draw_text(
            s,
            cx - dm.width * 0.5,
            cy + dm.height * 0.4,
            15.0,
            Color::new(0.5, 0.55, 0.65, 0.85),
        );
    }
}
