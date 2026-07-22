use cards::card::Card;
use cards::render::{draw_card_back, draw_card_face, draw_empty_slot};
use macroquad::prelude::*;
use std::collections::HashSet;

mod game;
mod solver;

use game::{Game, Move, NUM_COLS, Phase, TOTAL_RUNS};
use solver::Solver;

const TICK: f32 = 0.16;
const ANIM_DURATION: f32 = 0.24;
const RESTART_DELAY: f64 = 2.8;

// ── Layout ────────────────────────────────────────────────────────────────────

struct Layout {
    cw: f32,
    ch: f32,
    gap: f32,
    ox: f32,
    top_y: f32,
    tab_y: f32,
    sh: f32,
}

impl Layout {
    fn from_screen() -> Self {
        let sw = screen_width();
        let sh = screen_height();
        let avail_w = sw - 20.0;
        let cw = ((avail_w - 9.0 * 10.0) / NUM_COLS as f32).clamp(26.0, 78.0);
        let ch = cw * 1.40;
        let gap = ((avail_w - NUM_COLS as f32 * cw) / (NUM_COLS as f32 - 1.0)).max(3.0);
        let top_y = 42.0;
        let tab_y = top_y + ch + 16.0;
        Self {
            cw,
            ch,
            gap,
            ox: 10.0,
            top_y,
            tab_y,
            sh,
        }
    }

    fn col_x(&self, i: usize) -> f32 {
        self.ox + i as f32 * (self.cw + self.gap)
    }

    fn stock_pos(&self) -> (f32, f32) {
        (self.col_x(0), self.top_y)
    }

    fn tableau_offsets(&self, game: &Game, pile: usize) -> (f32, f32) {
        let nd = game.n_down[pile];
        let n_up = game.tableau[pile].len().saturating_sub(nd);
        let avail_h = self.sh - self.tab_y - 8.0;
        let down_off = (self.ch * 0.12).max(8.0);
        let up_off_ideal = (self.ch * 0.22).max(11.0);
        let up_off = if n_up > 1 {
            let needed = nd as f32 * down_off + (n_up as f32 - 1.0) * up_off_ideal + self.ch;
            if needed > avail_h {
                ((avail_h - self.ch - nd as f32 * down_off) / (n_up as f32 - 1.0)).max(6.0)
            } else {
                up_off_ideal
            }
        } else {
            up_off_ideal
        };
        (down_off, up_off)
    }

    fn tableau_card_pos(&self, game: &Game, pile: usize, card_idx: usize) -> (f32, f32) {
        let tx = self.col_x(pile);
        let nd = game.n_down[pile];
        let (down_off, up_off) = self.tableau_offsets(game, pile);
        let mut cy = self.tab_y;
        for i in 0..card_idx {
            cy += if i < nd { down_off } else { up_off };
        }
        (tx, cy)
    }

    fn tableau_top_pos(&self, game: &Game, pile: usize) -> (f32, f32) {
        let len = game.tableau[pile].len();
        if len == 0 {
            (self.col_x(pile), self.tab_y)
        } else {
            self.tableau_card_pos(game, pile, len - 1)
        }
    }
}

// ── Animation ─────────────────────────────────────────────────────────────────

struct FlyingCard {
    card: Card,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    /// (pile, index) this card is leaving in the pre-move board, so the static tableau
    /// draw can hide exactly this card — matching by board position rather than by
    /// `Card` value, since spider decks contain duplicate rank/suit combinations and a
    /// value-based hide would blank out every copy on the board while just one flies.
    hide_at: Option<(usize, usize)>,
}

fn compute_flying_cards(game: &Game, m: Move, layout: &Layout) -> Vec<FlyingCard> {
    let mut after = game.clone();
    after.apply(m);

    match m {
        Move::Deal => (0..NUM_COLS)
            .map(|i| {
                let card = game.stock[game.stock.len() - 1 - i];
                let (x0, y0) = layout.stock_pos();
                let (x1, y1) = layout.tableau_top_pos(&after, i);
                FlyingCard {
                    card,
                    x0,
                    y0,
                    x1,
                    y1,
                    hide_at: None,
                }
            })
            .collect(),

        Move::TableauToTableau { from, n, to } => {
            let t = &game.tableau[from];
            (0..n)
                .map(|i| {
                    let card_idx = t.len() - n + i;
                    let card = t[card_idx];
                    let (x0, y0) = layout.tableau_card_pos(game, from, card_idx);
                    // Use the pre-move length of `to` (deterministic) rather than
                    // `after.tableau[to].len()`: if this move completes a 13-card run,
                    // `try_complete` truncates `after`'s pile first, and subtracting `n`
                    // from the now-shorter length would underflow `usize` (wrapping to
                    // near-u64::MAX in release builds) and hang the position-offset loop.
                    let (x1, y1) = layout.tableau_card_pos(&after, to, game.tableau[to].len() + i);
                    FlyingCard {
                        card,
                        x0,
                        y0,
                        x1,
                        y1,
                        hide_at: Some((from, card_idx)),
                    }
                })
                .collect()
        }
    }
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0_f32).powi(3) / 2.0
    }
}

// ── Variant mode ──────────────────────────────────────────────────────────────
// `V` cycles 1-suit → 2-suit → 4-suit → Auto → 1-suit …; Auto matches the
// original alternate-every-round behaviour (derived from `generation`), so
// hitting `V` enough times always lands back on the default rotation.

#[derive(Clone, Copy, PartialEq)]
enum VariantMode {
    Suits1,
    Suits2,
    Suits4,
    Auto,
}

impl VariantMode {
    fn next(self) -> Self {
        match self {
            VariantMode::Suits1 => VariantMode::Suits2,
            VariantMode::Suits2 => VariantMode::Suits4,
            VariantMode::Suits4 => VariantMode::Auto,
            VariantMode::Auto => VariantMode::Suits1,
        }
    }

    fn n_suits(self, generation: u32) -> u8 {
        match self {
            VariantMode::Suits1 => 1,
            VariantMode::Suits2 => 2,
            VariantMode::Suits4 => 4,
            VariantMode::Auto => match generation % 3 {
                0 => 1,
                1 => 2,
                _ => 4,
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

// ── CLI args (native only — meaningless in a browser tab) ───────────────────────

struct CliArgs {
    /// `--debug`: print every move the solver makes to stderr.
    debug: bool,
    /// `--once`: play a single game to Won/Stuck, print a result line, then exit
    /// instead of looping through new generations forever. Meant for scripted runs
    /// (`spider --variant 4 --once --debug`) rather than manual `timeout`/`kill`.
    once: bool,
    /// `--variant <1|2|4|auto>`: pin the starting variant instead of always booting
    /// into Auto (which still rotates 1→2→4 across generations as before).
    variant: Option<VariantMode>,
    /// `--no-ui`: run the game/solver with no window, no GL context, and no miniquad
    /// involvement at all (see `run_headless`) — for scripted `--once --debug --no-ui`
    /// testing that shouldn't need a display to work. Native only: there's no
    /// `run_headless` counterpart on WASM (a browser tab already has no window to
    /// avoid opening), so the field would otherwise sit unread and warn.
    #[cfg(not(target_arch = "wasm32"))]
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
                    eprintln!("--variant requires a value: 1, 2, 4, or auto");
                    std::process::exit(2);
                });
                variant = Some(match v.as_str() {
                    "1" => VariantMode::Suits1,
                    "2" => VariantMode::Suits2,
                    "4" => VariantMode::Suits4,
                    "auto" => VariantMode::Auto,
                    other => {
                        eprintln!("unknown --variant value '{other}': expected 1, 2, 4, or auto");
                        std::process::exit(2);
                    }
                });
            }
            other => {
                eprintln!(
                    "unknown argument '{other}' (expected --debug, --once, --no-ui, --variant <1|2|4|auto>)"
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
    }
}

fn log_move(debug: bool, game: &Game, m: Move) {
    if !debug {
        return;
    }
    eprintln!(
        "move={:?} n_down={:?} completed={}/{} empties={} moves={} gen={}",
        m,
        game.n_down,
        game.completed,
        TOTAL_RUNS,
        game.tableau.iter().filter(|t| t.is_empty()).count(),
        game.moves,
        game.generation + 1,
    );
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn conf() -> Conf {
    Conf {
        window_title: "Spider Solitaire".to_owned(),
        window_width: 900,
        window_height: 720,
        ..Default::default()
    }
}

/// Runs the game with no window, no draw calls, and no miniquad/GL involvement at all —
/// this is a plain synchronous loop over `game`/`solver`, the only two modules in this
/// crate with no macroquad-rendering dependency (`macroquad::rand` is a pure `no_std`
/// PRNG with no windowing tie-in, safe to call here). miniquad has no headless backend
/// to opt into, so the only way to guarantee zero window creation is to never call
/// `miniquad::start`/`Window::from_config` in the first place — hence this being a
/// wholly separate path chosen in `main()` *before* macroquad enters the picture, rather
/// than a flag checked from inside the normal windowed loop.
#[cfg(not(target_arch = "wasm32"))]
fn run_headless(cli: CliArgs) -> ! {
    macroquad::rand::srand(screenshot::seed());

    let mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut game = Game::new(0, mode.n_suits(0));
    let mut solver = Solver::new();

    loop {
        match game.phase {
            Phase::Playing => {
                if let Some(m) = solver.choose_move(&game) {
                    log_move(cli.debug, &game, m);
                    game.apply(m);
                } else {
                    game.phase = Phase::Stuck;
                }
            }
            Phase::Won | Phase::Stuck => {
                if cli.once {
                    println!(
                        "result={} variant={}-suit moves={} completed={}/{}",
                        if game.phase == Phase::Won {
                            "won"
                        } else {
                            "stuck"
                        },
                        game.n_suits,
                        game.moves,
                        game.completed,
                        TOTAL_RUNS,
                    );
                    std::process::exit(0);
                }
                let next_gen = game.generation + 1;
                game = Game::new(next_gen, mode.n_suits(next_gen));
                solver = Solver::new();
            }
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
    macroquad::rand::srand(screenshot::seed());

    let mut mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut game = Game::new(0, mode.n_suits(0));
    let mut display_game = game.clone();
    let mut solver = Solver::new();
    let mut accum = 0.0f32;
    let mut anim_t: f32 = 1.0;
    let mut flying: Vec<FlyingCard> = Vec::new();
    let mut end_time: Option<f64> = None;
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();

    loop {
        control.handle_keys();
        let now = macroquad::miniquad::date::now();
        let dt = control.scale(get_frame_time().min(0.1));
        let layout = Layout::from_screen();

        if is_key_pressed(KeyCode::V) {
            mode = mode.next();
            let next_gen = game.generation + 1;
            game = Game::new(next_gen, mode.n_suits(next_gen));
            display_game = game.clone();
            solver = Solver::new();
            end_time = None;
            accum = 0.0;
            anim_t = 1.0;
            flying.clear();
        }

        anim_t = (anim_t + dt / ANIM_DURATION).min(1.0);
        if anim_t >= 1.0 {
            display_game = game.clone();
            flying.clear();
        }

        match game.phase {
            Phase::Playing => {
                // `--once` is for scripted testing: skip the tick/animation pacing
                // (~4 moves/sec) and just apply moves as fast as the solver can pick
                // them — otherwise grinding toward the 4000-move safety cap can take
                // several minutes of real wall-clock time for no reason.
                if cli.once {
                    if let Some(m) = solver.choose_move(&game) {
                        log_move(cli.debug, &game, m);
                        game.apply(m);
                    } else {
                        game.phase = Phase::Stuck;
                    }
                } else {
                    accum += dt;
                    if anim_t >= 1.0 && accum >= TICK {
                        accum -= TICK;
                        if let Some(m) = solver.choose_move(&game) {
                            log_move(cli.debug, &game, m);
                            flying = compute_flying_cards(&game, m, &layout);
                            game.apply(m);
                            anim_t = 0.0;
                        } else {
                            game.phase = Phase::Stuck;
                        }
                    }
                }
            }
            Phase::Won | Phase::Stuck => {
                if cli.once {
                    println!(
                        "result={} variant={}-suit moves={} completed={}/{}",
                        if game.phase == Phase::Won {
                            "won"
                        } else {
                            "stuck"
                        },
                        game.n_suits,
                        game.moves,
                        game.completed,
                        TOTAL_RUNS,
                    );
                    std::process::exit(0);
                }

                let t = *end_time.get_or_insert(now);
                if now - t > RESTART_DELAY {
                    let score = (game.completed * 13) as i64;
                    control.episode_complete("spider", score);
                    let next_gen = game.generation + 1;
                    game = Game::new(next_gen, mode.n_suits(next_gen));
                    display_game = game.clone();
                    solver = Solver::new();
                    end_time = None;
                    accum = 0.0;
                    anim_t = 1.0;
                    flying.clear();
                }
            }
        }

        let in_flight: HashSet<(usize, usize)> =
            flying.iter().filter_map(|fc| fc.hide_at).collect();

        clear_background(Color::new(0.10, 0.22, 0.14, 1.0));
        draw_hud(&game, mode.label(), &control.label());
        draw_game(&display_game, &layout, &in_flight);

        let t = ease_in_out(anim_t);
        for fc in &flying {
            let x = fc.x0 + (fc.x1 - fc.x0) * t;
            let y = fc.y0 + (fc.y1 - fc.y0) * t;
            draw_card_face(x, y, layout.cw, layout.ch, fc.card);
        }

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}

// ── HUD ───────────────────────────────────────────────────────────────────────

fn draw_hud(game: &Game, mode_label: &str, speed_label: &str) {
    let sw = screen_width();
    let (hud_bg, txt_col) = match game.phase {
        Phase::Won => (
            Color::new(0.05, 0.18, 0.08, 1.0),
            Color::new(0.28, 1.0, 0.52, 1.0),
        ),
        Phase::Stuck => (
            Color::new(0.18, 0.05, 0.05, 1.0),
            Color::new(1.0, 0.38, 0.38, 1.0),
        ),
        Phase::Playing => (
            Color::new(0.07, 0.07, 0.12, 1.0),
            Color::new(0.68, 0.68, 0.85, 1.0),
        ),
    };
    draw_rectangle(0.0, 0.0, sw, 34.0, hud_bg);

    let status = match game.phase {
        Phase::Playing => String::new(),
        Phase::Won => "  - WON! Restarting...".to_owned(),
        Phase::Stuck => "  - STUCK. Restarting...".to_owned(),
    };
    let msg = format!(
        "Spider  {}-suit{}   Runs: {}/{}   Moves: {}   Gen: {}{}",
        game.n_suits,
        mode_label,
        game.completed,
        TOTAL_RUNS,
        game.moves,
        game.generation + 1,
        status,
    );
    draw_text(&msg, 10.0, 24.0, 20.0, txt_col);

    let sd = measure_text(speed_label, None, 20, 1.0);
    draw_text(speed_label, sw - 8.0 - sd.width, 24.0, 20.0, txt_col);
}

// ── Game board ────────────────────────────────────────────────────────────────

fn draw_game(game: &Game, layout: &Layout, in_flight: &HashSet<(usize, usize)>) {
    let Layout {
        cw,
        ch,
        top_y,
        tab_y,
        ..
    } = *layout;

    // ── Stock ─────────────────────────────────────────────────────────────────
    let (sx, _) = layout.stock_pos();
    if game.stock.is_empty() {
        draw_empty_slot(sx, top_y, cw, ch);
    } else {
        draw_card_back(sx, top_y, cw, ch);
        let deals_left = game.stock.len() / NUM_COLS;
        let cnt = deals_left.to_string();
        let d = measure_text(&cnt, None, 16, 1.0);
        draw_text(
            &cnt,
            sx + cw * 0.5 - d.width * 0.5,
            top_y + ch + 14.0,
            16.0,
            Color::new(0.55, 0.65, 0.55, 0.75),
        );
    }

    // ── Completed runs (shown as a small stack next to the stock) ───────────────
    let comp_x = layout.col_x(1);
    if game.completed > 0 {
        draw_card_back(comp_x, top_y, cw, ch);
        let cnt = game.completed.to_string();
        let d = measure_text(&cnt, None, 16, 1.0);
        draw_text(
            &cnt,
            comp_x + cw * 0.5 - d.width * 0.5,
            top_y + ch + 14.0,
            16.0,
            Color::new(0.55, 0.65, 0.55, 0.75),
        );
    } else {
        draw_empty_slot(comp_x, top_y, cw, ch);
    }

    // ── Tableau ───────────────────────────────────────────────────────────────
    for p in 0..NUM_COLS {
        let tx = layout.col_x(p);
        let t = &game.tableau[p];
        let nd = game.n_down[p];

        if t.is_empty() {
            draw_empty_slot(tx, tab_y, cw, ch);
            continue;
        }

        let (down_off, up_off) = layout.tableau_offsets(game, p);
        let mut cy = tab_y;
        for (i, &card) in t.iter().enumerate() {
            let is_last = i == t.len() - 1;
            if i < nd {
                draw_card_back(tx, cy, cw, ch);
                cy += down_off;
            } else {
                if !in_flight.contains(&(p, i)) {
                    draw_card_face(tx, cy, cw, ch, card);
                }
                if !is_last {
                    cy += up_off;
                }
            }
        }
    }
}
