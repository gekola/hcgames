use macroquad::prelude::*;
use render_cache::RenderCache;

mod game;
mod generator;
mod solver;

use game::{Board, Game, H, Phase, Piece, W, full_rows, place_cells, rotation_states};
use generator::{GenMode, PieceGenerator};
use solver::Solver;

// Tetris's board is inherently tall and narrow (10x20) — unlike every other game here,
// which uses the shared 900x720 default (see `xtask::native_size`), it gets its own
// narrower canvas (matched in `conf()` and `xtask::native_size`/`max_fit_scale`, same
// precedent as `game2048`'s 500x610) sized to the board+panel content instead of
// stretching a small next-piece panel across a wide leftover gap.
const WIN_W: f32 = 600.0;
const WIN_H: f32 = 720.0;

const CELL: f32 = 28.0;
const BOARD_W: f32 = W as f32 * CELL;
const BOARD_H: f32 = H as f32 * CELL;
const PANEL_GAP: f32 = 40.0;
/// Inner padding between the panel's bordered container and its content (next-piece
/// boxes, stat lines).
const PANEL_PAD: f32 = 10.0;
/// Width of the NEXT-piece boxes and the stat lines below them.
const PANEL_W: f32 = 120.0;
const PANEL_OUTER_W: f32 = PANEL_W + PANEL_PAD * 2.0;
const BOARD_X: f32 = (WIN_W - (BOARD_W + PANEL_GAP + PANEL_OUTER_W)) / 2.0;
// Tall enough that a piece spawning at `SPAWN_ROW` (2 rows above the board, like real
// Tetris) clears the title text above it instead of drawing through it.
const BOARD_Y: f32 = 128.0;
const PANEL_OUTER_X: f32 = BOARD_X + BOARD_W + PANEL_GAP;
const PANEL_X: f32 = PANEL_OUTER_X + PANEL_PAD;

/// Row the falling piece visually spawns at — purely cosmetic (`Game::apply` places
/// pieces instantly; this is the renderer's own "drop-in" effect, matching how real
/// Tetris spawns a piece a row or two above the visible playfield before it falls in).
const SPAWN_ROW: f32 = -2.0;
/// Animation timeline, as fractions of `FallAnim::t`'s [0, 1] range: hold at the spawn
/// orientation/column, then rotate-and-slide into the target column, then hard-drop the
/// rest of the way — the same three beats a real player (or bot) visibly goes through,
/// not an instant teleport to the final placement. See `FallAnim::pose`.
const ROTATE_FRAC: f32 = 0.15;
const SLIDE_FRAC: f32 = 0.60;
const ANIM_SPEED: f32 = 1.8;
const FLASH_DUR: f32 = 0.28;
const OVER_PAUSE: f32 = 2.5;

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

fn piece_color(piece: Piece) -> Color {
    match piece {
        Piece::I => rgb(45, 226, 230),
        Piece::O => rgb(234, 219, 65),
        Piece::T => rgb(178, 90, 235),
        Piece::S => rgb(90, 216, 105),
        Piece::Z => rgb(232, 82, 82),
        Piece::J => rgb(80, 120, 235),
        Piece::L => rgb(235, 150, 60),
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ── Piece-generation mode (V-cycle) ─────────────────────────────────────────────
// See `generator::GenMode` for what each concrete mode approximates. `Auto` rotates the
// four by generation, same pattern as Sudoku's difficulty cycle.

#[derive(Clone, Copy, PartialEq)]
enum VariantMode {
    Bag7,
    Classic,
    Tgm,
    Memoryless,
    Auto,
}

impl VariantMode {
    fn next(self) -> Self {
        match self {
            VariantMode::Bag7 => VariantMode::Classic,
            VariantMode::Classic => VariantMode::Tgm,
            VariantMode::Tgm => VariantMode::Memoryless,
            VariantMode::Memoryless => VariantMode::Auto,
            VariantMode::Auto => VariantMode::Bag7,
        }
    }

    fn gen_mode(self, generation: u32) -> GenMode {
        match self {
            VariantMode::Bag7 => GenMode::Bag7,
            VariantMode::Classic => GenMode::Classic,
            VariantMode::Tgm => GenMode::Tgm,
            VariantMode::Memoryless => GenMode::Memoryless,
            VariantMode::Auto => match generation % 4 {
                0 => GenMode::Bag7,
                1 => GenMode::Classic,
                2 => GenMode::Tgm,
                _ => GenMode::Memoryless,
            },
        }
    }

    fn name(self, generation: u32) -> &'static str {
        match self.gen_mode(generation) {
            GenMode::Bag7 => "7-bag",
            GenMode::Classic => "classic",
            GenMode::Tgm => "TGM",
            GenMode::Memoryless => "memoryless",
        }
    }

    fn label(self) -> &'static str {
        match self {
            VariantMode::Auto => " (auto)",
            _ => "",
        }
    }
}

/// Groups the piece-generation state (`piece_gen`), the authoritative game state (`game`), and
/// the solver driving it — everything a new episode or a variant switch replaces
/// together. Kept separate from `View`, which only tracks how the *current* piece's
/// placement is being animated.
struct Session {
    mode: VariantMode,
    piece_gen: PieceGenerator,
    game: Game,
    solver: Solver,
}

impl Session {
    fn new(mode: VariantMode, generation: u32) -> Self {
        let mut piece_gen = PieceGenerator::new(mode.gen_mode(generation));
        let game = Game::new(generation, &mut piece_gen);
        Self {
            mode,
            piece_gen,
            game,
            solver: Solver::new(),
        }
    }

    fn next_generation(&self) -> Self {
        Self::new(self.mode, self.game.generation + 1)
    }

    fn switch_variant(&self) -> Self {
        Self::new(self.mode.next(), self.game.generation + 1)
    }
}

/// A piece's cosmetic journey from spawn to its already-decided landing placement — see
/// `pose` for the three visible beats this plays out (hold at spawn, rotate + slide,
/// drop). `Game::apply` has already placed the piece by the time this exists; nothing
/// here feeds back into game state.
struct FallAnim {
    piece: Piece,
    spawn_shape: [(i32, i32); 4],
    target_shape: [(i32, i32); 4],
    spawn_col: i32,
    target_col: i32,
    target_row: f32,
    t: f32,
}

impl FallAnim {
    /// The shape/column/row to draw for the current `t`. Three beats, matching how a
    /// real game (or a bot playing one) actually looks: sit at the spawn column in the
    /// spawn orientation, then rotate (an instant snap — real Tetris doesn't tween
    /// rotation either) while sliding to the target column and dropping partway, then
    /// accelerate straight down the rest of the way (a hard drop).
    fn pose(&self) -> ([(i32, i32); 4], f32, f32) {
        let mid_row = SPAWN_ROW + (self.target_row - SPAWN_ROW) * 0.3;
        if self.t < ROTATE_FRAC {
            (self.spawn_shape, self.spawn_col as f32, SPAWN_ROW)
        } else if self.t < SLIDE_FRAC {
            let local = smoothstep((self.t - ROTATE_FRAC) / (SLIDE_FRAC - ROTATE_FRAC));
            let col = self.spawn_col as f32 + (self.target_col - self.spawn_col) as f32 * local;
            let row = SPAWN_ROW + (mid_row - SPAWN_ROW) * local;
            (self.target_shape, col, row)
        } else {
            let local = (self.t - SLIDE_FRAC) / (1.0 - SLIDE_FRAC);
            let ease_in = local * local;
            let row = mid_row + (self.target_row - mid_row) * ease_in;
            (self.target_shape, self.target_col as f32, row)
        }
    }
}

#[derive(PartialEq)]
enum ViewPhase {
    Falling,
    Flash,
    GameOver,
}

/// Purely cosmetic state layered on top of `Session`: `Game::apply` places a piece and
/// clears lines instantly, so everything here just interpolates from "before" to
/// "already-happened" for the player's benefit. `settled` is what the (cached) board
/// texture actually draws — it only catches up to `session.game.board` once the current
/// piece's fall/flash animation finishes, so the piece never appears twice.
struct View {
    phase: ViewPhase,
    settled: Board,
    fall: Option<FallAnim>,
    locked_board: Board,
    cleared_rows: Vec<usize>,
    flash_t: f32,
    over_t: f32,
}

impl View {
    fn new(session: &Session) -> Self {
        Self {
            phase: ViewPhase::Falling,
            settled: session.game.board,
            fall: None,
            locked_board: session.game.board,
            cleared_rows: Vec::new(),
            flash_t: 0.0,
            over_t: 0.0,
        }
    }

    /// Picks and starts animating the next placement, or switches to the `GameOver`
    /// overlay (reporting the finished episode) if the board's topped out.
    fn advance(&mut self, session: &mut Session, control: &mut control::Control, debug: bool) {
        session.game.refill(&mut session.piece_gen);
        if session.game.phase != Phase::Playing {
            control.episode_complete("tetris", session.game.score as i64);
            if debug {
                eprintln!(
                    "game_over score={} lines={} level={} generation={}",
                    session.game.score,
                    session.game.lines,
                    session.game.level,
                    session.game.generation + 1
                );
            }
            self.phase = ViewPhase::GameOver;
            self.over_t = OVER_PAUSE;
            return;
        }

        let mv = session
            .solver
            .choose_move(&session.game)
            .expect("Phase::Playing guarantees at least one legal placement");
        let piece = session.game.current;
        let target_shape = rotation_states(piece)[mv.rot as usize];
        let mut locked = session.game.board;
        place_cells(&mut locked, piece, &target_shape, mv.col, mv.row);
        let cleared = full_rows(&locked);

        session.game.apply(mv);
        session.game.refill(&mut session.piece_gen);
        if debug {
            eprintln!(
                "drop piece={:?} rot={} col={} lines={} score={} gen={}",
                piece,
                mv.rot,
                mv.col,
                session.game.lines,
                session.game.score,
                session.game.generation + 1
            );
        }

        let spawn_shape = rotation_states(piece)[0];
        let spawn_width = spawn_shape.iter().map(|c| c.0).max().unwrap() + 1;
        let spawn_col = ((W as i32 - spawn_width) / 2).max(0);

        self.fall = Some(FallAnim {
            piece,
            spawn_shape,
            target_shape,
            spawn_col,
            target_col: mv.col,
            target_row: mv.row as f32,
            t: 0.0,
        });
        self.locked_board = locked;
        self.cleared_rows = cleared;
        self.phase = ViewPhase::Falling;
    }
}

// ── CLI args (native only — meaningless in a browser tab) ───────────────────────

struct CliArgs {
    debug: bool,
    once: bool,
    variant: Option<VariantMode>,
    #[cfg(not(target_arch = "wasm32"))]
    no_ui: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_cli_args() -> CliArgs {
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
                        "--variant requires a value: bag7, classic, tgm, memoryless, or auto"
                    );
                    std::process::exit(2);
                });
                variant = Some(match v.as_str() {
                    "bag7" => VariantMode::Bag7,
                    "classic" => VariantMode::Classic,
                    "tgm" => VariantMode::Tgm,
                    "memoryless" => VariantMode::Memoryless,
                    "auto" => VariantMode::Auto,
                    other => {
                        eprintln!(
                            "unknown --variant value '{other}': expected bag7, classic, tgm, memoryless, or auto"
                        );
                        std::process::exit(2);
                    }
                });
            }
            other => {
                eprintln!(
                    "unknown argument '{other}' (expected --debug, --once, --no-ui, --variant <bag7|classic|tgm|memoryless|auto>)"
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
fn parse_cli_args() -> CliArgs {
    CliArgs {
        debug: false,
        once: false,
        variant: None,
    }
}

fn print_result(session: &Session) {
    println!(
        "result=game_over score={} lines={} level={} generation={}",
        session.game.score,
        session.game.lines,
        session.game.level,
        session.game.generation + 1
    );
}

/// Same solver loop as the windowed game, but with no window/GL context and no per-tick
/// pacing — `Game::apply` is an instant, discrete move (unlike a dt-driven game), so
/// there's no virtual-time stepping to do here, just run flat-out.
#[cfg(not(target_arch = "wasm32"))]
fn run_headless(cli: CliArgs) {
    macroquad::rand::srand(screenshot::seed());
    let mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut session = Session::new(mode, 0);

    loop {
        session.game.refill(&mut session.piece_gen);
        match session.game.phase {
            Phase::Playing => {
                let mv = session
                    .solver
                    .choose_move(&session.game)
                    .expect("Phase::Playing guarantees at least one legal placement");
                let piece = session.game.current;
                session.game.apply(mv);
                if cli.debug {
                    eprintln!(
                        "drop piece={:?} rot={} col={} lines={} score={} gen={}",
                        piece,
                        mv.rot,
                        mv.col,
                        session.game.lines,
                        session.game.score,
                        session.game.generation + 1
                    );
                }
            }
            Phase::GameOver => {
                print_result(&session);
                if cli.once {
                    return;
                }
                session = session.next_generation();
            }
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn conf() -> Conf {
    Conf {
        window_title: "Tetris".to_owned(),
        window_width: WIN_W as i32,
        window_height: WIN_H as i32,
        high_dpi: true,
        ..Default::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = parse_cli_args();
    if cli.no_ui {
        run_headless(cli);
        return;
    }
    macroquad::Window::from_config(conf(), amain(cli));
}

#[cfg(target_arch = "wasm32")]
fn main() {
    macroquad::Window::from_config(conf(), amain(parse_cli_args()));
}

async fn amain(cli: CliArgs) {
    rand::srand(screenshot::seed());
    let mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut session = Session::new(mode, 0);
    let mut view = View::new(&session);
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();

    // The locked board (up to 200 flat-colored cells, no text) is redrawn only when a
    // piece finishes falling/flashing, not every render frame — see `render_cache` and
    // `games/snake/src/main.rs` for the same pattern on an equally text-free board. The
    // falling piece and the line-clear flash both change continuously, so they stay live
    // per-frame draws on top, same split as every other game here.
    let mut board_cache = RenderCache::new(Rect::new(
        BOARD_X - 1.0,
        BOARD_Y - 1.0,
        BOARD_W + 2.0,
        BOARD_H + 2.0,
    ));

    view.advance(&mut session, &mut control, cli.debug);
    board_cache.mark_dirty();

    loop {
        control.handle_keys();
        let dt = control.scale(get_frame_time());

        if is_key_pressed(KeyCode::V) || control.variant_swipe() {
            session = session.switch_variant();
            view = View::new(&session);
            view.advance(&mut session, &mut control, cli.debug);
            board_cache.mark_dirty();
        }

        match view.phase {
            ViewPhase::Falling => {
                if let Some(fall) = &mut view.fall {
                    fall.t = (fall.t + dt * ANIM_SPEED).min(1.0);
                    if fall.t >= 1.0 {
                        if view.cleared_rows.is_empty() {
                            view.settled = session.game.board;
                            board_cache.mark_dirty();
                            view.advance(&mut session, &mut control, cli.debug);
                            board_cache.mark_dirty();
                        } else {
                            view.settled = view.locked_board;
                            view.flash_t = 0.0;
                            view.phase = ViewPhase::Flash;
                            board_cache.mark_dirty();
                        }
                    }
                }
            }
            ViewPhase::Flash => {
                view.flash_t += dt;
                if view.flash_t >= FLASH_DUR {
                    view.settled = session.game.board;
                    board_cache.mark_dirty();
                    view.advance(&mut session, &mut control, cli.debug);
                    board_cache.mark_dirty();
                }
            }
            ViewPhase::GameOver => {
                if cli.once {
                    print_result(&session);
                    std::process::exit(0);
                }
                view.over_t -= dt;
                if view.over_t <= 0.0 {
                    session = session.next_generation();
                    view = View::new(&session);
                    view.advance(&mut session, &mut control, cli.debug);
                    board_cache.mark_dirty();
                }
            }
        }

        clear_background(rgb(15, 15, 20));

        draw_hud(&session, &control);

        board_cache.draw(|| draw_board_static(&view.settled));

        if view.phase == ViewPhase::Falling
            && let Some(fall) = &view.fall
        {
            draw_falling(fall);
        }
        if view.phase == ViewPhase::Flash {
            draw_flash(&view.cleared_rows, view.flash_t);
        }
        if view.phase == ViewPhase::GameOver && !control.stream_mode() {
            draw_game_over(view.over_t);
        }

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw_cell(x: f32, y: f32, color: Color) {
    draw_rectangle(x + 1.0, y + 1.0, CELL - 2.0, CELL - 2.0, color);
}

/// The locked board: background, grid lines, and every settled cell. No text at all —
/// see the `board_cache` comment in `amain` for why that matters.
fn draw_board_static(board: &Board) {
    draw_rectangle(
        BOARD_X - 1.0,
        BOARD_Y - 1.0,
        BOARD_W + 2.0,
        BOARD_H + 2.0,
        rgb(60, 60, 75),
    );
    draw_rectangle(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, rgb(18, 18, 26));

    for (r, row) in board.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if let Some(piece) = cell {
                draw_cell(
                    BOARD_X + c as f32 * CELL,
                    BOARD_Y + r as f32 * CELL,
                    piece_color(*piece),
                );
            }
        }
    }

    let grid = rgb(35, 35, 46);
    for c in 0..=W {
        let x = BOARD_X + c as f32 * CELL;
        draw_line(x, BOARD_Y, x, BOARD_Y + BOARD_H, 1.0, grid);
    }
    for r in 0..=H {
        let y = BOARD_Y + r as f32 * CELL;
        draw_line(BOARD_X, y, BOARD_X + BOARD_W, y, 1.0, grid);
    }
}

/// The currently-falling piece, interpolated between its cosmetic spawn point and its
/// (already locked-in, per `Game::apply`) landing placement. Live every frame — unlike
/// `draw_board_static`, its position changes continuously.
fn draw_falling(fall: &FallAnim) {
    let (shape, col, row) = fall.pose();
    let color = piece_color(fall.piece);
    for &(dx, dy) in &shape {
        draw_cell(
            BOARD_X + (col + dx as f32) * CELL,
            BOARD_Y + (row + dy as f32) * CELL,
            color,
        );
    }
}

/// Blinks the rows about to be cleared a few times over `FLASH_DUR`, drawn on top of the
/// (already-locked, pre-clear) cached board texture. Live every frame — the blink state
/// changes continuously.
fn draw_flash(cleared_rows: &[usize], flash_t: f32) {
    let half_cycles = (flash_t / FLASH_DUR * 6.0) as i32;
    if half_cycles % 2 != 0 {
        return;
    }
    for &r in cleared_rows {
        draw_rectangle(
            BOARD_X,
            BOARD_Y + r as f32 * CELL,
            BOARD_W,
            CELL,
            Color::new(1.0, 1.0, 1.0, 0.85),
        );
    }
}

fn draw_piece_preview(x: f32, y: f32, w: f32, h: f32, piece: Piece) {
    const PREVIEW_CELL: f32 = 14.0;
    let shape = rotation_states(piece)[0];
    let sw = (shape.iter().map(|c| c.0).max().unwrap() + 1) as f32 * PREVIEW_CELL;
    let sh = (shape.iter().map(|c| c.1).max().unwrap() + 1) as f32 * PREVIEW_CELL;
    let ox = x + (w - sw) * 0.5;
    let oy = y + (h - sh) * 0.5;
    let color = piece_color(piece);
    for &(dx, dy) in &shape {
        draw_rectangle(
            ox + dx as f32 * PREVIEW_CELL + 1.0,
            oy + dy as f32 * PREVIEW_CELL + 1.0,
            PREVIEW_CELL - 2.0,
            PREVIEW_CELL - 2.0,
            color,
        );
    }
}

/// Title, mode/speed labels, and the side panel (next-piece previews + score/lines/
/// level). Cheap enough (a handful of `draw_text`/`draw_rectangle` calls, no per-cell
/// text) to redraw every frame directly rather than caching, same as every other game's
/// top HUD strip. Drawn unconditionally, even in stream mode — this is the game's own
/// visual identity (title, live score), not a HUD overlay for a spectator to hide. Only
/// the speed multiplier (meaningless with no visitor around to have changed it) is
/// stream-mode-gated, same split as `game2048`'s title/score-box HUD.
fn draw_hud(session: &Session, control: &control::Control) {
    let text = rgb(210, 210, 225);
    let dim = rgb(140, 140, 160);

    draw_text("TETRIS", BOARD_X, 46.0, 34.0, text);
    let mode_label = format!(
        "{}{}",
        session.mode.name(session.game.generation),
        session.mode.label()
    );
    draw_text(&mode_label, BOARD_X, 72.0, 18.0, dim);

    if !control.stream_mode() {
        let speed = control.label();
        let sd = measure_text(&speed, None, 20, 1.0);
        draw_text(&speed, WIN_W - 20.0 - sd.width, 46.0, 20.0, dim);
    }

    // The panel container: same border/fill treatment as the board, spanning its full
    // height. Without this, the panel's actual content (next-piece boxes + a handful of
    // stat lines) only fills the top third or so of the board's height, leaving a tall
    // stretch of bare background beneath it that reads as extra empty space on the right
    // — even though the board and panel are already horizontally centered as a pair.
    draw_rectangle(
        PANEL_OUTER_X - 1.0,
        BOARD_Y - 1.0,
        PANEL_OUTER_W + 2.0,
        BOARD_H + 2.0,
        rgb(60, 60, 75),
    );
    draw_rectangle(
        PANEL_OUTER_X,
        BOARD_Y,
        PANEL_OUTER_W,
        BOARD_H,
        rgb(18, 18, 26),
    );

    draw_text("NEXT", PANEL_X, BOARD_Y + 16.0, 20.0, dim);
    let mut y = BOARD_Y + 26.0;
    for &piece in session.game.queue.iter().take(3) {
        draw_rectangle(PANEL_X, y, PANEL_W, 68.0, rgb(24, 24, 32));
        draw_piece_preview(PANEL_X, y, PANEL_W, 68.0, piece);
        y += 80.0;
    }

    y += 16.0;
    for (label, value) in [
        ("SCORE", session.game.score),
        ("LINES", session.game.lines),
        ("LEVEL", session.game.level),
        ("GEN", session.game.generation + 1),
    ] {
        let line = format!("{label}  {value}");
        draw_text(&line, PANEL_X, y, 20.0, text);
        y += 28.0;
    }
}

fn draw_game_over(over_t: f32) {
    draw_rectangle(
        BOARD_X,
        BOARD_Y,
        BOARD_W,
        BOARD_H,
        Color::new(0.0, 0.0, 0.0, 0.72),
    );
    let cx = BOARD_X + BOARD_W * 0.5;
    let cy = BOARD_Y + BOARD_H * 0.5;

    let title = "GAME OVER";
    let d = measure_text(title, None, 30, 1.0);
    draw_text(title, cx - d.width * 0.5, cy - 10.0, 30.0, rgb(240, 90, 90));

    let sub = format!("Restarting in {:.0}...", over_t.max(0.0));
    let sd = measure_text(&sub, None, 18, 1.0);
    draw_text(
        &sub,
        cx - sd.width * 0.5,
        cy + 22.0,
        18.0,
        rgb(210, 210, 225),
    );
}
