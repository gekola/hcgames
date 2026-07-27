use macroquad::prelude::*;
use render_cache::RenderCache;
use std::collections::HashSet;

mod game;
mod solver;

use game::{Board, FallEntry, Game, H, Outcome, Phase, Special, Tile, Variant, W, Wave};

const CELL: f32 = 62.0;
const BOARD_W: f32 = W as f32 * CELL;
const BOARD_H: f32 = H as f32 * CELL;
const PANEL_GAP: f32 = 40.0;
const PANEL_PAD: f32 = 14.0;
const PANEL_W: f32 = 200.0;
const PANEL_OUTER_W: f32 = PANEL_W + PANEL_PAD * 2.0;
const BOARD_X: f32 = (900.0 - (BOARD_W + PANEL_GAP + PANEL_OUTER_W)) / 2.0;
const BOARD_Y: f32 = 130.0;
const PANEL_OUTER_X: f32 = BOARD_X + BOARD_W + PANEL_GAP;
const PANEL_X: f32 = PANEL_OUTER_X + PANEL_PAD;

const SWAP_DUR: f32 = 0.22;
const FLASH_DUR: f32 = 0.22;
const FALL_BASE: f32 = 0.16;
const FALL_PER_ROW: f32 = 0.045;
const FALL_MAX: f32 = 0.65;
const IDLE_DUR: f32 = 0.35;
const OVER_PAUSE: f32 = 2.5;
const COMBO_BANNER_DUR: f32 = 1.1;

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

fn color_rgb(c: game::Color) -> Color {
    match c {
        game::Color::Red => rgb(224, 68, 68),
        game::Color::Orange => rgb(232, 142, 44),
        game::Color::Yellow => rgb(226, 202, 54),
        game::Color::Green => rgb(88, 190, 96),
        game::Color::Blue => rgb(72, 132, 232),
        game::Color::Purple => rgb(176, 92, 222),
    }
}

fn color_name(c: game::Color) -> &'static str {
    match c {
        game::Color::Red => "RED",
        game::Color::Orange => "ORANGE",
        game::Color::Yellow => "YELLOW",
        game::Color::Green => "GREEN",
        game::Color::Blue => "BLUE",
        game::Color::Purple => "PURPLE",
    }
}

fn darken(c: Color, amt: f32) -> Color {
    Color::new(c.r * (1.0 - amt), c.g * (1.0 - amt), c.b * (1.0 - amt), c.a)
}

fn lighten(c: Color, amt: f32) -> Color {
    Color::new(
        c.r + (1.0 - c.r) * amt,
        c.g + (1.0 - c.g) * amt,
        c.b + (1.0 - c.b) * amt,
        c.a,
    )
}

/// One silhouette per color — not just a color-coded fill, so the board reads even
/// without color perception (or at a glance/small size). Low-side polygons (triangle,
/// diamond, pentagon) look visually smaller than a hexagon/circle at the same
/// circumradius, hence the per-shape radius bump in `draw`.
#[derive(Clone, Copy)]
enum GemShape {
    Circle,
    Triangle,
    Diamond,
    Pentagon,
    Hexagon,
    Star,
}

impl GemShape {
    fn for_color(c: game::Color) -> GemShape {
        match c {
            game::Color::Red => GemShape::Circle,
            game::Color::Orange => GemShape::Triangle,
            game::Color::Yellow => GemShape::Star,
            game::Color::Green => GemShape::Pentagon,
            game::Color::Blue => GemShape::Hexagon,
            game::Color::Purple => GemShape::Diamond,
        }
    }

    fn draw(self, cx: f32, cy: f32, r: f32, color: Color) {
        match self {
            GemShape::Circle => draw_poly(cx, cy, 24, r, 0.0, color),
            GemShape::Triangle => draw_poly(cx, cy, 3, r * 1.2, -90.0, color),
            GemShape::Diamond => draw_poly(cx, cy, 4, r * 1.12, 45.0, color),
            GemShape::Pentagon => draw_poly(cx, cy, 5, r * 1.08, -90.0, color),
            GemShape::Hexagon => draw_poly(cx, cy, 6, r, 90.0, color),
            // A hexagram (two overlapping triangles) rather than a true 5-point star —
            // macroquad has no star primitive, and this reads as a star at tile scale.
            GemShape::Star => draw_hexagram(cx, cy, r * 1.2, color),
        }
    }
}

/// Fills a hexagram (two equilateral triangles, ±90° rotated — the same shape
/// `GemShape::Star` used to draw as two separate overlapping `draw_poly` calls) as one
/// non-overlapping 12-vertex fan instead. Two alpha-blended triangles double-blend their
/// shared hexagonal core, which reads as a visibly *brighter* patch at the star's center
/// whenever `color` is translucent (any tile fade/flash) — reported as "the yellow gem
/// looks transparent with a bright overlap." A single fill has no overlap to double-blend
/// regardless of alpha, so this is a real fix, not a `RenderCache`-style opaque-precompute
/// workaround (this bug hits every draw at alpha<1, cached or not).
///
/// The 12 boundary vertices alternate the outer star tips (radius `r`, at 30°+60°k — where
/// the two triangles' own vertices already sit, see the rotations above) with the inner
/// concave points where their edges cross (radius `r / sqrt(3)`, the standard hexagram
/// inradius/circumradius ratio, at 60°k). A center-to-boundary triangle fan fills this
/// correctly in one pass because a hexagram is star-shaped (every boundary point is
/// visible from the center along a straight line inside the shape).
fn draw_hexagram(cx: f32, cy: f32, r: f32, color: Color) {
    let inner = r / 3f32.sqrt();
    let verts: Vec<Vec2> = (0..12)
        .map(|k| {
            let ang = k as f32 * std::f32::consts::PI / 6.0;
            let rad = if k % 2 == 0 { inner } else { r };
            vec2(cx + ang.cos() * rad, cy + ang.sin() * rad)
        })
        .collect();
    let center = vec2(cx, cy);
    for i in 0..12 {
        draw_triangle(center, verts[i], verts[(i + 1) % 12], color);
    }
}

/// A faceted gem: a darker backing shape (reads as an outline/shadow rim), the main
/// color slightly inset and nudged down, and (if `highlight`) a soft gloss streak
/// toward the upper-left — same layering for every `GemShape`.
fn draw_gem(cx: f32, cy: f32, size: f32, color: game::Color, alpha: f32, highlight: bool) {
    let a = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);
    let base = color_rgb(color);
    let shape = GemShape::for_color(color);
    let r = size * 0.5;
    shape.draw(cx, cy, r, a(darken(base, 0.12)));
    shape.draw(cx, cy - size * 0.02, r * 0.9, a(base));
    // A soft diagonal gloss streak — three small overlapping, fading circles along a
    // line toward the upper-left corner, not one hard-edged dot. A single circle (in
    // any size/position tried) kept reading as an artificial "sticker," and a second
    // tinted circle underneath it read as a concentric bullseye (see git history of
    // this function for both). Three faded circles in a row blend into a soft streak
    // instead. Kept conservatively inside even a triangle/star's tight inradius
    // (~0.3 * size) so it can't spill onto the dark cell background outside the shape.
    //
    // Drawn as *opaque* circles in a color pre-blended (in Rust, not via GPU alpha) to
    // what "white at alpha `al` over `base`" would look like — not translucent circles
    // relying on the GPU to blend them. This region gets cached by `RenderCache` (see
    // `draw_board_static`), which renders into an offscreen target that starts fully
    // transparent rather than the opaque screen framebuffer everything else draws to;
    // alpha-blending translucent content onto that starting-transparent destination
    // came out visibly grayer than the identical draw calls do live (confirmed:
    // reported as gray specifically "between moves," i.e. only while the `Idle`-phase
    // cached texture — not the live Swap/Flash/Fall draws — was on screen). match-3 is
    // the first cached region in this workspace with genuinely translucent content
    // inside it; every other game's cached region is fully opaque, which is presumably
    // why this never came up before. Precomputing the blend and drawing opaque sidesteps
    // whatever that render-target blending discrepancy actually is, rather than
    // chasing it — the two draws are mathematically the same operation regardless.
    if highlight {
        for &(dx, dy, rad, al) in &[
            (0.13f32, 0.16f32, 0.075f32, 0.10f32),
            (0.08, 0.10, 0.09, 0.16),
            (0.03, 0.04, 0.075, 0.10),
        ] {
            draw_circle(
                cx - size * dx,
                cy - size * dy,
                size * rad,
                lighten(base, al),
            );
        }
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cell_xy(row: f32, col: f32) -> (f32, f32) {
    (BOARD_X + col * CELL, BOARD_Y + row * CELL)
}

/// Draws one tile at board coordinates `(row, col)` (fractional row allowed, for
/// falling/sliding animation) — `alpha` fades it (clear/spawn transitions), `scale`
/// grows/shrinks it about its own center (spawn pop-in / clear pop-out), `highlight`
/// controls the gem's facet + specular glint independently of `alpha` (see `draw_gem`).
fn draw_tile(row: f32, col: f32, tile: Tile, alpha: f32, scale: f32, highlight: bool) {
    if alpha <= 0.0 || scale <= 0.0 {
        return;
    }
    let (x, y) = cell_xy(row, col);
    let pad = CELL * 0.08 + CELL * 0.5 * (1.0 - scale);
    let size = CELL - pad * 2.0;
    let cx = x + CELL * 0.5;
    let cy = y + CELL * 0.5;

    let a = |c: Color| Color::new(c.r, c.g, c.b, c.a * alpha);

    match tile {
        Tile::Plain(color) => {
            draw_gem(cx, cy, size, color, alpha, highlight);
        }
        Tile::Bonus(color, special) => {
            draw_gem(cx, cy, size, color, alpha, highlight);
            // Gated on `highlight` and drawn *opaque* (pre-blended in Rust, same
            // `lighten` trick `draw_gem`'s gloss streak uses — see its doc comment) rather
            // than a translucent white faded by `alpha`: this region gets cached by
            // `RenderCache` (an offscreen target that starts transparent, not the opaque
            // screen framebuffer everything else draws to), and a translucent stripe/ring
            // drawn into it composited visibly grayer than the identical live draw —
            // reported as the bonus tile markers flickering gray between moves, i.e. only
            // while the cached `Idle`-phase texture (not the live Swap/Flash/Fall draws)
            // was on screen, the same symptom the gloss streak had. Every call site that
            // passes `highlight: true` already draws at `alpha: 1.0` (a settled,
            // non-animating tile), so gating on `highlight` loses no fade that mattered.
            if highlight {
                let stripe = lighten(color_rgb(color), 0.55);
                match special {
                    Special::RowClear => {
                        for i in [0.32, 0.68] {
                            draw_rectangle(x + pad, y + pad + size * i - 1.5, size, 3.0, stripe);
                        }
                    }
                    Special::ColClear => {
                        for i in [0.32, 0.68] {
                            draw_rectangle(x + pad + size * i - 1.5, y + pad, 3.0, size, stripe);
                        }
                    }
                    Special::Wrapped => {
                        // A circular halo rather than a square ring — reads cleanly
                        // around any of `GemShape`'s silhouettes, not just the square it
                        // used to be drawn as.
                        draw_poly_lines(cx, cy, 28, size * 0.52, 0.0, 3.0, stripe);
                        draw_poly_lines(cx, cy, 28, size * 0.36, 0.0, 1.5, stripe);
                    }
                }
            }
        }
        Tile::ColorBomb => {
            // The old single-tone fill (20,20,26) was nearly indistinguishable from the
            // board's own cell background (24,22,30) — with no gem shape and only 6 small
            // sparkle dots on top, the "orb" itself was essentially invisible, so the tile
            // read as a loose scatter of dots rather than a solid bomb ("crumbling").
            // Two-toned the same way every colored gem already is (a darker rim, a
            // distinctly lighter inset) instead of one near-invisible fill.
            draw_circle(cx, cy, size * 0.5, a(rgb(48, 45, 62)));
            draw_circle(cx, cy, size * 0.42, a(rgb(66, 62, 84)));
            let sparkle_colors = game::Color::ALL;
            for (i, &c) in sparkle_colors.iter().enumerate() {
                let ang = i as f32 / sparkle_colors.len() as f32 * std::f32::consts::TAU;
                let r = size * 0.32;
                draw_circle(
                    cx + ang.cos() * r,
                    cy + ang.sin() * r,
                    size * 0.09,
                    a(color_rgb(c)),
                );
            }
        }
        Tile::Ingredient => {
            draw_poly(
                cx,
                cy - size * 0.06,
                3,
                size * 0.42,
                0.0,
                a(rgb(235, 245, 250)),
            );
            draw_circle(cx, cy + size * 0.18, size * 0.30, a(rgb(235, 245, 250)));
            draw_circle(
                cx - size * 0.08,
                cy + size * 0.10,
                size * 0.08,
                a(Color::new(1.0, 1.0, 1.0, 0.7)),
            );
            draw_poly_lines(
                cx,
                cy + size * 0.02,
                24,
                size * 0.44,
                0.0,
                2.0,
                a(rgb(150, 180, 190)),
            );
        }
    }
}

fn draw_board_frame() {
    draw_rectangle(
        BOARD_X - 1.0,
        BOARD_Y - 1.0,
        BOARD_W + 2.0,
        BOARD_H + 2.0,
        rgb(60, 60, 75),
    );
    draw_rectangle(BOARD_X, BOARD_Y, BOARD_W, BOARD_H, rgb(24, 22, 30));
    let grid = rgb(38, 36, 48);
    for c in 1..W {
        let x = BOARD_X + c as f32 * CELL;
        draw_line(x, BOARD_Y, x, BOARD_Y + BOARD_H, 1.0, grid);
    }
    for r in 1..H {
        let y = BOARD_Y + r as f32 * CELL;
        draw_line(BOARD_X, y, BOARD_X + BOARD_W, y, 1.0, grid);
    }
}

fn draw_jelly_underlay(board: &Board) {
    for r in 0..H {
        for c in 0..W {
            if board.jelly[r][c] > 0 {
                let (x, y) = cell_xy(r as f32, c as f32);
                draw_rectangle(
                    x + 3.0,
                    y + 3.0,
                    CELL - 6.0,
                    CELL - 6.0,
                    Color::new(0.55, 0.85, 0.95, 0.28),
                );
            }
        }
    }
}

/// The fully-settled board: frame + jelly underlay + every tile, no animation. What
/// `board_cache` actually caches — see the `Idle`/`GameOver` arms of the render loop.
fn draw_board_static(board: &Board) {
    draw_board_frame();
    draw_jelly_underlay(board);
    for r in 0..H {
        for c in 0..W {
            draw_tile(r as f32, c as f32, board.tiles[r][c], 1.0, 1.0, true);
        }
    }
}

fn draw_swap_live(pre: &Board, a: (usize, usize), b: (usize, usize), t: f32) {
    draw_board_frame();
    draw_jelly_underlay(pre);
    for r in 0..H {
        for c in 0..W {
            if (r, c) != a && (r, c) != b {
                draw_tile(r as f32, c as f32, pre.tiles[r][c], 1.0, 1.0, true);
            }
        }
    }
    let k = smoothstep(t);
    let (ar, ac) = (a.0 as f32, a.1 as f32);
    let (br, bc) = (b.0 as f32, b.1 as f32);
    draw_tile(
        ar + (br - ar) * k,
        ac + (bc - ac) * k,
        pre.tiles[a.0][a.1],
        1.0,
        1.0,
        true,
    );
    draw_tile(
        br + (ar - br) * k,
        bc + (ac - bc) * k,
        pre.tiles[b.0][b.1],
        1.0,
        1.0,
        true,
    );
}

fn draw_flash_live(wave: &Wave, t: f32) {
    draw_board_frame();
    draw_jelly_underlay(&wave.board_before);
    let cleared: HashSet<(usize, usize)> = wave.cleared.iter().copied().collect();
    let spawned: HashSet<(usize, usize)> = wave.spawned.iter().copied().collect();
    for r in 0..H {
        for c in 0..W {
            let pos = (r, c);
            if cleared.contains(&pos) {
                // A smooth pulse (not a hard on/off toggle) for the first 60% of the
                // flash, then a clean fade to nothing over the back 40%. The facet
                // highlight and glint are suppressed for this whole phase regardless
                // (see `draw_gem`) — they never participate in the pulse.
                let alpha = if t < 0.6 {
                    let pulse = (t * std::f32::consts::TAU * 2.5).sin() * 0.5 + 0.5;
                    0.55 + 0.45 * pulse
                } else {
                    (1.0 - (t - 0.6) / 0.4).max(0.0)
                };
                draw_tile(
                    r as f32,
                    c as f32,
                    wave.board_before.tiles[r][c],
                    alpha,
                    1.0,
                    false,
                );
            } else if spawned.contains(&pos) {
                draw_tile(
                    r as f32,
                    c as f32,
                    wave.board_before.tiles[r][c],
                    (1.0 - t).max(0.0),
                    1.0,
                    false,
                );
                draw_tile(
                    r as f32,
                    c as f32,
                    wave.board_after.tiles[r][c],
                    t,
                    0.4 + 0.6 * t,
                    false,
                );
            } else {
                draw_tile(
                    r as f32,
                    c as f32,
                    wave.board_before.tiles[r][c],
                    1.0,
                    1.0,
                    true,
                );
            }
        }
    }
}

fn draw_fall_live(wave: &Wave, t: f32) {
    draw_board_frame();
    draw_jelly_underlay(&wave.board_after);
    let k = smoothstep(t);
    let moving: HashSet<(usize, usize)> = wave.falls.iter().map(|f| (f.to_row, f.col)).collect();
    for r in 0..H {
        for c in 0..W {
            if !moving.contains(&(r, c)) {
                draw_tile(
                    r as f32,
                    c as f32,
                    wave.board_after.tiles[r][c],
                    1.0,
                    1.0,
                    true,
                );
            }
        }
    }
    for f in &wave.falls {
        let row = f.from_row as f32 + (f.to_row as f32 - f.from_row as f32) * k;
        draw_tile(row, f.col as f32, f.tile, 1.0, 1.0, true);
    }
}

fn max_fall_rows(wave: &Wave) -> i32 {
    wave.falls
        .iter()
        .map(|f: &FallEntry| f.to_row as i32 - f.from_row)
        .max()
        .unwrap_or(1)
}

fn fall_duration(wave: &Wave) -> f32 {
    (FALL_BASE + FALL_PER_ROW * max_fall_rows(wave) as f32).min(FALL_MAX)
}

// ── Variant cycling (V hotkey) ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum VariantMode {
    Score,
    Jelly,
    Ingredients,
    Mystery,
    Timed,
    /// Steps through the hand-tuned `game::LEVELS` line instead of one fixed `Variant`
    /// — deliberately *not* one of `Auto`'s generation%4 rotation targets (see root
    /// CLAUDE.md's "In-game controls" note on Auto never landing on an explicit-select
    /// mode by itself); only reachable via this cycle or `--variant levels`. `Session`
    /// special-cases this arm everywhere below rather than routing it through
    /// `variant()`/`name()`, since a level's `Variant` varies by `level_index`, not by
    /// `generation` alone.
    Levels,
    Auto,
}

impl VariantMode {
    fn next(self) -> Self {
        match self {
            VariantMode::Score => VariantMode::Jelly,
            VariantMode::Jelly => VariantMode::Ingredients,
            VariantMode::Ingredients => VariantMode::Mystery,
            VariantMode::Mystery => VariantMode::Timed,
            VariantMode::Timed => VariantMode::Levels,
            VariantMode::Levels => VariantMode::Auto,
            VariantMode::Auto => VariantMode::Score,
        }
    }

    fn variant(self, generation: u32) -> Variant {
        match self {
            VariantMode::Score => Variant::Score,
            VariantMode::Jelly => Variant::Jelly,
            VariantMode::Ingredients => Variant::Ingredients,
            VariantMode::Mystery => Variant::Mystery,
            VariantMode::Timed => Variant::Timed,
            VariantMode::Auto => match generation % 5 {
                0 => Variant::Score,
                1 => Variant::Jelly,
                2 => Variant::Ingredients,
                3 => Variant::Mystery,
                _ => Variant::Timed,
            },
            // Never actually read — `Session` builds `Levels` games via `Game::new_level`
            // and `game::LEVELS[level_index]` directly, bypassing this function.
            VariantMode::Levels => Variant::Score,
        }
    }

    fn name(self, generation: u32) -> &'static str {
        match self.variant(generation) {
            Variant::Score => "score attack",
            Variant::Jelly => "jelly clear",
            Variant::Ingredients => "collection",
            Variant::Mystery => "color hunt",
            Variant::Timed => "timed",
        }
    }

    fn label(self) -> &'static str {
        match self {
            VariantMode::Auto => " (auto)",
            _ => "",
        }
    }
}

struct Session {
    mode: VariantMode,
    /// Only meaningful when `mode == VariantMode::Levels` — index into `game::LEVELS`
    /// and consecutive-loss count at that index. Unlike the other modes' `Variant`,
    /// which `variant(generation)` derives statelessly, "which level" and "how many
    /// times has the bot failed it" can't be derived from `generation` alone (a stuck
    /// level gets replayed across several generations before advancing), so they're
    /// tracked here and carried across `next_generation` calls instead.
    level_index: usize,
    level_attempts: u32,
    game: Game,
}

impl Session {
    fn new(mode: VariantMode, generation: u32) -> Self {
        if mode == VariantMode::Levels {
            return Self {
                mode,
                level_index: 0,
                level_attempts: 0,
                game: Game::new_level(game::LEVELS[0], generation),
            };
        }
        Self {
            mode,
            level_index: 0,
            level_attempts: 0,
            game: Game::new(mode.variant(generation), generation),
        }
    }

    /// Starts the next episode. For `Levels`, this is where win/stuck is judged: a win
    /// or hitting `game::LEVEL_STUCK_LIMIT` consecutive losses advances to the next
    /// level (wrapping past the end of the line back to the start); otherwise the same
    /// level is replayed with a fresh board.
    fn next_generation(&self) -> Self {
        let generation = self.game.generation + 1;
        if self.mode == VariantMode::Levels {
            let won = matches!(self.game.phase, Phase::Over(Outcome::Won));
            let stuck = self.level_attempts + 1 >= game::LEVEL_STUCK_LIMIT;
            let (level_index, level_attempts) = if won || stuck {
                ((self.level_index + 1) % game::LEVELS.len(), 0)
            } else {
                (self.level_index, self.level_attempts + 1)
            };
            return Self {
                mode: self.mode,
                level_index,
                level_attempts,
                game: Game::new_level(game::LEVELS[level_index], generation),
            };
        }
        Self::new(self.mode, generation)
    }

    fn switch_variant(&self) -> Self {
        Self::new(self.mode.next(), self.game.generation + 1)
    }
}

// ── View: cosmetic playback of each move's already-resolved `Resolution` ───────────────

#[derive(PartialEq)]
enum StepPhase {
    Swap,
    Flash(usize),
    Fall(usize),
    Idle,
    GameOver,
}

struct View {
    phase: StepPhase,
    t: f32,
    settled: Board,
    pre_swap: Board,
    resolution: Option<game::Resolution>,
    combo_banner_t: f32,
    over_t: f32,
}

impl View {
    fn new(session: &Session) -> Self {
        Self {
            phase: StepPhase::Idle,
            t: IDLE_DUR,
            settled: session.game.board.clone(),
            pre_swap: session.game.board.clone(),
            resolution: None,
            combo_banner_t: 0.0,
            over_t: 0.0,
        }
    }

    /// Picks (via the solver) and starts animating the next move, or switches to the
    /// `GameOver` overlay if the current episode has already ended.
    fn advance(&mut self, session: &mut Session, control: &mut control::Control, debug: bool) {
        if session.game.phase != Phase::Playing {
            control.episode_complete("match-3", session.game.score as i64);
            if debug {
                eprintln!(
                    "game_over variant={:?} phase={:?} score={} moves_used={} generation={}",
                    session.game.variant,
                    session.game.phase,
                    session.game.score,
                    session.game.moves_used,
                    session.game.generation + 1
                );
            }
            self.phase = StepPhase::GameOver;
            self.over_t = OVER_PAUSE;
            return;
        }

        let pre = session.game.board.clone();
        let mv =
            solver::choose_move(&session.game).expect("Phase::Playing guarantees a legal move");
        let res = session.game.apply(mv);
        if debug {
            eprintln!(
                "swap a={:?} b={:?} combo={} waves={} score_gained={} score={} moves_used={} generation={}",
                mv.a,
                mv.b,
                res.combo,
                res.waves.len(),
                res.score_gained,
                session.game.score,
                session.game.moves_used,
                session.game.generation + 1
            );
        }
        if res.combo {
            self.combo_banner_t = COMBO_BANNER_DUR;
        }
        self.pre_swap = pre;
        self.resolution = Some(res);
        self.phase = StepPhase::Swap;
        self.t = 0.0;
    }

    /// Advances the current animation phase by `dt`, transitioning through
    /// Swap -> (Flash -> Fall) per wave -> Idle -> next move, or driving the
    /// `GameOver` overlay's restart countdown. Returns `true` once, the frame the
    /// episode's outcome is finalized and `--once` should print+exit.
    fn tick(
        &mut self,
        session: &mut Session,
        control: &mut control::Control,
        dt: f32,
        debug: bool,
    ) {
        if self.combo_banner_t > 0.0 {
            self.combo_banner_t = (self.combo_banner_t - dt).max(0.0);
        }
        match self.phase {
            StepPhase::Swap => {
                self.t += dt / SWAP_DUR;
                if self.t >= 1.0 {
                    let wave0_before = self.resolution.as_ref().unwrap().waves[0]
                        .board_before
                        .clone();
                    self.settled = wave0_before;
                    self.phase = StepPhase::Flash(0);
                    self.t = 0.0;
                }
            }
            StepPhase::Flash(i) => {
                self.t += dt / FLASH_DUR;
                if self.t >= 1.0 {
                    self.phase = StepPhase::Fall(i);
                    self.t = 0.0;
                }
            }
            StepPhase::Fall(i) => {
                let dur = fall_duration(&self.resolution.as_ref().unwrap().waves[i]);
                self.t += dt / dur;
                if self.t >= 1.0 {
                    let waves = &self.resolution.as_ref().unwrap().waves;
                    self.settled = waves[i].board_after.clone();
                    if i + 1 < waves.len() {
                        self.phase = StepPhase::Flash(i + 1);
                    } else {
                        self.phase = StepPhase::Idle;
                    }
                    self.t = 0.0;
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
                        "--variant requires a value: score, jelly, ingredients, mystery, timed, levels, or auto"
                    );
                    std::process::exit(2);
                });
                variant = Some(match v.as_str() {
                    "score" => VariantMode::Score,
                    "jelly" => VariantMode::Jelly,
                    "ingredients" => VariantMode::Ingredients,
                    "mystery" => VariantMode::Mystery,
                    "timed" => VariantMode::Timed,
                    "levels" => VariantMode::Levels,
                    "auto" => VariantMode::Auto,
                    other => {
                        eprintln!(
                            "unknown --variant value '{other}': expected score, jelly, ingredients, mystery, timed, levels, or auto"
                        );
                        std::process::exit(2);
                    }
                });
            }
            other => {
                eprintln!(
                    "unknown argument '{other}' (expected --debug, --once, --no-ui, --variant <score|jelly|ingredients|mystery|timed|levels|auto>)"
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
    let Phase::Over(outcome) = session.game.phase else {
        unreachable!("print_result only called once the episode has ended");
    };
    let outcome = match outcome {
        Outcome::Won => "won",
        Outcome::OutOfMoves => "out_of_moves",
        Outcome::TimeUp => "time_up",
    };
    let level_suffix = if session.mode == VariantMode::Levels {
        format!(
            " level={} attempts={}",
            session.level_index + 1,
            session.level_attempts + 1
        )
    } else {
        String::new()
    };
    let mystery_goals = session
        .game
        .mystery_goals
        .iter()
        .map(|g| format!("{:?}:{}/{}", g.color, g.collected, g.target))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "result={outcome} variant={:?} score={} moves_used={} jelly_remaining={} ingredients_collected={}/{} mystery_goals=[{mystery_goals}] generation={} reshuffles={}{level_suffix}",
        session.game.variant,
        session.game.score,
        session.game.moves_used,
        session.game.jelly_remaining,
        session.game.ingredients_collected,
        session.game.ingredients_target,
        session.game.generation + 1,
        session.game.reshuffles
    );
}

/// One full move-cycle's worth of animation in the windowed version (swap + however
/// many cascade waves + the idle beat before the next move) is roughly a second — used
/// only to give the `Timed` variant's countdown something to tick against in headless
/// mode, where there's no real per-frame `dt` to drive it (see CLAUDE.md's
/// `run_headless`/virtual-`dt` note; unlike an animation-paced game this one is
/// move-paced, so a per-move chunk stands in for a per-frame one).
#[cfg(not(target_arch = "wasm32"))]
const HEADLESS_SECONDS_PER_MOVE: f32 = 1.0;

#[cfg(not(target_arch = "wasm32"))]
fn run_headless(cli: CliArgs) -> ! {
    macroquad::rand::srand(screenshot::seed());
    let mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut session = Session::new(mode, 0);

    loop {
        match session.game.phase {
            Phase::Playing => {
                let mv = solver::choose_move(&session.game)
                    .expect("Phase::Playing guarantees a legal move");
                let res = session.game.apply(mv);
                if session.game.variant == Variant::Timed {
                    session.game.tick_time(HEADLESS_SECONDS_PER_MOVE);
                }
                if cli.debug {
                    eprintln!(
                        "swap a={:?} b={:?} combo={} score={} moves_used={} generation={}",
                        mv.a,
                        mv.b,
                        res.combo,
                        session.game.score,
                        session.game.moves_used,
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
        window_title: "Match 3".to_owned(),
        window_width: 900,
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
    rand::srand(screenshot::seed());
    let mode = cli.variant.unwrap_or(VariantMode::Auto);
    let mut session = Session::new(mode, 0);
    let mut view = View::new(&session);
    let mut shot = screenshot::Capture::from_env();
    let mut control = control::Control::new();

    // The board is genuinely mid-animation (swap slide, clear flash, gravity fall) for
    // nearly the entire time a move is resolving — there's no static content to cache
    // during that stretch, same reasoning `game2048` documents for its own slide/merge
    // animation. The `Idle` beat between moves (and the `GameOver` overlay) *is* fully
    // static, though, so those are the only phases that actually draw through the
    // cache — see the `animating` branch below, mirroring game2048's split exactly.
    // `with_backdrop` matches `draw_board_frame`'s own border fill (the first thing its
    // closure draws, covering this exact rect) — see `RenderCache::with_backdrop`'s doc
    // comment for why this is the general fix for every translucent draw inside
    // `draw_board_static` (gem gloss streak, bonus-tile stripes/ring, jelly underlay)
    // rather than the opaque-precompute workaround each of those needed individually
    // before this existed.
    let mut board_cache = RenderCache::new(Rect::new(
        BOARD_X - 1.0,
        BOARD_Y - 1.0,
        BOARD_W + 2.0,
        BOARD_H + 2.0,
    ))
    .with_backdrop(rgb(60, 60, 75));
    board_cache.mark_dirty();

    loop {
        control.handle_keys();
        let dt = control.scale(get_frame_time());

        if is_key_pressed(KeyCode::V) || control.variant_swipe() {
            session = session.switch_variant();
            view = View::new(&session);
            board_cache.mark_dirty();
        }

        if view.phase == StepPhase::GameOver {
            if cli.once {
                print_result(&session);
                std::process::exit(0);
            }
            view.over_t -= dt;
            if view.over_t <= 0.0 {
                session = session.next_generation();
                view = View::new(&session);
                board_cache.mark_dirty();
            }
        } else {
            if session.game.variant == Variant::Timed {
                session.game.tick_time(dt);
            }
            view.tick(&mut session, &mut control, dt, cli.debug);
        }

        clear_background(rgb(15, 15, 20));
        draw_hud(&session, &view, &control);

        let animating = !matches!(view.phase, StepPhase::Idle | StepPhase::GameOver);
        if animating {
            match view.phase {
                StepPhase::Swap => {
                    let mv = view.resolution.as_ref().unwrap().mv;
                    draw_swap_live(&view.pre_swap, mv.a, mv.b, view.t.min(1.0));
                }
                StepPhase::Flash(i) => {
                    draw_flash_live(&view.resolution.as_ref().unwrap().waves[i], view.t.min(1.0));
                }
                StepPhase::Fall(i) => {
                    draw_fall_live(&view.resolution.as_ref().unwrap().waves[i], view.t.min(1.0));
                }
                StepPhase::Idle | StepPhase::GameOver => unreachable!(),
            }
            board_cache.mark_dirty();
        } else {
            board_cache.draw(|| draw_board_static(&view.settled));
        }

        if view.combo_banner_t > 0.0 && !control.stream_mode() {
            draw_combo_banner(view.combo_banner_t);
        }
        if view.phase == StepPhase::GameOver && !control.stream_mode() {
            draw_game_over(&session, view.over_t);
        }

        shot.tick();
        screenshot::handle_hotkey();
        next_frame().await;
    }
}

// ── HUD ───────────────────────────────────────────────────────────────────────

fn draw_combo_banner(t: f32) {
    let alpha = (t / COMBO_BANNER_DUR).min(1.0);
    let text = "COMBO!";
    let size = 34.0;
    let d = measure_text(text, None, size as u16, 1.0);
    let cx = BOARD_X + BOARD_W * 0.5;
    let y = BOARD_Y + 30.0 - (1.0 - alpha) * 14.0;
    draw_text(
        text,
        cx - d.width * 0.5,
        y,
        size,
        Color::new(1.0, 0.85, 0.3, alpha),
    );
}

fn draw_hud(session: &Session, view: &View, control: &control::Control) {
    let text = rgb(210, 210, 225);
    let dim = rgb(140, 140, 160);
    let good = rgb(120, 220, 140);

    draw_text("MATCH 3", BOARD_X, 46.0, 34.0, text);
    let mode_label = if session.mode == VariantMode::Levels {
        let level = &game::LEVELS[session.level_index % game::LEVELS.len()];
        if session.level_attempts > 0 {
            format!(
                "level {} — {} (attempt {}/{})",
                session.level_index + 1,
                level.name,
                session.level_attempts + 1,
                game::LEVEL_STUCK_LIMIT
            )
        } else {
            format!("level {} — {}", session.level_index + 1, level.name)
        }
    } else {
        format!(
            "{}{}",
            session.mode.name(session.game.generation),
            session.mode.label()
        )
    };
    draw_text(&mode_label, BOARD_X, 72.0, 18.0, dim);

    if !control.stream_mode() {
        let speed = control.label();
        let sd = measure_text(&speed, None, 20, 1.0);
        draw_text(&speed, 900.0 - 20.0 - sd.width, 46.0, 20.0, dim);
    }

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
        rgb(24, 22, 30),
    );

    let mut y = BOARD_Y + 30.0;
    let line = |label: &str, value: &str, color: Color, y: &mut f32| {
        draw_text(label, PANEL_X, *y, 16.0, dim);
        *y += 22.0;
        draw_text(value, PANEL_X, *y, 26.0, color);
        *y += 40.0;
    };

    let g = &session.game;
    match g.variant {
        Variant::Score => {
            line(
                "SCORE / TARGET",
                &format!("{} / {}", g.score, g.score_target),
                text,
                &mut y,
            );
            line("MOVES LEFT", &g.remaining_moves().to_string(), good, &mut y);
        }
        Variant::Jelly => {
            line("JELLY LEFT", &g.jelly_remaining.to_string(), good, &mut y);
            line("MOVES LEFT", &g.remaining_moves().to_string(), text, &mut y);
            line("SCORE", &g.score.to_string(), dim, &mut y);
        }
        Variant::Ingredients => {
            line(
                "COLLECTED",
                &format!("{} / {}", g.ingredients_collected, g.ingredients_target),
                good,
                &mut y,
            );
            line("MOVES LEFT", &g.remaining_moves().to_string(), text, &mut y);
            line("SCORE", &g.score.to_string(), dim, &mut y);
        }
        Variant::Timed => {
            line(
                "TIME LEFT",
                &format!("{:.0}s", g.time_remaining),
                good,
                &mut y,
            );
            line("SCORE", &g.score.to_string(), text, &mut y);
        }
        Variant::Mystery => {
            for goal in &g.mystery_goals {
                line(
                    &format!("{} CLEARED", color_name(goal.color)),
                    &format!("{} / {}", goal.collected, goal.target),
                    color_rgb(goal.color),
                    &mut y,
                );
            }
            line("MOVES LEFT", &g.remaining_moves().to_string(), text, &mut y);
            line("SCORE", &g.score.to_string(), dim, &mut y);
        }
    }

    y += 8.0;
    draw_text(format!("GEN  {}", g.generation + 1), PANEL_X, y, 20.0, dim);

    let _ = view;
}

fn outcome_text(outcome: Outcome) -> (&'static str, Color) {
    match outcome {
        Outcome::Won => ("GOAL CLEARED!", rgb(120, 220, 140)),
        Outcome::OutOfMoves => ("OUT OF MOVES", rgb(230, 110, 100)),
        Outcome::TimeUp => ("TIME'S UP", rgb(230, 110, 100)),
    }
}

fn draw_game_over(session: &Session, over_t: f32) {
    draw_rectangle(
        BOARD_X,
        BOARD_Y,
        BOARD_W,
        BOARD_H,
        Color::new(0.0, 0.0, 0.0, 0.72),
    );
    let cx = BOARD_X + BOARD_W * 0.5;
    let cy = BOARD_Y + BOARD_H * 0.5;

    let Phase::Over(outcome) = session.game.phase else {
        return;
    };
    let (title, color) = outcome_text(outcome);
    let d = measure_text(title, None, 30, 1.0);
    draw_text(title, cx - d.width * 0.5, cy - 20.0, 30.0, color);

    let score_line = format!("score {}", session.game.score);
    let sd = measure_text(&score_line, None, 20, 1.0);
    draw_text(
        &score_line,
        cx - sd.width * 0.5,
        cy + 14.0,
        20.0,
        rgb(210, 210, 225),
    );

    let sub = format!("Restarting in {:.0}...", over_t.max(0.0));
    let subd = measure_text(&sub, None, 18, 1.0);
    draw_text(
        &sub,
        cx - subd.width * 0.5,
        cy + 42.0,
        18.0,
        rgb(210, 210, 225),
    );
}
