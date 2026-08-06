use std::collections::{HashMap, HashSet, VecDeque};

// ── Grid geometry ─────────────────────────────────────────────────────────────────
//
// Cells are addressed by "doubled-width" hex coordinates `(col, row)`: `row` is a real
// board row (0 = ceiling), `col` is in units of half a bubble diameter (one `RADIUS`),
// not whole columns — a cell's true pixel x is `col * RADIUS`, with no row-dependent
// offset term at all. A row *as placed* (initial fill, or a fresh row inserted at
// `row = 0` by `descend_row`) satisfies `col & 1 == row & 1`, with `col = row&1,
// row&1 + 2, row&1 + 4, ...`.
//
// This was chosen over a plain offset/"brick wall" grid (odd rows shifted right by
// `RADIUS`, `col` a whole-number column) specifically because of `descend_row`: pushing
// a new row from the ceiling shifts every existing bubble down by one row, and in a
// circle-packed hex grid a bubble directly below another is *not* vertically aligned to
// its own row's cell grid unless the coordinate system already encodes the half-cell
// stagger in `col` itself. Plain offset coordinates need a compensating `col`
// adjustment whenever `row`'s parity flips (i.e. every single descend); doubled-width
// coordinates don't, since `col` alone determines pixel x — `descend_row` is just
// `row += 1` on every stored key, `col` untouched. It also gives a single constant
// 6-neighbor offset table (`NEIGHBOR_OFFSETS`) with no even/odd-row branching at all —
// the traditional #1 hex-grid bug class (verified independently below by hand-deriving
// the offsets from the pixel formulas rather than trusting a remembered table).
//
// One consequence worth being explicit about: `col & 1 == row & 1` is only a snapshot
// of a row *at the moment it's placed*, not an invariant of the live board. A shifted
// row's `row` flips parity every descend while its `col`s don't move (that's the
// straight-down shift above, and it's geometrically correct — a uniform translation of
// every cell preserves every pairwise distance, so the shifted content stays properly
// hex-packed relative to itself). But it does mean `row & 1` can't be used elsewhere as
// a proxy for "which columns this row's real content occupies" once any descend has
// happened — `board.cells.contains_key` is the only authority on that. See
// `nearest_occupied_cell`'s comment for the bug this caused when that assumption leaked
// into shot-collision detection.
pub const RADIUS: f32 = 22.0;
pub const DIAMETER: f32 = RADIUS * 2.0;
/// Vertical distance between rows for mutually tangent circles of radius `RADIUS`
/// packed in a triangular lattice — the height of the equilateral triangle formed by
/// three tangent circles' centers.
pub const ROW_HEIGHT: f32 = 38.105_18; // RADIUS * sqrt(3)
pub const COLS: i32 = 10;
pub const BOARD_W: f32 = DIAMETER * COLS as f32;
pub const BOARD_X: f32 = (600.0 - BOARD_W) / 2.0;
pub const BOARD_Y: f32 = 110.0;
/// A bubble landing at this row or deeper ends the episode — see `update_phase`. A
/// fixed global cushion (not per-`Level`, since the shooter's screen position depends
/// on it and the canvas is a fixed size) — see this crate's CLAUDE.md "Balance" section
/// for the full measured tuning trail: `DEATH_ROW=12` never produced a single loss
/// across 40 seeds regardless of how aggressively row-descend escalated (a skilled
/// beam-search bot's floater cascades burst-clear 10-20+ bubbles at once, so *average*
/// pressure alone doesn't threaten it), and `LEVELS`' `initial_rows` has to stay well
/// below this value — a level whose starting wall already sits close to `DEATH_ROW`
/// dies to the very first ceiling descend almost immediately, an instant-death
/// artifact rather than earned difficulty (measured directly: see `LEVELS`' own doc
/// comment). `7` is what actually produced a real, gradually-increasing-by-level loss
/// rate with organic (not instant) losses.
pub const DEATH_ROW: i32 = 7;
pub const SHOOTER_X: f32 = BOARD_X + BOARD_W / 2.0;
pub const SHOOTER_Y: f32 = BOARD_Y + (DEATH_ROW as f32 + 2.0) * ROW_HEIGHT;
/// Derived from `DEATH_ROW` (rather than a hand-picked constant) so the frame panel
/// stays sized to whatever `DEATH_ROW`/`SHOOTER_Y` actually are — leaves a little
/// margin below the shooter for its own cannon graphic.
pub const BOARD_H: f32 = SHOOTER_Y - BOARD_Y + RADIUS * 3.0;

/// A round-index-indexed difficulty tier — cycles by `Game::generation` (see
/// `level_for`), same "watch it ramp up over successive rounds, then loop" framing
/// match-3's `LEVELS` line uses, adapted to this game's two requested levers: how many
/// colors are in play (`color_count` — fewer colors means far more incidental matches,
/// same lever match-3's `LevelParams::color_count` ramp already established, here run
/// in the *easy* direction at low levels) and how dense the starting wall is
/// (`initial_rows`). `DEATH_ROW`/`DESCEND_INTERVAL`/solver weights stay global — those
/// already ramp difficulty *within* an episode (row-descend escalation); this ramps the
/// baseline *across* episodes instead.
pub struct Level {
    pub name: &'static str,
    pub color_count: usize,
    pub initial_rows: i32,
}

/// Mirrors match-3's `MIN_LEVEL_COLORS` floor for the same reason: too few colors makes
/// matching close to trivial rather than genuinely easier-but-real.
pub const MIN_LEVEL_COLORS: usize = 3;

// `initial_rows` is deliberately capped at 5, not ramped all the way to a near-`DEATH_ROW`
// value — a first pass ramped it to 7 and measured almost every "Full Spectrum"/"Overflow"
// loss landing at `shots_used` 1-7 (i.e. dying to the very first ceiling descend, sometimes
// before the bot got a real turn at all): with `DEATH_ROW` fixed globally (the shooter's
// screen position depends on it, so it can't vary per level on a fixed-size canvas), a
// starting wall that dense left near-zero margin *before play even begins* — that's an
// instant-death artifact, not earned difficulty, and reads as broken rather than hard. See
// this crate's CLAUDE.md for the measured before/after.
pub const LEVELS: &[Level] = &[
    Level {
        name: "Warm-Up",
        color_count: 3,
        initial_rows: 3,
    },
    Level {
        name: "Getting Busy",
        color_count: 4,
        initial_rows: 3,
    },
    Level {
        name: "Color Rush",
        color_count: 4,
        initial_rows: 4,
    },
    Level {
        name: "Wider Palette",
        color_count: 5,
        initial_rows: 4,
    },
    Level {
        name: "Packed House",
        color_count: 5,
        initial_rows: 5,
    },
    Level {
        name: "Full Spectrum",
        color_count: 6,
        initial_rows: 4,
    },
    Level {
        name: "Overflow",
        color_count: 6,
        initial_rows: 5,
    },
];

pub fn level_for(generation: u32) -> &'static Level {
    &LEVELS[generation as usize % LEVELS.len()]
}
/// Shots between each ceiling row-push at the start of an episode — the
/// survival-pressure pacing (see root CLAUDE.md discussion / plan: "row-descend
/// survival" episode structure). Shrinks over the episode via `descend_interval_for` —
/// a flat interval measured as never once threatening `DEATH_ROW` (the beam solver
/// comfortably keeps the board under control indefinitely at a fixed pace, floater
/// cascades routinely popping 10+ bubbles at once): rows arriving faster as the run
/// goes on, the genre-standard difficulty curve, is what actually gives a run real
/// stakes instead of a fixed-cap cruise. See this crate's CLAUDE.md's "Balance" section
/// for the full measured tuning trail (this knob alone wasn't enough — `DEATH_ROW` and
/// `LEVELS`' `initial_rows`/`color_count` needed to move too).
pub const DESCEND_INTERVAL: u32 = 6;
pub const MIN_DESCEND_INTERVAL: u32 = 1;
/// The interval shrinks by 1 every this many shots.
const DESCEND_RAMP_SHOTS: u32 = 10;
/// Hard cap on episode length — see `Outcome::Survived`.
pub const SHOT_LIMIT: u32 = 150;

pub fn descend_interval_for(shots_used: u32) -> u32 {
    DESCEND_INTERVAL
        .saturating_sub(shots_used / DESCEND_RAMP_SHOTS)
        .max(MIN_DESCEND_INTERVAL)
}

const ANGLE_MIN: f32 = 8.0;
const ANGLE_MAX: f32 = 172.0;
const ANGLE_STEP: f32 = 1.0;
const RAY_STEP: f32 = RADIUS * 0.5;
const COLLISION_DIST: f32 = RADIUS * 1.8;
const MAX_BOUNCES: u32 = 8;
const MAX_RAY_STEPS: u32 = 600;

const POP_POINTS: u32 = 10;
const FLOATER_POINTS: u32 = 20;

/// The six neighbor offsets, constant for every `(col, row)` regardless of parity —
/// hand-derived from the pixel formulas (`cell_pixel`) rather than assumed: same-row
/// neighbors are a full diameter apart in x (`Δcol = ±2`), diagonal neighbors are half a
/// diameter apart in x and one row apart (`Δcol = ±1, Δrow = ±1`). Symmetry
/// (`a` a neighbor of `b` iff `b` a neighbor of `a`) is checked by
/// `neighbor_offsets_are_symmetric` below.
const NEIGHBOR_OFFSETS: [(i32, i32); 6] = [(2, 0), (-2, 0), (1, 1), (1, -1), (-1, 1), (-1, -1)];

/// `row0_parity` is `board.row0_parity` — which column parity the *current* literal
/// row 0 uses (see `Board::row0_parity`'s doc comment for why this isn't always 0).
fn col_bounds(row0_parity: i32, row: i32) -> (i32, i32) {
    if (row0_parity + row) & 1 == 0 {
        (0, 2 * (COLS - 1))
    } else {
        (1, 2 * COLS - 3)
    }
}

pub fn cell_pixel(col: i32, row: i32) -> (f32, f32) {
    (
        BOARD_X + RADIUS + col as f32 * RADIUS,
        BOARD_Y + RADIUS + row as f32 * ROW_HEIGHT,
    )
}

/// Approximate nearest cell to a pixel point — used only to seed the small search
/// window `nearest_occupied_cell` scans, not as a final answer, so the col clamp here
/// is a plain board-width bound rather than a parity-exact one: `nearest_occupied_cell`
/// checks `board.cells` directly (authoritative regardless of parity drift — see its
/// own doc comment) across a ±2-column window around this estimate, wide enough to
/// cover either parity.
fn pixel_to_cell(x: f32, y: f32) -> (i32, i32) {
    let row = (((y - BOARD_Y - RADIUS) / ROW_HEIGHT).round() as i32).max(0);
    let parity = row & 1;
    let col_f = (x - BOARD_X - RADIUS) / RADIUS;
    let mut col = col_f.round() as i32;
    if (col & 1) != parity {
        let up = col + 1;
        let down = col - 1;
        col = if (up as f32 - col_f).abs() < (down as f32 - col_f).abs() {
            up
        } else {
            down
        };
    }
    (col.clamp(0, 2 * (COLS - 1)), row)
}

// ── Board-local RNG + deterministic hashing ─────────────────────────────────────────
//
// Same shape and same reason as match-3's `Rng`/`DeterministicHasher`/`preview_seed`
// (see that crate's `game.rs` doc comment and `feedback_solver_rng_confound` memory):
// a hypothetical-scoring clone must never let its RNG carry forward the real game's
// actual future stream, or the beam solver's own scoring becomes an oracle for
// whichever move it's about to apply for real. `Game::simulate` and
// `beam_solver::SearchState::apply`'s impl (solver.rs) both reseed from `preview_seed`
// before resolving, for exactly the same reason match-3's two call sites do.
#[derive(Clone, Copy)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn seeded(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn gen_range(&mut self, low: u64, high: u64) -> u64 {
        debug_assert!(low < high);
        low + self.next_u64() % (high - low)
    }
}

pub(crate) struct DeterministicHasher(pub(crate) u64);

impl std::hash::Hasher for DeterministicHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            let mut z = self.0 ^ (b as u64);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            self.0 = z ^ (z >> 31);
        }
    }
}

/// Deterministic seed for a hypothetical resolve — hashes the board's sorted content
/// (never raw `HashMap` iteration order, which isn't stable run-to-run) plus the
/// candidate move. See `Rng`'s doc comment.
pub(crate) fn preview_seed(board: &Board, mv: Move) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut entries: Vec<((i32, i32), Color)> = board.cells.iter().map(|(&k, &v)| (k, v)).collect();
    entries.sort_unstable_by_key(|&(k, _)| k);
    let mut h = DeterministicHasher(0x2545_F491_4F6C_DD1D);
    entries.hash(&mut h);
    mv.hash(&mut h);
    h.finish()
}

// ── Core types ───────────────────────────────────────────────────────────────────

/// 6, not the genre-typical-minimum 5 — an initial 5-color build let the beam solver
/// keep the board almost empty (routine 10-20-bubble floater cascades) for the entire
/// `SHOT_LIMIT`, never once losing across a 30-seed sweep even with `MIN_DESCEND_INTERVAL`
/// escalation maxed out; a 6th color measurably thins out available matches (same lever
/// match-3's `LevelParams::color_count` ramp already uses, in reverse) enough to matter.
/// See this crate's CLAUDE.md for the numbers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Color {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl Color {
    pub const ALL: [Color; 6] = [
        Color::Red,
        Color::Orange,
        Color::Yellow,
        Color::Green,
        Color::Blue,
        Color::Purple,
    ];
}

#[derive(Clone)]
pub struct Board {
    pub cells: HashMap<(i32, i32), Color>,
    /// Which column parity literal row 0 currently uses (`0` or `1`) — starts `0`
    /// (initial fill's row 0 always uses even columns) and flips every `descend_row`.
    /// `col_bounds` needs this: `descend_row` shifts existing rows by `row += 1` with
    /// `col` untouched (a valid uniform translation — every pairwise cell distance is
    /// preserved), but a fresh row is always inserted at literal row 0, whose *required*
    /// column parity to stay tangent-adjacent to what's now row 1 alternates every
    /// single descend. Hardcoding "row 0 = even columns" here (as the code briefly did)
    /// severs the entire wall from the ceiling on every other descend — see
    /// `descend_row`'s comment.
    pub row0_parity: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Move {
    pub target: (i32, i32),
    pub angle_deg: f32,
}

impl std::hash::Hash for Move {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.target.hash(state);
        self.angle_deg.to_bits().hash(state);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The board emptied out. Rare in practice once row-descend is running (a new row
    /// keeps arriving every `DESCEND_INTERVAL` shots regardless of skill) — mostly
    /// reachable only early, before the first descend or two.
    Won,
    Lost,
    /// Reached `SHOT_LIMIT` shots without losing — a good, watchable run. Without this,
    /// episodes have no guaranteed end at all: a competent bot can in principle survive
    /// indefinitely (only `Lost` and the near-unreachable `Won` end an episode
    /// otherwise), which surfaced as a real hang in `full_playthrough_terminates`
    /// (5000+ shots, still `Playing`) rather than just a theoretical concern. Matches
    /// every other game in this workspace having *some* move/time limit bounding an
    /// episode's length.
    Survived,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    Over(Outcome),
}

/// Everything a move's cosmetic playback (`main.rs`) and the solver's scoring both
/// need — the "compute first, animate after" split every self-playing game in this
/// workspace uses (see match-3's `Resolution`/`Wave`).
pub struct Resolution {
    pub mv: Move,
    pub color: Color,
    /// The raymarched flight path (shooter position through every wall-bounce point to
    /// the landing point), recomputed fresh against the pre-move board — cheap (one
    /// raymarch), and lets `main.rs` animate the actual bounce trajectory rather than a
    /// straight lerp to the target cell.
    pub path: Vec<(f32, f32)>,
    /// Cells removed by the landing color-match (empty if the shot didn't pop anything).
    pub popped: Vec<(i32, i32)>,
    /// Cells removed because they were no longer connected to the ceiling after
    /// `popped` was removed (empty unless `popped` is non-empty).
    pub floaters: Vec<(i32, i32)>,
    pub score_gained: u32,
    pub descended: bool,
}

#[derive(Clone)]
pub struct Game {
    pub board: Board,
    /// The color palette this episode draws from — a prefix of `Color::ALL` sized by
    /// this generation's `Level::color_count` (see `level_for`). Carried on `Game`
    /// (rather than threaded through every random-draw call separately) so every
    /// draw — initial fill, the shot queue, row-descend refills — stays consistent
    /// automatically, same reasoning match-3's `Board::active_colors` doc comment gives.
    pub active_colors: Vec<Color>,
    pub current_color: Color,
    pub next_color: Color,
    pub score: u32,
    pub shots_used: u32,
    pub shots_until_descend: u32,
    pub phase: Phase,
    pub generation: u32,
    pub(crate) rng: Rng,
}

impl Game {
    pub fn new(generation: u32) -> Self {
        let level = level_for(generation);
        debug_assert!(
            (MIN_LEVEL_COLORS..=Color::ALL.len()).contains(&level.color_count),
            "level {} color_count {} out of range",
            level.name,
            level.color_count
        );
        let active_colors: Vec<Color> = Color::ALL[..level.color_count].to_vec();
        let mut rng = Rng::seeded(macroquad::rand::gen_range(0u64, u64::MAX));
        let mut cells = HashMap::new();
        for row in 0..level.initial_rows {
            let (min_c, max_c) = col_bounds(0, row);
            let mut col = min_c;
            while col <= max_c {
                let color = active_colors[rng.gen_range(0, active_colors.len() as u64) as usize];
                cells.insert((col, row), color);
                col += 2;
            }
        }
        let board = Board {
            cells,
            row0_parity: 0,
        };
        let mut game = Self {
            board,
            active_colors,
            current_color: Color::ALL[0],
            next_color: Color::ALL[0],
            score: 0,
            shots_used: 0,
            shots_until_descend: DESCEND_INTERVAL,
            phase: Phase::Playing,
            generation,
            rng,
        };
        game.current_color = game.draw_color();
        game.next_color = game.draw_color();
        game
    }

    /// Every distinct landing cell reachable by some sampled firing angle right now —
    /// the discrete move space a continuous aim angle gets reduced to (root CLAUDE.md's
    /// "Self-playing solver games" section: `beam_solver` needs a `Vec<Move>`, not a
    /// continuous parameter). Angles are sampled every `ANGLE_STEP` degrees; multiple
    /// angles landing on the same cell keep only the one closest to 90° (straight up) as
    /// that cell's representative `Move`, so a later replay of the *chosen* move's own
    /// angle reproduces the same landing deterministically.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut best: HashMap<(i32, i32), f32> = HashMap::new();
        let mut angle = ANGLE_MIN;
        while angle <= ANGLE_MAX + 1e-4 {
            if let (_, Some(target)) = raymarch(&self.board, angle) {
                let entry = best.entry(target).or_insert(angle);
                if (angle - 90.0).abs() < (*entry - 90.0).abs() {
                    *entry = angle;
                }
            }
            angle += ANGLE_STEP;
        }
        best.into_iter()
            .map(|(target, angle_deg)| Move { target, angle_deg })
            .collect()
    }

    /// Resolves `mv` against a clone of this game without touching `self` — used by the
    /// solver to score every candidate before committing to one. Returns both the
    /// resulting `Game` (the solver's scoring needs the post-move board shape, not just
    /// `Resolution`'s pop/floater lists) and the `Resolution` itself. The clone's `rng`
    /// is reseeded from `preview_seed` rather than carrying `self.rng`'s current state
    /// forward — see `Rng`'s doc comment for why a straight carry-over would make this
    /// an oracle for whatever move the caller actually applies next.
    pub fn simulate(&self, mv: Move) -> (Game, Resolution) {
        let mut g = self.clone();
        g.rng = Rng::seeded(preview_seed(&self.board, mv));
        let res = g.resolve(mv);
        (g, res)
    }

    /// Applies `mv` for real: mutates `self`, advances the shot queue and row-descend
    /// counter, updates `phase`.
    pub fn apply(&mut self, mv: Move) -> Resolution {
        self.resolve(mv)
    }

    fn resolve(&mut self, mv: Move) -> Resolution {
        let color = self.current_color;
        let (path, landing) = raymarch(&self.board, mv.angle_deg);
        debug_assert_eq!(
            landing,
            Some(mv.target),
            "mv.angle_deg must reproduce mv.target against the same board"
        );

        self.board.cells.insert(mv.target, color);

        let mut popped = Vec::new();
        let mut floaters = Vec::new();
        let group = flood_same_color(&self.board, mv.target, color);
        if group.len() >= 3 {
            for &c in &group {
                self.board.cells.remove(&c);
            }
            let floating = find_floating(&self.board);
            for &c in &floating {
                self.board.cells.remove(&c);
            }
            popped = group.into_iter().collect();
            floaters = floating.into_iter().collect();
        }

        let score_gained =
            popped.len() as u32 * POP_POINTS + floaters.len() as u32 * FLOATER_POINTS;
        self.score += score_gained;
        self.shots_used += 1;

        self.current_color = self.next_color;
        self.next_color = self.draw_color();

        let descended = if self.shots_until_descend == 0 {
            self.descend_row();
            self.shots_until_descend = descend_interval_for(self.shots_used);
            true
        } else {
            self.shots_until_descend -= 1;
            false
        };

        self.update_phase();

        Resolution {
            mv,
            color,
            path,
            popped,
            floaters,
            score_gained,
            descended,
        }
    }

    /// Draws from colors currently present on the board (never a color with zero
    /// bubbles left) — an empty-board fallback only matters for the frame the board
    /// clears, which is already a win (see `update_phase`), so which color it "would"
    /// draw next is moot. Sorted before indexing so the draw is a pure function of
    /// `self.rng`'s state and the board's *content* (not `HashMap` iteration order,
    /// which isn't stable run-to-run) — the same determinism concern `preview_seed`
    /// hashes sorted entries for.
    fn draw_color(&mut self) -> Color {
        let mut present: Vec<Color> = self.board.cells.values().copied().collect();
        present.sort_unstable();
        present.dedup();
        if present.is_empty() {
            return self.active_colors
                [self.rng.gen_range(0, self.active_colors.len() as u64) as usize];
        }
        present[self.rng.gen_range(0, present.len() as u64) as usize]
    }

    /// Every existing bubble drops one row (`row += 1`, `col` unchanged — see the grid
    /// geometry doc comment at the top of this file for why doubled-width coordinates
    /// make this a pure shift with no lateral correction needed), then a fresh random
    /// row is inserted at `row = 0`.
    ///
    /// The fresh row's column parity must flip every call (`row0_parity ^= 1`) to stay
    /// tangent-adjacent to what's now row 1: a bug fixed here used to hardcode row 0 to
    /// always start at column 0 (even), which is only correct the *first* time — every
    /// subsequent descend needs the opposite parity to actually touch the shifted wall
    /// above it (`col_bounds`' doc comment / `Board::row0_parity`'s doc comment have the
    /// full geometric argument). Getting this wrong doesn't misplace anything within a
    /// diameter or two — it silently detaches the *entire* existing wall from the
    /// ceiling every other descend, which `find_floating` never catches because it only
    /// runs after a pop, not after a descend.
    fn descend_row(&mut self) {
        let mut shifted = HashMap::with_capacity(self.board.cells.len() + COLS as usize);
        for (&(col, row), &color) in &self.board.cells {
            shifted.insert((col, row + 1), color);
        }
        self.board.row0_parity ^= 1;
        let (min_c, max_c) = col_bounds(self.board.row0_parity, 0);
        let mut col = min_c;
        while col <= max_c {
            let color =
                self.active_colors[self.rng.gen_range(0, self.active_colors.len() as u64) as usize];
            shifted.insert((col, 0), color);
            col += 2;
        }
        self.board.cells = shifted;
    }

    fn update_phase(&mut self) {
        if self.phase != Phase::Playing {
            return;
        }
        if self.board.cells.is_empty() {
            self.phase = Phase::Over(Outcome::Won);
            return;
        }
        if self.board.cells.keys().any(|&(_, r)| r >= DEATH_ROW) {
            self.phase = Phase::Over(Outcome::Lost);
            return;
        }
        if self.shots_used >= SHOT_LIMIT {
            self.phase = Phase::Over(Outcome::Survived);
        }
    }
}

pub(crate) fn max_row(board: &Board) -> i32 {
    board.cells.keys().map(|&(_, r)| r).max().unwrap_or(0)
}

// ── Shot physics ─────────────────────────────────────────────────────────────────

/// A raymarched shot's polyline (for animation) plus the landing cell it snapped to,
/// or `None` if it never lands (rare/absent in practice — see `raymarch`).
type RaymarchResult = (Vec<(f32, f32)>, Option<(i32, i32)>);

/// Raymarches a shot fired at `angle_deg` (0 = along +x, 90 = straight up) from the
/// shooter, reflecting off the side walls, until it hits the ceiling or an existing
/// bubble. Returns the full polyline (for animation) and the landing cell it snapped
/// to, or `None` if it never lands (angle clamps + a bounce cap keep this rare/absent
/// in practice — see `ANGLE_MIN`/`ANGLE_MAX`/`MAX_BOUNCES`).
fn raymarch(board: &Board, angle_deg: f32) -> RaymarchResult {
    let rad = angle_deg.to_radians();
    let mut x = SHOOTER_X;
    let mut y = SHOOTER_Y;
    let mut dx = rad.cos();
    let dy = -rad.sin();
    let mut path = vec![(x, y)];
    let mut bounces = 0u32;

    for _ in 0..MAX_RAY_STEPS {
        x += dx * RAY_STEP;
        y += dy * RAY_STEP;

        if x - RADIUS < BOARD_X {
            x = BOARD_X + RADIUS;
            dx = -dx;
            bounces += 1;
        } else if x + RADIUS > BOARD_X + BOARD_W {
            x = BOARD_X + BOARD_W - RADIUS;
            dx = -dx;
            bounces += 1;
        }
        if bounces > MAX_BOUNCES {
            return (path, None);
        }

        path.push((x, y));

        if y - RADIUS <= BOARD_Y {
            let landing = nearest_empty_in_row(board, x, 0);
            snap_path_to_landing(&mut path, landing);
            return (path, landing);
        }
        if let Some(hit) = nearest_occupied_cell(board, x, y) {
            let landing = nearest_empty_neighbor(board, hit, x, y);
            snap_path_to_landing(&mut path, landing);
            return (path, landing);
        }
    }
    (path, None)
}

/// `path`'s last point is wherever the raymarch happened to notice a collision
/// (`RAY_STEP`-sized steps, within `COLLISION_DIST` of the hit cell) — not the same
/// point as `landing`'s actual cell center, which can be a full `RADIUS`-ish away on a
/// diagonal approach. Appending the true landing pixel as one final waypoint makes
/// `main.rs`'s flight animation arrive exactly where the bubble will actually settle,
/// instead of visibly snapping to it the instant `Flying` ends.
fn snap_path_to_landing(path: &mut Vec<(f32, f32)>, landing: Option<(i32, i32)>) {
    if let Some(cell) = landing {
        path.push(cell_pixel(cell.0, cell.1));
    }
}

fn nearest_occupied_cell(board: &Board, x: f32, y: f32) -> Option<(i32, i32)> {
    let (center_col, center_row) = pixel_to_cell(x, y);
    let mut best: Option<((i32, i32), f32)> = None;
    for row in (center_row - 1).max(0)..=(center_row + 1) {
        let mut col = center_col - 2;
        while col <= center_col + 2 {
            // No parity/bounds pre-filter here: `descend_row` shifts existing rows by
            // `row += 1` with `col` untouched (geometrically valid — a uniform
            // translation preserves every pairwise cell distance), but that means a
            // shifted row's real occupied columns no longer match the `col & 1 == row &
            // 1` convention `col_bounds` assumes for a *freshly placed* row. Filtering
            // on that assumption here silently skipped real occupied cells in any row
            // that had survived a descend, so the raymarch missed the collision and
            // landed the shot in the wrong cell — visible as misaligned/floating
            // bubbles in shifted rows. `contains_key` is authoritative on its own, and
            // the distance check below already rejects anything geometrically wrong.
            if board.cells.contains_key(&(col, row)) {
                let (cx, cy) = cell_pixel(col, row);
                let d = ((cx - x).powi(2) + (cy - y).powi(2)).sqrt();
                if d < COLLISION_DIST && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some(((col, row), d));
                }
            }
            col += 1;
        }
    }
    best.map(|(c, _)| c)
}

/// The closest empty neighbor of `hit` to `(x, y)` — iterates `NEIGHBOR_OFFSETS` in a
/// fixed order and only updates `best` on a strictly smaller distance, so an exact tie
/// deterministically resolves to whichever offset comes first in that fixed array
/// (needed for `HCG_SEED` reproducibility — see root CLAUDE.md's "Native CLI flags").
fn nearest_empty_neighbor(board: &Board, hit: (i32, i32), x: f32, y: f32) -> Option<(i32, i32)> {
    let mut best: Option<((i32, i32), f32)> = None;
    for &(dc, dr) in &NEIGHBOR_OFFSETS {
        let cand = (hit.0 + dc, hit.1 + dr);
        if cand.1 < 0 {
            continue;
        }
        let (min_c, max_c) = col_bounds(board.row0_parity, cand.1);
        if cand.0 < min_c || cand.0 > max_c {
            continue;
        }
        if board.cells.contains_key(&cand) {
            continue;
        }
        let (cx, cy) = cell_pixel(cand.0, cand.1);
        let d = ((cx - x).powi(2) + (cy - y).powi(2)).sqrt();
        if best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((cand, d));
        }
    }
    best.map(|(c, _)| c)
}

fn nearest_empty_in_row(board: &Board, x: f32, row: i32) -> Option<(i32, i32)> {
    let (min_c, max_c) = col_bounds(board.row0_parity, row);
    let mut best: Option<((i32, i32), f32)> = None;
    let mut col = min_c;
    while col <= max_c {
        if !board.cells.contains_key(&(col, row)) {
            let (cx, _) = cell_pixel(col, row);
            let d = (cx - x).abs();
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some(((col, row), d));
            }
        }
        col += 2;
    }
    best.map(|(c, _)| c)
}

// ── Connectivity ─────────────────────────────────────────────────────────────────

fn flood_same_color(board: &Board, start: (i32, i32), color: Color) -> HashSet<(i32, i32)> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some(cur) = queue.pop_front() {
        for &(dc, dr) in &NEIGHBOR_OFFSETS {
            let n = (cur.0 + dc, cur.1 + dr);
            if visited.contains(&n) {
                continue;
            }
            if board.cells.get(&n) == Some(&color) {
                visited.insert(n);
                queue.push_back(n);
            }
        }
    }
    visited
}

/// Every bubble not transitively connected to some `row == 0` bubble via any adjacency
/// (color-blind — a bubble is physically held up by touching *any* neighbor, not just
/// same-color ones). Only meaningful to call right after removing a matched group; a
/// full BFS from every ceiling cell, not a local check, since a bubble several rows
/// down can lose its only support from a pop far above it.
fn find_floating(board: &Board) -> HashSet<(i32, i32)> {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    for &pos in board.cells.keys().filter(|&&(_, r)| r == 0) {
        if reachable.insert(pos) {
            queue.push_back(pos);
        }
    }
    while let Some(cur) = queue.pop_front() {
        for &(dc, dr) in &NEIGHBOR_OFFSETS {
            let n = (cur.0 + dc, cur.1 + dr);
            if board.cells.contains_key(&n) && reachable.insert(n) {
                queue.push_back(n);
            }
        }
    }
    board
        .cells
        .keys()
        .filter(|p| !reachable.contains(p))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards the traditional #1 hex-grid bug class this file's `NEIGHBOR_OFFSETS`
    /// comment calls out — the doubled-width table was hand-derived from the pixel
    /// formulas, not copied from memory, and this is the check that would have caught
    /// it being wrong. Checked across every row parity and both boundary columns, not
    /// just one interior cell.
    #[test]
    fn neighbor_offsets_are_symmetric() {
        for row in 0..6 {
            let (min_c, max_c) = col_bounds(0, row);
            let mut col = min_c;
            while col <= max_c {
                for &(dc, dr) in &NEIGHBOR_OFFSETS {
                    let n = (col + dc, row + dr);
                    if n.1 < 0 {
                        continue;
                    }
                    let (n_min, n_max) = col_bounds(0, n.1);
                    if n.0 < n_min || n.0 > n_max {
                        continue;
                    }
                    let back_is_neighbor = NEIGHBOR_OFFSETS
                        .iter()
                        .any(|&(bdc, bdr)| (n.0 + bdc, n.1 + bdr) == (col, row));
                    assert!(back_is_neighbor, "({col},{row}) -> {n:?} is not mutual");
                }
                col += 2;
            }
        }
    }

    /// Neighboring cells' pixel centers should be one bubble diameter apart (same row)
    /// or exactly `sqrt(RADIUS^2 + ROW_HEIGHT^2)` apart (diagonal, by construction —
    /// `Δx = RADIUS`, `Δy = ROW_HEIGHT`) — i.e. genuinely tangent-circle distance, not
    /// just a topologically-consistent but geometrically-wrong table.
    #[test]
    fn neighbor_offsets_match_tangent_circle_distance() {
        let (cx, cy) = cell_pixel(4, 4);
        for &(dc, dr) in &NEIGHBOR_OFFSETS {
            let (nx, ny) = cell_pixel(4 + dc, 4 + dr);
            let dist = ((nx - cx).powi(2) + (ny - cy).powi(2)).sqrt();
            let expected = if dr == 0 {
                DIAMETER
            } else {
                (RADIUS.powi(2) + ROW_HEIGHT.powi(2)).sqrt()
            };
            assert!(
                (dist - expected).abs() < 0.01,
                "offset {:?}: dist {dist} != expected {expected}",
                (dc, dr)
            );
        }
    }

    #[test]
    fn descend_row_shifts_pixel_position_straight_down_only() {
        let mut game = Game::new(0);
        game.board.cells.clear();
        game.board.cells.insert((4, 2), Color::Red);
        let (x0, y0) = cell_pixel(4, 2);
        game.descend_row();
        let (x1, y1) = cell_pixel(4, 3);
        assert_eq!(game.board.cells.get(&(4, 3)), Some(&Color::Red));
        assert!((x1 - x0).abs() < 0.01, "x shifted sideways on descend");
        assert!((y1 - y0 - ROW_HEIGHT).abs() < 0.01);
    }

    /// Regression test for the bug this file's grid-geometry doc comment and
    /// `nearest_occupied_cell`'s own comment now describe: after a descend, a shifted
    /// row's real column parity no longer matches the `row & 1` convention `col_bounds`
    /// assumes for a freshly-placed row. `nearest_occupied_cell` used to pre-filter
    /// candidates on that assumption, so a shot aimed straight at a bubble sitting in a
    /// once-shifted row would silently pass through it instead of colliding.
    #[test]
    fn nearest_occupied_cell_finds_bubble_in_shifted_row() {
        let mut game = Game::new(0);
        game.board.cells.clear();
        game.board.cells.insert((4, 2), Color::Red);
        game.descend_row(); // (4, 2) -> (4, 3): row parity flips, col doesn't.
        assert_eq!(game.board.cells.get(&(4, 3)), Some(&Color::Red));
        let (x, y) = cell_pixel(4, 3);
        assert_eq!(nearest_occupied_cell(&game.board, x, y), Some((4, 3)));
    }

    /// Regression test for the real root cause behind the "floating disconnected
    /// bubble" bug report: `descend_row` used to always insert the fresh ceiling row at
    /// even columns, correct only the *first* time. From the second descend on, that
    /// hardcoded parity no longer matched what was needed to stay tangent-adjacent to
    /// the (now-shifted) row below it, silently severing the *entire* existing wall
    /// from the ceiling every other descend — `find_floating` never caught it because
    /// it only runs after a pop, not after a descend itself. Two descends on a
    /// fully-connected 2-row wall must leave it fully connected.
    #[test]
    fn descend_row_keeps_wall_connected() {
        let mut game = Game::new(0);
        game.board.cells.clear();
        let mut col = 0;
        while col <= 2 * (COLS - 1) {
            game.board.cells.insert((col, 0), Color::Red);
            col += 2;
        }
        let mut col = 1;
        while col <= 2 * COLS - 3 {
            game.board.cells.insert((col, 1), Color::Red);
            col += 2;
        }
        for n in 1..=4 {
            game.descend_row();
            assert!(
                find_floating(&game.board).is_empty(),
                "wall disconnected from ceiling after {n} descend(s)"
            );
        }
    }

    #[test]
    fn full_playthrough_terminates() {
        macroquad::rand::srand(42);
        for i in 0..3 {
            let mut game = Game::new(i);
            let mut beam = crate::solver::new_beam_search();
            let mut guard = 0;
            loop {
                guard += 1;
                assert!(
                    guard <= SHOT_LIMIT + 1,
                    "playthrough exceeded SHOT_LIMIT without ending"
                );
                match game.phase {
                    Phase::Playing => {
                        let mv = crate::solver::choose_move(&mut beam, &game)
                            .expect("Phase::Playing guarantees a legal move");
                        assert!(game.legal_moves().iter().any(|m| m.target == mv.target));
                        game.apply(mv);
                        // The settled board must never contain a bubble disconnected
                        // from the ceiling — `resolve()` removes every floater in the
                        // same call as the pop that caused it. This is exactly the
                        // check that caught `descend_row`'s parity bug: it failed on
                        // ~68% of settled boards across a full playthrough before the
                        // fix.
                        assert!(
                            find_floating(&game.board).is_empty(),
                            "shot {} left a disconnected bubble on the settled board",
                            game.shots_used
                        );
                    }
                    Phase::Over(_) => break,
                }
            }
        }
    }
}
