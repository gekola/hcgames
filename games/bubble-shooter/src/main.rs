use macroquad::prelude::*;
use render_cache::RenderCache;

mod game;
mod solver;

use game::{
    BOARD_H, BOARD_W, BOARD_X, BOARD_Y, Board, Color, DEATH_ROW, Game, Outcome, Phase, RADIUS,
    Resolution, SHOOTER_X, SHOOTER_Y, cell_pixel,
};

const FLIGHT_SPEED: f32 = 1000.0;
const MIN_FLIGHT_DUR: f32 = 0.12;
const MAX_FLIGHT_DUR: f32 = 0.55;
const POP_DUR: f32 = 0.22;
const FALL_DUR: f32 = 0.35;
const IDLE_DUR: f32 = 0.22;
const OVER_PAUSE: f32 = 2.5;

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

// `Color` is already taken by `game::Color` (bubble color) — alias macroquad's own
// `Color` type locally so both can be spelled without `game::`/`macroquad::` prefixes
// everywhere below.
type Color32 = macroquad::color::Color;

fn color_rgb(c: Color) -> Color32 {
    match c {
        Color::Red => rgb(224, 68, 68),
        Color::Orange => rgb(232, 142, 44),
        Color::Yellow => rgb(226, 202, 54),
        Color::Green => rgb(88, 190, 96),
        Color::Blue => rgb(72, 132, 232),
        Color::Purple => rgb(176, 92, 222),
    }
}

fn darken(c: Color32, amt: f32) -> Color32 {
    Color32::new(c.r * (1.0 - amt), c.g * (1.0 - amt), c.b * (1.0 - amt), c.a)
}

/// Opaque, precomputed "white at alpha `al` over `base`" — not an actual translucent
/// draw. `board_cache` below uses `with_backdrop` (opaque clear), which fixes the
/// drastic gray-under-cache mismatch, but match-3's CLAUDE.md documents a smaller
/// residual gap for near-opaque translucent draws specifically; precomputing avoids the
/// whole class from the start rather than discovering it the same way match-3 did.
fn lighten(c: Color32, amt: f32) -> Color32 {
    Color32::new(
        c.r + (1.0 - c.r) * amt,
        c.g + (1.0 - c.g) * amt,
        c.b + (1.0 - c.b) * amt,
        c.a,
    )
}

fn draw_bubble(cx: f32, cy: f32, radius: f32, color: Color, alpha: f32, highlight: bool) {
    if alpha <= 0.0 {
        return;
    }
    let base = color_rgb(color);
    let a = |c: Color32| Color32::new(c.r, c.g, c.b, c.a * alpha);
    draw_circle(cx, cy, radius, a(darken(base, 0.15)));
    draw_circle(cx, cy, radius * 0.88, a(base));
    if highlight {
        // Opaque gloss dots, same reasoning as `lighten`'s doc comment — see match-3's
        // `draw_gem` for the reference pattern this mirrors.
        draw_circle(
            cx - radius * 0.32,
            cy - radius * 0.34,
            radius * 0.22,
            lighten(base, 0.55),
        );
        draw_circle(
            cx - radius * 0.18,
            cy - radius * 0.22,
            radius * 0.11,
            lighten(base, 0.75),
        );
    }
}

fn death_line_y() -> f32 {
    BOARD_Y + RADIUS + (DEATH_ROW as f32 - 0.5) * game::ROW_HEIGHT
}

fn draw_board_frame() {
    draw_rectangle(
        BOARD_X - 1.0,
        BOARD_Y - 1.0,
        BOARD_W + 2.0,
        BOARD_H + 2.0,
        rgb(60, 60, 75),
    );
    draw_rectangle(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, rgb(20, 18, 26));
    let y = death_line_y();
    let dash = rgb(200, 70, 70);
    let mut x = BOARD_X;
    while x < BOARD_X + BOARD_W {
        draw_line(x, y, (x + 14.0).min(BOARD_X + BOARD_W), y, 2.0, dash);
        x += 22.0;
    }
}

fn draw_shooter(current: Color, next: Color) {
    draw_circle(SHOOTER_X, SHOOTER_Y, RADIUS * 1.15, rgb(50, 48, 58));
    draw_bubble(SHOOTER_X, SHOOTER_Y, RADIUS * 0.95, current, 1.0, true);
    let nx = SHOOTER_X + RADIUS * 2.4;
    let ny = SHOOTER_Y + RADIUS * 0.1;
    draw_circle(nx, ny, RADIUS * 0.62, rgb(40, 38, 48));
    draw_bubble(nx, ny, RADIUS * 0.5, next, 1.0, false);
    draw_text(
        "NEXT",
        nx - 16.0,
        ny + RADIUS * 0.95,
        14.0,
        rgb(150, 150, 165),
    );
}

fn draw_board_bubbles(board: &Board, skip: &std::collections::HashSet<(i32, i32)>) {
    for (&(col, row), &color) in &board.cells {
        if skip.contains(&(col, row)) {
            continue;
        }
        let (x, y) = cell_pixel(col, row);
        draw_bubble(x, y, RADIUS * 0.92, color, 1.0, true);
    }
}

/// The fully-settled frame: board frame + death line + every bubble + shooter/next
/// preview, no animation. What `board_cache` actually caches (`Idle`/`GameOver`).
fn draw_settled(board: &Board, current: Color, next: Color) {
    draw_board_frame();
    draw_board_bubbles(board, &Default::default());
    draw_shooter(current, next);
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

const DASH_LEN: f32 = 10.0;
const DASH_GAP: f32 = 8.0;

/// `path` is now sparse (a vertex only at the shooter, each wall bounce, and the
/// landing pixel — see `game::raymarch`'s doc comment), so a whole leg is typically one
/// long straight segment: alternating whole `path` segments (the old approach, back
/// when every segment was one short `RAY_STEP`) would draw either one solid line or one
/// gap with no dashing at all. Walk the polyline by actual on-screen distance instead,
/// alternating `DASH_LEN`/`DASH_GAP`-sized draws across segment boundaries — the dash
/// pattern no longer depends on how many vertices the underlying path happens to have.
fn draw_dashed_path(path: &[(f32, f32)], color: Color32, thickness: f32) {
    if path.len() < 2 {
        return;
    }
    let mut drawing = true;
    let mut remaining = DASH_LEN;
    for w in path.windows(2) {
        let (mut x0, mut y0) = w[0];
        let (x1, y1) = w[1];
        let mut seg_left = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        if seg_left <= f32::EPSILON {
            continue;
        }
        let (ux, uy) = ((x1 - x0) / seg_left, (y1 - y0) / seg_left);
        while seg_left > 0.0 {
            let step = remaining.min(seg_left);
            let (nx, ny) = (x0 + ux * step, y0 + uy * step);
            if drawing {
                draw_line(x0, y0, nx, ny, thickness, color);
            }
            x0 = nx;
            y0 = ny;
            seg_left -= step;
            remaining -= step;
            if remaining <= f32::EPSILON {
                drawing = !drawing;
                remaining = if drawing { DASH_LEN } else { DASH_GAP };
            }
        }
    }
}

fn draw_flying_live(pre_board: &Board, res: &Resolution, current: Color, next: Color, t: f32) {
    draw_board_frame();
    draw_board_bubbles(pre_board, &Default::default());
    draw_shooter(current, next);
    let path = &res.path;
    let base = color_rgb(res.color);
    let dim = Color32::new(base.r, base.g, base.b, 0.35);
    draw_dashed_path(path, dim, 2.0);
    if path.len() >= 2 {
        let f = t.clamp(0.0, 1.0) * (path.len() - 1) as f32;
        let i = (f.floor() as usize).min(path.len() - 2);
        let frac = f - i as f32;
        let (x0, y0) = path[i];
        let (x1, y1) = path[i + 1];
        let x = x0 + (x1 - x0) * frac;
        let y = y0 + (y1 - y0) * frac;
        draw_bubble(x, y, RADIUS * 0.92, res.color, 1.0, true);
    }
}

fn draw_popping_live(
    board_with_shot: &Board,
    res: &Resolution,
    current: Color,
    next: Color,
    t: f32,
) {
    draw_board_frame();
    let popped: std::collections::HashSet<(i32, i32)> = res.popped.iter().copied().collect();
    draw_board_bubbles(board_with_shot, &popped);
    let pulse = (t * std::f32::consts::TAU * 2.5).sin() * 0.5 + 0.5;
    let alpha = if t < 0.7 {
        0.5 + 0.5 * pulse
    } else {
        (1.0 - (t - 0.7) / 0.3).max(0.0)
    };
    for &pos in &res.popped {
        let (x, y) = cell_pixel(pos.0, pos.1);
        draw_bubble(x, y, RADIUS * 0.92, res.color, alpha, false);
    }
    draw_shooter(current, next);
}

/// Draws against `board_with_shot` (pre-descend), not the fully-`settled` post-move
/// board: `res.popped`/`res.floaters` are cell coordinates computed *before*
/// `descend_row` runs inside `resolve()` (see `Resolution`'s doc comment), so on a
/// shot that also triggers a descend, `settled` is one row further down than those
/// coordinates — pairing them together made the wall visibly jump a row mid-animation
/// and dropped floaters from the wrong height. `board_with_shot` was captured at the
/// same pre-descend moment as `popped`/`floaters`, so it's the consistent board to
/// draw the static wall against here; skip both cell sets since `board_with_shot`
/// itself never had them removed (only the real `self.board` was).
fn draw_falling_live(
    board_with_shot: &Board,
    res: &Resolution,
    current: Color,
    next: Color,
    t: f32,
) {
    draw_board_frame();
    let mut skip: std::collections::HashSet<(i32, i32)> = res.popped.iter().copied().collect();
    skip.extend(res.floaters.iter().copied());
    draw_board_bubbles(board_with_shot, &skip);
    let k = smoothstep(t);
    for &pos in &res.floaters {
        let Some(&color) = board_with_shot.cells.get(&pos) else {
            continue;
        };
        let (x, y) = cell_pixel(pos.0, pos.1);
        let fall_y = y + k * (BOARD_Y + BOARD_H - y + RADIUS * 2.0);
        draw_bubble(x, fall_y, RADIUS * 0.92, color, (1.0 - k).max(0.0), false);
    }
    draw_shooter(current, next);
}

fn flight_duration(path: &[(f32, f32)]) -> f32 {
    let len: f32 = path
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    (len / FLIGHT_SPEED).clamp(MIN_FLIGHT_DUR, MAX_FLIGHT_DUR)
}

// ── Session / episode plumbing ───────────────────────────────────────────────────

struct Session {
    game: Game,
    beam: solver::Beam,
}

impl Session {
    fn new(generation: u32) -> Self {
        Self {
            game: Game::new(generation),
            beam: solver::new_beam_search(),
        }
    }

    fn next_generation(&self) -> Self {
        Self::new(self.game.generation + 1)
    }

    fn choose_move(&mut self) -> game::Move {
        solver::choose_move(&mut self.beam, &self.game)
            .expect("Phase::Playing guarantees a legal move")
    }
}

// ── View: cosmetic playback of each shot's already-resolved `Resolution` ───────────

#[derive(PartialEq)]
enum StepPhase {
    Flying,
    Popping,
    Falling,
    Idle,
    GameOver,
}

struct View {
    phase: StepPhase,
    t: f32,
    pre_board: Board,
    board_with_shot: Board,
    settled: Board,
    resolution: Option<Resolution>,
    over_t: f32,
    /// Set for exactly one frame, the frame `phase` becomes `Idle` — the signal the
    /// render loop uses to `mark_dirty()` the board cache (the settled board just
    /// changed; every other frame while `Idle` should keep blitting the cached texture).
    settled_this_frame: bool,
}

impl View {
    fn new(session: &Session) -> Self {
        Self {
            phase: StepPhase::Idle,
            t: IDLE_DUR,
            pre_board: session.game.board.clone(),
            board_with_shot: session.game.board.clone(),
            settled: session.game.board.clone(),
            resolution: None,
            over_t: 0.0,
            settled_this_frame: true,
        }
    }

    fn enter_idle(&mut self) {
        self.phase = StepPhase::Idle;
        self.t = 0.0;
        self.settled_this_frame = true;
    }

    fn advance(&mut self, session: &mut Session, control: &mut control::Control, debug: bool) {
        if session.game.phase != Phase::Playing {
            control.episode_complete("bubble-shooter", session.game.score as i64);
            if debug {
                eprintln!(
                    "game_over phase={:?} score={} shots_used={} generation={}",
                    session.game.phase,
                    session.game.score,
                    session.game.shots_used,
                    session.game.generation + 1
                );
            }
            self.phase = StepPhase::GameOver;
            self.over_t = OVER_PAUSE;
            return;
        }

        let pre = session.game.board.clone();
        let mv = session.choose_move();
        let res = session.game.apply(mv);
        let mut with_shot = pre.clone();
        with_shot.cells.insert(mv.target, res.color);
        if debug {
            eprintln!(
                "shot target={:?} angle={:.1} popped={} floaters={} score_gained={} descended={} score={} shots_used={} generation={}",
                mv.target,
                mv.angle_deg,
                res.popped.len(),
                res.floaters.len(),
                res.score_gained,
                res.descended,
                session.game.score,
                session.game.shots_used,
                session.game.generation + 1
            );
        }
        self.pre_board = pre;
        self.board_with_shot = with_shot;
        self.settled = session.game.board.clone();
        self.resolution = Some(res);
        self.phase = StepPhase::Flying;
        self.t = 0.0;
    }

    fn tick(
        &mut self,
        session: &mut Session,
        control: &mut control::Control,
        dt: f32,
        debug: bool,
    ) {
        self.settled_this_frame = false;
        match self.phase {
            StepPhase::Flying => {
                let dur = flight_duration(&self.resolution.as_ref().unwrap().path);
                self.t += dt / dur;
                if self.t >= 1.0 {
                    if self.resolution.as_ref().unwrap().popped.is_empty() {
                        self.enter_idle();
                    } else {
                        self.phase = StepPhase::Popping;
                        self.t = 0.0;
                    }
                }
            }
            StepPhase::Popping => {
                self.t += dt / POP_DUR;
                if self.t >= 1.0 {
                    if self.resolution.as_ref().unwrap().floaters.is_empty() {
                        self.enter_idle();
                    } else {
                        self.phase = StepPhase::Falling;
                        self.t = 0.0;
                    }
                }
            }
            StepPhase::Falling => {
                self.t += dt / FALL_DUR;
                if self.t >= 1.0 {
                    self.enter_idle();
                }
            }
            StepPhase::Idle => {
                self.t += dt;
                if self.t >= IDLE_DUR {
                    self.advance(session, control, debug);
                }
            }
            StepPhase::GameOver => {}
        }
    }
}

// ── CLI args (native only — meaningless in a browser tab) ───────────────────────

struct CliArgs {
    debug: bool,
    once: bool,
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

fn print_result(session: &Session) {
    let Phase::Over(outcome) = session.game.phase else {
        unreachable!("print_result only called once the episode has ended");
    };
    let outcome = match outcome {
        Outcome::Won => "won",
        Outcome::Lost => "lost",
        Outcome::Survived => "survived",
    };
    let level = game::level_for(session.game.generation);
    println!(
        "result={outcome} score={} shots_used={} level=\"{}\" colors={} initial_rows={} generation={}",
        session.game.score,
        session.game.shots_used,
        level.name,
        level.color_count,
        level.initial_rows,
        session.game.generation + 1
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn run_headless(cli: CliArgs) -> ! {
    macroquad::rand::srand(screenshot::seed());
    let mut session = Session::new(0);

    loop {
        match session.game.phase {
            Phase::Playing => {
                let mv = session.choose_move();
                let res = session.game.apply(mv);
                if cli.debug {
                    eprintln!(
                        "shot target={:?} popped={} floaters={} score_gained={} descended={} score={} shots_used={} generation={}",
                        mv.target,
                        res.popped.len(),
                        res.floaters.len(),
                        res.score_gained,
                        res.descended,
                        session.game.score,
                        session.game.shots_used,
                        session.game.generation + 1
                    );
                }
            }
            Phase::Over(_) => {
                print_result(&session);
                if cli.once {
                    std::process::exit(0);
                }
                session = session.next_generation();
            }
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn conf() -> Conf {
    Conf {
        window_title: "Bubble Shooter".to_owned(),
        window_width: 600,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = parse_cli_args();
    if cli.no_ui {
        run_headless(cli);
    }
    macroquad::Window::from_config(conf(), amain(cli));
}

#[cfg(target_arch = "wasm32")]
fn main() {
    macroquad::Window::from_config(conf(), amain(parse_cli_args()));
}

async fn amain(cli: CliArgs) {
    let mut control = control::Control::new();
    rand::srand(control.seed());
    let mut session = Session::new(0);
    let mut view = View::new(&session);
    let mut shot = screenshot::Capture::from_env();
    // Set once the daily-challenge run ends (see `control::Control::daily_mode`) — the
    // board freezes on the `GameOver` overlay's final frame instead of advancing to the
    // next episode.
    let mut daily_done = false;

    render_cache::prewarm_glyphs(
        &[
            "NEXT",
            "BUBBLE SHOOTER",
            "score 000000",
            "level 0/0 - Wider Palette",
            "PAUSED",
            "x1.000",
        ],
        &[14, 15, 16, 18, 20, 26, 30],
    );

    // Same split match-3 uses: only the `Idle`/`GameOver` frames are genuinely
    // static between ticks (a shot is mid-flight/pop/fall animation nearly the whole
    // rest of the time) — those are the only phases that draw through the cache.
    // `with_supersample(2)` antialiases the cached content (its `sample_count: 0`
    // target gets no hardware AA); every animating frame is routed through the same
    // pipeline via `draw_fresh` below, so cached and live rasterize identically. See
    // `lib/render_cache`'s and match-3's `CLAUDE.md` for the full rationale — this is
    // a circle-packed board, so the AA-mismatch this guards against would be even more
    // visible here (curved silhouettes everywhere) than match-3's mostly-straight-edged
    // gem shapes.
    let mut board_cache = RenderCache::new(Rect::new(
        BOARD_X - 1.0,
        BOARD_Y - 1.0,
        BOARD_W + 2.0,
        BOARD_H + 2.0,
    ))
    .with_backdrop(rgb(60, 60, 75))
    .with_supersample(2);
    board_cache.mark_dirty();

    loop {
        control.handle_keys();
        let dt = control.scale(get_frame_time());

        if view.phase == StepPhase::GameOver {
            if cli.once {
                print_result(&session);
                std::process::exit(0);
            }
            view.over_t -= dt;
            if view.over_t <= 0.0 && !daily_done {
                if control.daily_mode() {
                    let Phase::Over(outcome) = session.game.phase else {
                        unreachable!("GameOver view only reached once the episode has ended");
                    };
                    let result_clause = match outcome {
                        Outcome::Won => {
                            format!("clear the board, ending at score {}", session.game.score)
                        }
                        Outcome::Lost => {
                            format!("lose it at score {}", session.game.score)
                        }
                        Outcome::Survived => {
                            format!(
                                "survive all {} shots at score {}",
                                session.game.shots_used, session.game.score
                            )
                        }
                    };
                    control::share_result(&control::daily_verdict_text(
                        "Bubble Shooter",
                        control::daily_puzzle_number(),
                        &result_clause,
                    ));
                    daily_done = true;
                } else {
                    session = session.next_generation();
                    view = View::new(&session);
                    board_cache.mark_dirty();
                }
            }
        } else {
            view.tick(&mut session, &mut control, dt, cli.debug);
            if view.settled_this_frame {
                board_cache.mark_dirty();
            }
        }

        clear_background(rgb(12, 12, 18));
        draw_hud(&session, &control);

        let animating = !matches!(view.phase, StepPhase::Idle | StepPhase::GameOver);
        if animating {
            board_cache.draw_fresh(|| match view.phase {
                StepPhase::Flying => draw_flying_live(
                    &view.pre_board,
                    view.resolution.as_ref().unwrap(),
                    session.game.current_color,
                    session.game.next_color,
                    view.t.min(1.0),
                ),
                StepPhase::Popping => draw_popping_live(
                    &view.board_with_shot,
                    view.resolution.as_ref().unwrap(),
                    session.game.current_color,
                    session.game.next_color,
                    view.t.min(1.0),
                ),
                StepPhase::Falling => draw_falling_live(
                    &view.board_with_shot,
                    view.resolution.as_ref().unwrap(),
                    session.game.current_color,
                    session.game.next_color,
                    view.t.min(1.0),
                ),
                StepPhase::Idle | StepPhase::GameOver => unreachable!(),
            });
        } else {
            board_cache.draw(|| {
                draw_settled(
                    &view.settled,
                    session.game.current_color,
                    session.game.next_color,
                )
            });
        }

        if view.phase == StepPhase::GameOver && !control.stream_mode() {
            draw_game_over(&session, view.over_t, control.daily_mode());
        }

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}

// ── HUD ───────────────────────────────────────────────────────────────────────

fn draw_hud(session: &Session, control: &control::Control) {
    let text = rgb(210, 210, 225);
    let dim = rgb(140, 140, 160);
    let good = rgb(120, 220, 140);

    draw_text("BUBBLE SHOOTER", 20.0, 40.0, 30.0, text);
    let shots_to_descend = session.game.shots_until_descend + 1;
    let line = format!(
        "score {}   next row in {} shot{}",
        session.game.score,
        shots_to_descend,
        if shots_to_descend == 1 { "" } else { "s" }
    );
    draw_text(&line, 20.0, 62.0, 16.0, good);

    let level = game::level_for(session.game.generation);
    let level_index = session.game.generation as usize % game::LEVELS.len() + 1;
    let level_line = format!(
        "level {}/{} - {}",
        level_index,
        game::LEVELS.len(),
        level.name
    );
    draw_text(&level_line, 20.0, 82.0, 15.0, dim);

    if !control.stream_mode() {
        let speed = control.label();
        let sd = measure_text(&speed, None, 18, 1.0);
        draw_text(&speed, 600.0 - 16.0 - sd.width, 40.0, 18.0, dim);
    }
}

fn outcome_text(outcome: Outcome) -> (&'static str, Color32) {
    match outcome {
        Outcome::Won => ("BOARD CLEARED!", rgb(120, 220, 140)),
        Outcome::Lost => ("BUBBLES REACHED THE LINE", rgb(230, 110, 100)),
        Outcome::Survived => ("SURVIVED THE RUN", rgb(120, 220, 140)),
    }
}

fn draw_game_over(session: &Session, over_t: f32, daily_mode: bool) {
    draw_rectangle(
        BOARD_X,
        BOARD_Y,
        BOARD_W,
        BOARD_H,
        Color32::new(0.0, 0.0, 0.0, 0.72),
    );
    let cx = BOARD_X + BOARD_W * 0.5;
    let cy = BOARD_Y + BOARD_H * 0.5;

    let Phase::Over(outcome) = session.game.phase else {
        return;
    };
    let (title, color) = outcome_text(outcome);
    let d = measure_text(title, None, 26, 1.0);
    draw_text(title, cx - d.width * 0.5, cy - 20.0, 26.0, color);

    let score_line = format!("score {}", session.game.score);
    let sd = measure_text(&score_line, None, 20, 1.0);
    draw_text(
        &score_line,
        cx - sd.width * 0.5,
        cy + 14.0,
        20.0,
        rgb(210, 210, 225),
    );

    // Daily-challenge runs freeze here rather than restarting (see
    // `control::Control::daily_mode`) — a countdown to a restart that never happens
    // would lie, and `over_t` itself has stopped counting down anyway.
    let sub = if daily_mode {
        "Today's run is over.".to_owned()
    } else {
        format!("Restarting in {:.0}...", over_t.max(0.0))
    };
    let subd = measure_text(&sub, None, 18, 1.0);
    draw_text(
        &sub,
        cx - subd.width * 0.5,
        cy + 42.0,
        18.0,
        rgb(210, 210, 225),
    );
}
