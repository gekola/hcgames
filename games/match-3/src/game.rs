use std::collections::{HashSet, VecDeque};

/// A tiny board-local PRNG (SplitMix64), used instead of the ambient global
/// `macroquad::rand` for every draw that happens *after* a board exists (refill,
/// reshuffle, mystery-goal rolls). `macroquad::rand`/quad-rand is one process-global
/// generator with no public snapshot/restore API — any code that draws from it (even
/// `Game::simulate`, called purely to score a hypothetical move on a scratch clone)
/// permanently advances the *real* game's future randomness as a side effect. That
/// silently broke A/B comparisons between solvers that call `simulate` a different
/// number of times per real move (e.g. a beam search vs. plain greedy): at the same
/// `HCG_SEED`, the two would face different real refills, despite the fixed seed —
/// see `feedback_solver_rng_confound` memory / `.notes/match3_solver_beam_todo.md`.
///
/// Living on `Board` (already `Clone`, and already cloned freely for scratch/lookahead
/// use) is only half the fix, though. The first version of this change simply let a
/// clone's `rng` carry forward from whatever state the real board's `rng` was in —
/// which stopped scoring from *perturbing* the real game's future, but had a much
/// worse side effect: since nothing else touches the real board (or its `rng`) between
/// a `simulate` call and a same-move real `apply`, the "preview" became a **perfect
/// oracle** for whichever move actually gets chosen next, not an unbiased estimate —
/// measured as every variant's win rate jumping to 85-100% (up from a healthy
/// 45-65% band) on re-running the seed sweep, which is what caught it. See
/// `preview_seed` for the actual fix: every hypothetical resolve (`Game::simulate`,
/// and the beam solver's own scratch-line `apply`, see `solver.rs`) reseeds from a
/// deterministic hash of the pre-move board + move instead of carrying the real
/// stream forward, so a preview is decorrelated from whatever the real future
/// refill will actually be, while still being fully independent of caller/call-count
/// (still fixes the original confound). Only real gameplay (`Game::apply`, called on
/// `self.board` directly, never a clone) continues the board's *actual* running
/// stream, uninterrupted by however much scoring happened around it.
#[derive(Clone, Copy)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn seeded(seed: u64) -> Self {
        // XOR by a fixed odd constant so an all-zero incoming seed still produces a
        // well-mixed initial state (SplitMix64, like most xorshift-derived PRNGs, is
        // degenerate at state 0).
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish integer in `[low, high)`, matching `quad_rand::gen_range`'s
    /// half-open convention. Every call site here only ever needs a small range (board
    /// coordinates, a handful of colors, a goal count/target), so the modulo bias this
    /// introduces is negligible.
    fn gen_range(&mut self, low: u64, high: u64) -> u64 {
        debug_assert!(low < high);
        low + self.next_u64() % (high - low)
    }
}

/// A `std::hash::Hasher` backed by the same SplitMix64 avalanche step as `Rng`, but
/// XORing each incoming byte into the state *before* mixing (rather than just adding a
/// fixed constant, as `Rng::next_u64`'s own counter-style advance does) so the output
/// actually depends on the bytes written, not just how many. Deliberately **not**
/// `std::collections::hash_map::DefaultHasher` — this file already documents (see
/// `find_matches_with_spawns`' tie-break) that `std`'s default hasher is randomly
/// reseeded per *process*, which would silently break the "same `HCG_SEED` reproduces
/// the same run" contract if used for anything that affects which move gets chosen.
struct DeterministicHasher(u64);

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

/// Deterministic seed for a hypothetical resolve — hashes the board's visible content
/// (`tiles`/`jelly`, both already `Hash`-derived) plus the candidate move, *not*
/// `board.rng`'s own current state. See `Rng`'s doc comment for why: this is what
/// keeps every `Game::simulate`/beam-search preview decorrelated from the real board's
/// actual future stream while staying fully deterministic (same board+move always
/// previews identically) and independent of how many other candidates get scored
/// around it.
pub(crate) fn preview_seed(board: &Board, mv: Move) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = DeterministicHasher(0x2545_F491_4F6C_DD1D);
    board.tiles.hash(&mut h);
    board.jelly.hash(&mut h);
    mv.hash(&mut h);
    h.finish()
}

pub const W: usize = 8;
pub const H: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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

    /// Picks a random color from a runtime subset — used everywhere a board's own
    /// `active_colors` needs to be respected (initial gen, gravity refill, reshuffle) so
    /// a level with fewer active colors never has one sneak back in via a fresh tile.
    /// Every caller currently passes either `Color::ALL` (free-cycling variants) or a
    /// `LevelParams::color_count`-sized prefix of it (`Variant::Mode::Levels`).
    fn random_from(colors: &[Color], rng: &mut Rng) -> Color {
        colors[rng.gen_range(0, colors.len() as u64) as usize]
    }

    /// Index into `Color::ALL` / `Resolution::color_cleared` — used by `Variant::Mystery`
    /// to look up how many of its target color a move cleared.
    pub fn index(self) -> usize {
        Color::ALL.iter().position(|&c| c == self).unwrap()
    }
}

/// The three colored bonus tiles, spawned by matching more than the minimum 3-in-a-row —
/// see `find_matches_with_spawns`. `ColorBomb` (colorless, wild) lives on `Tile` directly
/// rather than here since it has no `Color` of its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Special {
    /// Clears its entire row when triggered. Spawned by a horizontal 4-in-a-row match
    /// — aligned with the match's own direction.
    RowClear,
    /// Clears its entire column when triggered. Spawned by a vertical 4-in-a-row
    /// match — aligned with the match's own direction.
    ColClear,
    /// Clears the 3x3 block centered on itself (clamped to the board edge) when
    /// triggered. Spawned by a match with both a horizontal and a vertical run of at
    /// least 3 sharing a cell (an L/T shape, 5 cells total).
    Wrapped,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tile {
    Plain(Color),
    /// A colored bonus tile — still matches normally by color (that's how it usually
    /// gets triggered) but clears extra cells when consumed.
    Bonus(Color, Special),
    /// Colorless wild tile spawned by a straight 5-in-a-row match. Never joins an
    /// ordinary color match itself; only ever triggered by being swapped directly or by
    /// getting swept into another special's effect area.
    ColorBomb,
    /// The `Ingredients` variant's collection goal — immune to matching/bonus effects,
    /// only moves via gravity, and is collected the instant it settles on the bottom
    /// row (see `compact_and_refill`).
    Ingredient,
}

impl Tile {
    fn color(self) -> Option<Color> {
        match self {
            Tile::Plain(c) | Tile::Bonus(c, _) => Some(c),
            Tile::ColorBomb | Tile::Ingredient => None,
        }
    }

    fn is_special(self) -> bool {
        matches!(self, Tile::Bonus(..) | Tile::ColorBomb)
    }
}

pub type Tiles = [[Tile; W]; H];
pub type Jelly = [[u8; W]; H];

type Pos = (usize, usize);
type Cleared = HashSet<Pos>;
type Spawns = Vec<(Pos, Tile)>;

#[derive(Clone)]
pub struct Board {
    pub tiles: Tiles,
    /// Layers of jelly under each cell (0 = none) — only ever nonzero in `Variant::Jelly`.
    /// Decremented (not tile-color-gated) whenever that cell is cleared by any match or
    /// bonus effect.
    pub jelly: Jelly,
    /// The color palette this board draws fresh tiles from — a runtime subset of
    /// `Color::ALL`, sized by `LevelParams::color_count` for `Variant::Mode::Levels`
    /// (always the full 6 for the free-cycling variants). Carried on `Board` itself
    /// (rather than threaded through every gravity/reshuffle call separately) so it stays
    /// consistent automatically across every clone this game already makes —
    /// `simulate`'s scratch boards, each wave's `board_before`/`board_after`, etc.
    pub active_colors: Vec<Color>,
    /// This board's own randomness stream — see `Rng`'s doc comment for why it lives
    /// here rather than going through the ambient global `macroquad::rand`. `pub(crate)`
    /// so `solver.rs`'s `beam_solver::SearchState::apply` impl can also reseed it via
    /// `preview_seed` — see that impl for why it needs to.
    pub(crate) rng: Rng,
}

/// A tile arriving at `to_row` in `col`, animated by the renderer from `from_row`
/// (its row before this wave's gravity/refill, or a negative row above the board for a
/// freshly spawned tile queued to fall in from off-screen) down to `to_row`.
#[derive(Clone, Copy)]
pub struct FallEntry {
    pub col: usize,
    pub from_row: i32,
    pub to_row: usize,
    pub tile: Tile,
}

/// One clear-and-settle round of a move's resolution — a straight-swap always produces
/// wave 0; a cascade (a fall creating a fresh match) produces more. The renderer plays
/// these back in order: flash `cleared` over `board_before`, then animate `falls` into
/// `board_after`.
pub struct Wave {
    pub board_before: Board,
    pub cleared: Vec<(usize, usize)>,
    /// Cells that got a *new* bonus tile this wave instead of being cleared — the
    /// renderer fades the new tile in here rather than clearing it.
    pub spawned: Vec<(usize, usize)>,
    pub falls: Vec<FallEntry>,
    pub board_after: Board,
}

/// The full outcome of one `Game::apply`/`Game::simulate` call — everything the
/// animation and the solver's scoring both need.
pub struct Resolution {
    pub mv: Move,
    /// True when the move directly swapped two bonus tiles together (a deliberate
    /// combo activation, scored extra by the solver and called out in the HUD) rather
    /// than an ordinary match or a lone bonus trigger.
    pub combo: bool,
    pub waves: Vec<Wave>,
    pub score_gained: u32,
    pub jelly_cleared: u32,
    pub ingredients_collected: u32,
    /// How many cells of each `Color` (indexed via `Color::index`) this move cleared —
    /// computed unconditionally, same as `jelly_cleared`/`ingredients_collected`, since
    /// `resolve()` doesn't know which `Variant` is asking. Only `Variant::Mystery` reads
    /// it (one lookup per active `Game::mystery_goals` entry), same relationship as the
    /// other two goal counters to their own variant.
    pub color_cleared: [u32; Color::ALL.len()],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    Score,
    Jelly,
    Ingredients,
    Timed,
    /// Clear every goal in `Game::mystery_goals` (1-3 colors, each with its own target —
    /// "hit all to win") — the "collect N of a specific color" goal (real match-3 games'
    /// most common goal type per `.notes/match3_todo.md` item #4), generalized to a small
    /// combination of simultaneous color targets rather than always exactly one. Reuses
    /// the plain `Score`-shaped board (no special board-gen needed) and counts cleared
    /// cells by color via `Resolution::color_cleared`.
    Mystery,
}

/// One color target within a `Variant::Mystery` episode — `Game::mystery_goals` holds
/// 1-3 of these, all of which must reach `target` to win.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MysteryGoal {
    pub color: Color,
    pub target: u32,
    pub collected: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Won,
    OutOfMoves,
    TimeUp,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    Over(Outcome),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Move {
    pub a: (usize, usize),
    pub b: (usize, usize),
}

const SCORE_TARGET: u32 = 3100;
const SCORE_MOVE_LIMIT: u32 = 20;
const JELLY_MOVE_LIMIT: u32 = 25;
const JELLY_CELL_COUNT: usize = 24;
const INGREDIENTS_TARGET: u32 = 3;
const INGREDIENTS_MOVE_LIMIT: u32 = 26;
pub const TIME_LIMIT: f32 = 60.0;
const MYSTERY_MOVE_LIMIT: u32 = 20;
/// Max simultaneous color goals a `Mystery` episode can roll — 1-3, chosen at board-gen
/// time (`gen_mystery_goals`). More goals means the same `MYSTERY_MOVE_LIMIT` has to cover
/// all of them, so each goal's *own* target has to shrink as the count grows to stay
/// winnable — see `MYSTERY_TARGET_RANGES`.
const MYSTERY_MAX_COLORS: usize = 3;
/// Inclusive per-goal target range for each goal count, indexed `[count - 1]` — "various
/// targets" per design (each goal in a multi-goal episode rolls its own value in range,
/// not one shared number). Tuned via the standard three-disjoint-150-seed-range sweep
/// per count; see CLAUDE.md's "Mystery" section for the measured win rates.
const MYSTERY_TARGET_RANGES: [(u32, u32); MYSTERY_MAX_COLORS] = [(28, 34), (24, 30), (22, 28)];

/// One entry in the hand-authored level line `LEVELS` that `VariantMode::Levels` (see
/// `main.rs`) steps through — reuses the same four `Variant` win-conditions as the
/// free-cycling modes above, but each level tunes its own target/move-limit numbers
/// instead of reading the fixed `SCORE_TARGET`/`JELLY_CELL_COUNT`/etc. constants, so
/// difficulty can ramp across the line. `move_limit`/`jelly_cell_count`/
/// `ingredients_target`/`time_limit` are only meaningful for the corresponding
/// `variant` (e.g. `time_limit` is ignored by a `Score` level) — see `Game::new_level`.
/// `color_count` applies to every variant, unlike those — fewer simultaneous colors
/// means more frequent incidental matches (real match-3 games' standard difficulty
/// ramp), so early levels use a narrower palette and later ones widen back to the full
/// `Color::ALL` (see `MIN_LEVEL_COLORS` for the floor `gen_plain_tiles`' avoid-immediate-
/// match retry loop needs to still terminate).
#[derive(Clone, Copy)]
pub struct LevelParams {
    pub name: &'static str,
    pub variant: Variant,
    pub score_target: u32,
    pub move_limit: u32,
    pub jelly_cell_count: usize,
    pub ingredients_target: u32,
    pub time_limit: f32,
    pub color_count: usize,
}

/// `gen_plain_tiles`' per-cell retry loop excludes at most 2 distinct colors (one to
/// avoid extending a horizontal run, one for vertical) before it finds a legal fill —
/// with only 2 active colors, both could be excluded simultaneously and the loop would
/// spin forever. 3 is the hard floor; `LEVELS` stays at 4+ for actual difficulty
/// headroom on top of that safety margin.
const MIN_LEVEL_COLORS: usize = 3;

/// After this many consecutive losses on the same level, `Session::next_generation`
/// (main.rs) advances to the next level anyway rather than replaying it forever — there's
/// no player to eventually git gud, just a fixed-heuristic bot, so an unwinnable level
/// would otherwise stall the line permanently. See `CLAUDE.md`'s level-progression
/// section.
pub const LEVEL_STUCK_LIMIT: u32 = 5;

/// The level line `VariantMode::Levels` cycles through, looping back to the start after
/// the last one. Win rates are seed-swept and floor-checked by the `level_line_win_rates`
/// test (`cargo test --release level_line_win_rates -- --ignored --nocapture`) — re-run it
/// after changing any of these numbers or the solver's scoring weights.
pub const LEVELS: &[LevelParams] = &[
    LevelParams {
        name: "Warm-Up",
        variant: Variant::Score,
        score_target: 1000,
        move_limit: 20,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 4,
    },
    LevelParams {
        name: "Sticky Start",
        variant: Variant::Jelly,
        score_target: 0,
        move_limit: 20,
        jelly_cell_count: 10,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 4,
    },
    LevelParams {
        name: "First Delivery",
        variant: Variant::Ingredients,
        score_target: 0,
        move_limit: 18,
        jelly_cell_count: 0,
        ingredients_target: 1,
        time_limit: 0.0,
        color_count: 4,
    },
    LevelParams {
        name: "Color Search",
        variant: Variant::Mystery,
        score_target: 0,
        move_limit: 20,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 4,
    },
    LevelParams {
        name: "Building Up",
        variant: Variant::Score,
        score_target: 1800,
        move_limit: 20,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 4,
    },
    LevelParams {
        name: "Jelly Patch",
        variant: Variant::Jelly,
        score_target: 0,
        move_limit: 22,
        jelly_cell_count: 16,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 5,
    },
    LevelParams {
        name: "Two by Two",
        variant: Variant::Ingredients,
        score_target: 0,
        move_limit: 22,
        jelly_cell_count: 0,
        ingredients_target: 2,
        time_limit: 0.0,
        color_count: 5,
    },
    LevelParams {
        name: "Rainbow Hunt",
        variant: Variant::Mystery,
        score_target: 0,
        move_limit: 22,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 5,
    },
    LevelParams {
        name: "Quick Hands",
        variant: Variant::Timed,
        score_target: 6200,
        move_limit: 0,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 45.0,
        color_count: 5,
    },
    LevelParams {
        name: "Point Rush",
        variant: Variant::Score,
        score_target: 2600,
        move_limit: 20,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 5,
    },
    LevelParams {
        name: "Deep Jelly",
        variant: Variant::Jelly,
        score_target: 0,
        move_limit: 24,
        jelly_cell_count: 22,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 6,
    },
    LevelParams {
        name: "Full Batch",
        variant: Variant::Ingredients,
        score_target: 0,
        move_limit: 28,
        jelly_cell_count: 0,
        ingredients_target: 3,
        time_limit: 0.0,
        color_count: 6,
    },
    LevelParams {
        name: "Color Storm",
        variant: Variant::Mystery,
        score_target: 0,
        move_limit: 24,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 6,
    },
    LevelParams {
        name: "Against the Clock",
        variant: Variant::Timed,
        score_target: 8200,
        move_limit: 0,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 55.0,
        color_count: 6,
    },
    LevelParams {
        name: "Grand Finale",
        variant: Variant::Score,
        score_target: 3000,
        move_limit: 18,
        jelly_cell_count: 0,
        ingredients_target: 0,
        time_limit: 0.0,
        color_count: 6,
    },
];

#[derive(Clone)]
pub struct Game {
    pub board: Board,
    pub variant: Variant,
    pub generation: u32,
    pub score: u32,
    pub moves_used: u32,
    pub move_limit: u32,
    pub score_target: u32,
    pub jelly_remaining: u32,
    pub ingredients_target: u32,
    pub ingredients_collected: u32,
    pub time_remaining: f32,
    /// Only meaningful for `Variant::Mystery` — 1-3 independent color goals, all of which
    /// must reach their `target` to win. Empty for every other variant, same pattern as
    /// `ingredients_target` being carried but unused outside `Variant::Ingredients`.
    pub mystery_goals: Vec<MysteryGoal>,
    pub phase: Phase,
    /// Times `apply` has had to bail the board out via `reshuffle` this episode — a
    /// deadlock-safety net, not solver skill. Seed-sweep win-rate validation should
    /// exclude (or note) episodes where this is nonzero: a reshuffle can hand the solver
    /// a materially easier or harder board mid-episode, unrelated to whatever change is
    /// being measured. See CLAUDE.md's "Goal-aware solver tuning" methodology note.
    pub reshuffles: u32,
}

impl Game {
    pub fn new(variant: Variant, generation: u32) -> Self {
        let mut board = gen_board(variant, JELLY_CELL_COUNT, INGREDIENTS_TARGET, &Color::ALL);
        let jelly_remaining = board.jelly.iter().flatten().filter(|&&j| j > 0).count() as u32;
        // Computed from `board.active_colors` (a borrow) before `board` moves into the
        // struct literal below — see `gen_mystery_goals`' doc comment for why it can't
        // just default to `Color::ALL`. Draws from the board's own `rng` (not the
        // global RNG) to stay part of that board's isolated stream — see `Rng`'s doc
        // comment.
        let mystery_goals = if variant == Variant::Mystery {
            gen_mystery_goals(&board.active_colors.clone(), &mut board.rng)
        } else {
            Vec::new()
        };
        Self {
            board,
            variant,
            generation,
            score: 0,
            moves_used: 0,
            move_limit: match variant {
                Variant::Score => SCORE_MOVE_LIMIT,
                Variant::Jelly => JELLY_MOVE_LIMIT,
                Variant::Ingredients => INGREDIENTS_MOVE_LIMIT,
                Variant::Timed => u32::MAX,
                Variant::Mystery => MYSTERY_MOVE_LIMIT,
            },
            // 0 for `Timed`: that variant has no win condition (see `update_phase`) —
            // its `score_target` is otherwise dead, kept at 0 rather than `SCORE_TARGET`
            // so it can't accidentally satisfy a future win check added for that arm.
            score_target: if variant == Variant::Timed {
                0
            } else {
                SCORE_TARGET
            },
            jelly_remaining,
            ingredients_target: INGREDIENTS_TARGET,
            ingredients_collected: 0,
            time_remaining: TIME_LIMIT,
            mystery_goals,
            phase: Phase::Playing,
            reshuffles: 0,
        }
    }

    /// Same shape as `Game::new`, but every target/move-limit number comes from a
    /// `LEVELS` entry instead of this module's fixed per-variant constants — see
    /// `VariantMode::Levels` (main.rs).
    pub fn new_level(params: LevelParams, generation: u32) -> Self {
        let variant = params.variant;
        debug_assert!(
            (MIN_LEVEL_COLORS..=Color::ALL.len()).contains(&params.color_count),
            "level {} color_count {} out of range",
            params.name,
            params.color_count
        );
        let mut board = gen_board(
            variant,
            params.jelly_cell_count,
            params.ingredients_target,
            &Color::ALL[..params.color_count],
        );
        let jelly_remaining = board.jelly.iter().flatten().filter(|&&j| j > 0).count() as u32;
        // Same "compute from a borrow of `board` before it moves into the struct literal"
        // shape as `Game::new` — goals are rolled fresh per episode (not stored on
        // `LevelParams`) same as the free-cycling variant, so a `Mystery` level just needs
        // `color_count`/`move_limit` like any other field, not a dedicated goal list.
        let mystery_goals = if variant == Variant::Mystery {
            gen_mystery_goals(&board.active_colors.clone(), &mut board.rng)
        } else {
            Vec::new()
        };
        Self {
            board,
            variant,
            generation,
            score: 0,
            moves_used: 0,
            move_limit: match variant {
                Variant::Timed => u32::MAX,
                _ => params.move_limit,
            },
            score_target: params.score_target,
            jelly_remaining,
            ingredients_target: params.ingredients_target,
            ingredients_collected: 0,
            time_remaining: params.time_limit,
            mystery_goals,
            phase: Phase::Playing,
            reshuffles: 0,
        }
    }

    pub fn remaining_moves(&self) -> u32 {
        self.move_limit.saturating_sub(self.moves_used)
    }

    /// Every adjacent swap that's legal right now: always legal if it activates a bonus
    /// tile (solo trigger or combo — see `classify_swap`), otherwise only if it forms an
    /// ordinary match. Ingredient tiles can never be swapped at all.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut out = Vec::new();
        for r in 0..H {
            for c in 0..W {
                for &(dr, dc) in &[(0i32, 1i32), (1, 0)] {
                    let (nr, nc) = (r as i32 + dr, c as i32 + dc);
                    if nr < 0 || nc < 0 || nr as usize >= H || nc as usize >= W {
                        continue;
                    }
                    let (nr, nc) = (nr as usize, nc as usize);
                    let a = (r, c);
                    let b = (nr, nc);
                    if is_legal_swap(&self.board.tiles, a, b) {
                        out.push(Move { a, b });
                    }
                }
            }
        }
        out
    }

    /// Resolves `mv` against a clone of the board without touching `self` — used by the
    /// solver to score every candidate move before committing to one. The clone's `rng`
    /// is reseeded from `preview_seed` rather than carrying over `self.board.rng`'s
    /// current state — see `Rng`'s doc comment for why a straight carry-over would make
    /// this an oracle for whatever move the caller actually applies next.
    pub fn simulate(&self, mv: Move) -> Resolution {
        let mut board = self.board.clone();
        board.rng = Rng::seeded(preview_seed(&board, mv));
        resolve(&mut board, mv)
    }

    /// Applies `mv` for real: mutates `self.board`/score/goal counters, advances
    /// `phase`, and — since a solver should never face a truly stuck board — reshuffles
    /// in place if the resulting board has no legal move left.
    pub fn apply(&mut self, mv: Move) -> Resolution {
        let res = resolve(&mut self.board, mv);
        self.score += res.score_gained;
        self.moves_used += 1;
        if self.variant == Variant::Jelly {
            self.jelly_remaining = self.jelly_remaining.saturating_sub(res.jelly_cleared);
        }
        if self.variant == Variant::Ingredients {
            self.ingredients_collected += res.ingredients_collected;
        }
        if self.variant == Variant::Mystery {
            for goal in &mut self.mystery_goals {
                goal.collected += res.color_cleared[goal.color.index()];
            }
        }
        self.update_phase();
        if self.phase == Phase::Playing && self.legal_moves().is_empty() {
            reshuffle(&mut self.board);
            self.reshuffles += 1;
        }
        res
    }

    /// Ticks the `Timed` variant's countdown — the other three variants are paced by
    /// move count instead, via `apply`. No-op (and never called into a countdown that
    /// goes negative) for those.
    pub fn tick_time(&mut self, dt: f32) {
        if self.variant != Variant::Timed || self.phase != Phase::Playing {
            return;
        }
        self.time_remaining = (self.time_remaining - dt).max(0.0);
        if self.time_remaining <= 0.0 {
            self.phase = Phase::Over(Outcome::TimeUp);
        }
    }

    fn update_phase(&mut self) {
        if self.phase != Phase::Playing {
            return;
        }
        let won = match self.variant {
            Variant::Score => self.score >= self.score_target,
            Variant::Jelly => self.jelly_remaining == 0,
            Variant::Ingredients => self.ingredients_collected >= self.ingredients_target,
            // The free-cycling `Timed` variant has no win condition, only `TimeUp`
            // (`score_target` is 0 for it — see `Game::new`); a `Timed` *level* gives
            // it a real target to race the clock against instead.
            Variant::Timed => self.score_target > 0 && self.score >= self.score_target,
            Variant::Mystery => {
                !self.mystery_goals.is_empty()
                    && self.mystery_goals.iter().all(|g| g.collected >= g.target)
            }
        };
        if won {
            self.phase = Phase::Over(Outcome::Won);
        } else if self.variant != Variant::Timed && self.moves_used >= self.move_limit {
            self.phase = Phase::Over(Outcome::OutOfMoves);
        }
    }
}

// ── Swap classification ──────────────────────────────────────────────────────────

enum SwapKind {
    Illegal,
    /// Both tiles are bonuses (or one/both are a `ColorBomb`) — a deliberate combo
    /// activation, always legal, no ordinary match required.
    Combo,
    /// Exactly one tile is a bonus (or a `ColorBomb`) — always legal, triggers that
    /// tile's own effect at its post-swap position.
    SoloTrigger,
    /// Two plain tiles — legal only if the swap actually forms a match.
    Normal,
}

fn classify_swap(a: Tile, b: Tile) -> SwapKind {
    if matches!(a, Tile::Ingredient) || matches!(b, Tile::Ingredient) {
        return SwapKind::Illegal;
    }
    match (a.is_special(), b.is_special()) {
        (true, true) => SwapKind::Combo,
        (true, false) | (false, true) => SwapKind::SoloTrigger,
        (false, false) => SwapKind::Normal,
    }
}

pub(crate) fn is_legal_swap(tiles: &Tiles, a: (usize, usize), b: (usize, usize)) -> bool {
    let ta = tiles[a.0][a.1];
    let tb = tiles[b.0][b.1];
    match classify_swap(ta, tb) {
        SwapKind::Illegal => false,
        SwapKind::Combo | SwapKind::SoloTrigger => true,
        SwapKind::Normal => {
            let mut t = *tiles;
            t[a.0][a.1] = tb;
            t[b.0][b.1] = ta;
            has_match_at(&t, a) || has_match_at(&t, b)
        }
    }
}

/// Same-color run length through `pos` in `dir` (`(0,1)` for a row-run, `(1,0)` for a
/// column-run), `pos` itself included.
fn run_len(tiles: &Tiles, pos: (usize, usize), dir: (i32, i32)) -> usize {
    let Some(color) = tiles[pos.0][pos.1].color() else {
        return 0;
    };
    let mut len = 1;
    for sign in [1i32, -1] {
        let (mut r, mut c) = (pos.0 as i32, pos.1 as i32);
        loop {
            r += dir.0 * sign;
            c += dir.1 * sign;
            if r < 0 || c < 0 || r as usize >= H || c as usize >= W {
                break;
            }
            if tiles[r as usize][c as usize].color() != Some(color) {
                break;
            }
            len += 1;
        }
    }
    len
}

fn has_match_at(tiles: &Tiles, pos: (usize, usize)) -> bool {
    run_len(tiles, pos, (0, 1)) >= 3 || run_len(tiles, pos, (1, 0)) >= 3
}

// ── Match search + bonus spawning (the `Normal` swap path) ─────────────────────────

/// Every matched cell on the board right now, plus the bonus tile (position, kind) each
/// run of 4+ spawns — the spawn cell is excluded from the returned matched set (it keeps
/// its new tile rather than being cleared). `prefer` cells (typically the two
/// just-swapped positions) get first claim on a run's spawn slot, since that's the
/// tile the player/bot visibly moved into place; runs not touching them spawn at their
/// middle cell.
fn find_matches_with_spawns(tiles: &Tiles, prefer: &[Pos]) -> (Cleared, Spawns) {
    let mut matched = HashSet::new();
    let mut runs: Vec<(Pos, (i32, i32), usize)> = Vec::new(); // (start, dir, len)

    for r in 0..H {
        let mut c = 0;
        while c < W {
            let len = run_len_from(tiles, (r, c), (0, 1));
            if len >= 3 {
                for k in 0..len {
                    matched.insert((r, c + k));
                }
                runs.push(((r, c), (0, 1), len));
            }
            c += len.max(1);
        }
    }
    for c in 0..W {
        let mut r = 0;
        while r < H {
            let len = run_len_from(tiles, (r, c), (1, 0));
            if len >= 3 {
                for k in 0..len {
                    matched.insert((r + k, c));
                }
                runs.push(((r, c), (1, 0), len));
            }
            r += len.max(1);
        }
    }

    // L/T intersections: any matched cell with both a horizontal and vertical run of
    // >=3 through it spawns a Wrapped tile there instead of the run-length rule below.
    let mut wrapped_at = HashSet::new();
    for &pos in &matched {
        if run_len(tiles, pos, (0, 1)) >= 3 && run_len(tiles, pos, (1, 0)) >= 3 {
            wrapped_at.insert(pos);
        }
    }

    let mut spawns: Vec<((usize, usize), Tile)> = Vec::new();
    let mut spawn_cells: HashSet<(usize, usize)> = HashSet::new();
    // Sorted rather than iterated straight off the `HashSet` — `std::collections::HashSet`'s
    // default hasher is randomly seeded per process, so picking a tie-break via `.next()`/
    // `.find()` directly off it made which of several *simultaneous* L/T-shape matches
    // (a real if rare board state) gets the `Wrapped` spawn — and everything downstream of
    // that (subsequent matches, RNG-consuming refills, the whole rest of the episode) —
    // differ between two runs of the *same* `HCG_SEED`. That silently broke this repo's
    // "HCG_SEED reproduces a run" contract (root CLAUDE.md's "Native CLI flags"); sorting
    // first makes the tie-break (arbitrary either way) deterministic instead.
    let mut wrapped_sorted: Vec<Pos> = wrapped_at.into_iter().collect();
    wrapped_sorted.sort_unstable();
    if let Some(&pos) = wrapped_sorted.iter().find(|p| prefer.contains(p)) {
        spawns.push((
            pos,
            Tile::Bonus(tiles[pos.0][pos.1].color().unwrap(), Special::Wrapped),
        ));
        spawn_cells.insert(pos);
    } else if let Some(&pos) = wrapped_sorted.first() {
        spawns.push((
            pos,
            Tile::Bonus(tiles[pos.0][pos.1].color().unwrap(), Special::Wrapped),
        ));
        spawn_cells.insert(pos);
    }

    for (start, dir, len) in runs {
        let cells: Vec<(usize, usize)> = (0..len)
            .map(|k| (start.0 + dir.0 as usize * k, start.1 + dir.1 as usize * k))
            .collect();
        if cells.iter().any(|c| spawn_cells.contains(c)) {
            continue; // already spawning a Wrapped somewhere in this run
        }
        let color = tiles[start.0][start.1].color().unwrap();
        let spawn_pos = cells
            .iter()
            .find(|c| prefer.contains(*c))
            .copied()
            .unwrap_or(cells[len / 2]);
        if len >= 5 {
            spawns.push((spawn_pos, Tile::ColorBomb));
            spawn_cells.insert(spawn_pos);
        } else if len == 4 {
            // Aligned with the match's own direction — a horizontal run of 4 spawns a
            // tile that clears its row, a vertical run spawns one that clears its
            // column (not perpendicular, unlike some other match-3 games' convention).
            let special = if dir == (0, 1) {
                Special::RowClear
            } else {
                Special::ColClear
            };
            spawns.push((spawn_pos, Tile::Bonus(color, special)));
            spawn_cells.insert(spawn_pos);
        }
    }

    for pos in &spawn_cells {
        matched.remove(pos);
    }
    (matched, spawns)
}

fn run_len_from(tiles: &Tiles, pos: (usize, usize), dir: (i32, i32)) -> usize {
    // Only counts a run starting exactly at `pos` (i.e. the cell "before" it in `dir`
    // is out of bounds or a different color) — used to walk row/col scans without
    // recounting the same run from every cell inside it.
    let prev = (pos.0 as i32 - dir.0, pos.1 as i32 - dir.1);
    if prev.0 >= 0 && prev.1 >= 0 && (prev.0 as usize) < H && (prev.1 as usize) < W {
        let prev = (prev.0 as usize, prev.1 as usize);
        if tiles[prev.0][prev.1].color().is_some()
            && tiles[prev.0][prev.1].color() == tiles[pos.0][pos.1].color()
        {
            return 0;
        }
    }
    run_len(tiles, pos, dir)
}

// ── Bonus effects + combo matrix ────────────────────────────────────────────────────

fn majority_color(tiles: &Tiles) -> Color {
    let mut counts = [0u32; Color::ALL.len()];
    for row in tiles {
        for tile in row {
            if let Some(color) = tile.color() {
                counts[Color::ALL.iter().position(|&c| c == color).unwrap()] += 1;
            }
        }
    }
    let (idx, _) = counts.iter().enumerate().max_by_key(|&(_, n)| *n).unwrap();
    Color::ALL[idx]
}

/// Cells a single bonus tile clears when triggered. `target_color` is only consulted
/// for `ColorBomb` — pass the swap partner's color when this bomb was just deliberately
/// swapped into place, `None` for one swept up incidentally by a chain, which falls
/// back to whatever color currently dominates the board.
fn effect_cells(
    tiles: &Tiles,
    tile: Tile,
    pos: (usize, usize),
    target_color: Option<Color>,
) -> Vec<(usize, usize)> {
    match tile {
        Tile::Bonus(_, Special::RowClear) => (0..W).map(|c| (pos.0, c)).collect(),
        Tile::Bonus(_, Special::ColClear) => (0..H).map(|r| (r, pos.1)).collect(),
        Tile::Bonus(_, Special::Wrapped) => block(pos, 1),
        Tile::ColorBomb => {
            let color = target_color.unwrap_or_else(|| majority_color(tiles));
            let mut cells = Vec::new();
            for (r, row) in tiles.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    if cell.color() == Some(color) {
                        cells.push((r, c));
                    }
                }
            }
            cells
        }
        _ => Vec::new(),
    }
}

/// Every cell within `radius` of `center` (Chebyshev distance), clamped to the board.
fn block(center: (usize, usize), radius: i32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for dr in -radius..=radius {
        for dc in -radius..=radius {
            let (r, c) = (center.0 as i32 + dr, center.1 as i32 + dc);
            if r >= 0 && c >= 0 && (r as usize) < H && (c as usize) < W {
                out.push((r as usize, c as usize));
            }
        }
    }
    out
}

fn thick_cross(center: (usize, usize)) -> HashSet<(usize, usize)> {
    let mut out = HashSet::new();
    for dr in -1i32..=1 {
        let r = center.0 as i32 + dr;
        if r >= 0 && (r as usize) < H {
            out.extend((0..W).map(|c| (r as usize, c)));
        }
    }
    for dc in -1i32..=1 {
        let c = center.1 as i32 + dc;
        if c >= 0 && (c as usize) < W {
            out.extend((0..H).map(|r| (r, c as usize)));
        }
    }
    out
}

/// The two swapped tiles are both bonuses — a deliberate combo, always stronger than
/// what incidentally chain-triggering the same two tiles separately would produce (see
/// `chain_specials`). `at`/`bt` are the post-swap tiles at `a`/`b` (i.e. `bt` is the tile
/// that used to be at `a`, now sitting at... no: callers pass the tile now occupying
/// each position, already swapped).
fn combo_cells(
    tiles: &Tiles,
    a: (usize, usize),
    at: Tile,
    b: (usize, usize),
    bt: Tile,
) -> HashSet<(usize, usize)> {
    use Special::*;
    use Tile::*;
    match (at, bt) {
        (ColorBomb, ColorBomb) => (0..H).flat_map(|r| (0..W).map(move |c| (r, c))).collect(),
        (ColorBomb, Bonus(color, special)) | (Bonus(color, special), ColorBomb) => {
            // The colorless `ColorBomb` itself never matches `color`, so it wouldn't
            // otherwise be included in its own combo's cleared cells — insert whichever
            // of `a`/`b` it's actually sitting at post-swap explicitly.
            let mut out: HashSet<_> = if at == ColorBomb { [a] } else { [b] }
                .into_iter()
                .collect();
            for r in 0..H {
                for c in 0..W {
                    if tiles[r][c].color() == Some(color) {
                        out.insert((r, c));
                        out.extend(effect_cells(
                            tiles,
                            Tile::Bonus(color, special),
                            (r, c),
                            None,
                        ));
                    }
                }
            }
            out
        }
        (Bonus(_, Wrapped), Bonus(_, Wrapped)) => {
            let mut out: HashSet<_> = block(a, 2).into_iter().collect();
            out.extend(block(b, 2));
            out
        }
        (Bonus(..), Bonus(..)) => {
            let mut out = thick_cross(a);
            out.extend(thick_cross(b));
            out
        }
        _ => HashSet::new(),
    }
}

/// Repeatedly folds in the effect cells of any *pre-existing* bonus tile that ends up
/// inside `cleared` (a match sweeping through a striped candy triggers it too, and so
/// on transitively) — bounded by the board size, so this always terminates. Cells in
/// `spawn_cells` are exempt (a cell becoming a brand-new bonus this wave doesn't also
/// detonate itself).
fn chain_specials(
    tiles: &Tiles,
    cleared: &mut HashSet<(usize, usize)>,
    spawn_cells: &HashSet<(usize, usize)>,
) {
    let mut queue: VecDeque<(usize, usize)> = cleared.iter().copied().collect();
    let mut processed: HashSet<(usize, usize)> = HashSet::new();
    while let Some(pos) = queue.pop_front() {
        if !processed.insert(pos) || spawn_cells.contains(&pos) {
            continue;
        }
        let tile = tiles[pos.0][pos.1];
        if tile.is_special() {
            for cell in effect_cells(tiles, tile, pos, None) {
                if cleared.insert(cell) {
                    queue.push_back(cell);
                }
            }
        }
    }
}

// ── Scoring ──────────────────────────────────────────────────────────────────────

fn score_for(cleared: usize, wave_index: usize) -> u32 {
    cleared as u32 * 10 * (wave_index as u32 + 1)
}

// ── Gravity / refill ────────────────────────────────────────────────────────────────

/// Compacts every column downward past `cleared` cells, collects an ingredient the
/// instant it settles on the bottom row, and refills the vacated top cells with fresh
/// random tiles — returning per-tile fall animations for the renderer and the count of
/// ingredients collected this pass.
fn compact_and_refill(
    board: &mut Board,
    cleared: &HashSet<(usize, usize)>,
) -> (Vec<FallEntry>, u32) {
    let mut falls = Vec::new();
    let mut collected = 0u32;
    // Cloned up front rather than read through `board.active_colors` inside the loop
    // below — the loop also mutates `board.tiles`, and this avoids holding a borrow of
    // `board` across that.
    let active_colors = board.active_colors.clone();

    for col in 0..W {
        let mut survivors: Vec<(usize, Tile)> = Vec::new(); // (original row, tile)
        for row in 0..H {
            if !cleared.contains(&(row, col)) {
                survivors.push((row, board.tiles[row][col]));
            }
        }

        // An ingredient that just landed on the bottom row this pass is collected
        // instead of settling there — it never actually gets drawn at row H-1. Guarded
        // by "wasn't already the bottom occupant" so an ingredient that settled on a
        // previous wave and simply has nothing new to fall onto it isn't re-collected
        // every subsequent wave.
        let just_landed_at_bottom = matches!(survivors.last(), Some(&(_, Tile::Ingredient)))
            && !matches!(board.tiles[H - 1][col], Tile::Ingredient);
        if just_landed_at_bottom {
            survivors.pop();
            collected += 1;
        }

        // Jelly belongs to the cell (a fixed goal layout drawn under whatever tile
        // currently sits there), not to any particular tile — it must NOT travel with
        // falling tiles, so `board.jelly` is left untouched here; only `tiles` moves.
        let deficit = H - survivors.len();
        let mut new_col = [Tile::Plain(Color::Red); H];
        for (i, &(from_row, tile)) in survivors.iter().enumerate() {
            let to_row = deficit + i;
            new_col[to_row] = tile;
            if from_row != to_row {
                falls.push(FallEntry {
                    col,
                    from_row: from_row as i32,
                    to_row,
                    tile,
                });
            }
        }
        for (i, slot) in new_col.iter_mut().enumerate().take(deficit) {
            let tile = Tile::Plain(Color::random_from(&active_colors, &mut board.rng));
            *slot = tile;
            falls.push(FallEntry {
                col,
                from_row: i as i32 - deficit as i32 - 1,
                to_row: i,
                tile,
            });
        }
        for (row, &tile) in new_col.iter().enumerate() {
            board.tiles[row][col] = tile;
        }
    }

    (falls, collected)
}

// ── Resolution driver ────────────────────────────────────────────────────────────────

pub(crate) fn resolve(board: &mut Board, mv: Move) -> Resolution {
    let a = mv.a;
    let b = mv.b;
    let tile_a = board.tiles[a.0][a.1];
    let tile_b = board.tiles[b.0][b.1];
    board.tiles[a.0][a.1] = tile_b;
    board.tiles[b.0][b.1] = tile_a;

    let kind = classify_swap(tile_a, tile_b);
    let combo = matches!(kind, SwapKind::Combo);

    let (mut cleared, mut spawns): (Cleared, Spawns) = match kind {
        SwapKind::Combo => (combo_cells(&board.tiles, a, tile_b, b, tile_a), Vec::new()),
        SwapKind::SoloTrigger => {
            // `tile_b` (special) lands at position `a` post-swap and vice versa (see
            // the swap two lines up) — the effect must anchor on where the special tile
            // now sits, not where it started.
            let (special_pos, special_tile, other_color) = if tile_b.is_special() {
                (a, tile_b, tile_a.color())
            } else {
                (b, tile_a, tile_b.color())
            };
            // `effect_cells` is color-filtered for `ColorBomb`, which is itself
            // colorless — without this it never clears (isn't "consumed by") its own
            // triggering swap, only the tiles it targets.
            let mut cells: Cleared =
                effect_cells(&board.tiles, special_tile, special_pos, other_color)
                    .into_iter()
                    .collect();
            cells.insert(special_pos);
            (cells, Vec::new())
        }
        SwapKind::Normal => find_matches_with_spawns(&board.tiles, &[a, b]),
        SwapKind::Illegal => unreachable!("resolve is only ever called with a legal move"),
    };

    let mut waves = Vec::new();
    let mut score_gained = 0u32;
    let mut jelly_cleared = 0u32;
    let mut ingredients_collected = 0u32;
    let mut color_cleared = [0u32; Color::ALL.len()];

    loop {
        let spawn_cells: HashSet<(usize, usize)> = spawns.iter().map(|(p, _)| *p).collect();
        chain_specials(&board.tiles, &mut cleared, &spawn_cells);
        // Ingredient tiles are immune to matches and bonus effects (see `Tile::Ingredient`'s
        // doc comment) — a `RowClear`/`ColClear`/`Wrapped`/`ColorBomb` effect area is
        // computed purely geometrically/by-color and doesn't know to route around one, so
        // this is the one place that enforces the immunity: strip any cell that happens to
        // hold an ingredient back out of `cleared` before it's scored or compacted away.
        cleared.retain(|&(r, c)| !matches!(board.tiles[r][c], Tile::Ingredient));
        if cleared.is_empty() {
            break;
        }

        let board_before = board.clone();
        score_gained += score_for(cleared.len(), waves.len());
        for &(r, c) in &cleared {
            if board.jelly[r][c] > 0 {
                board.jelly[r][c] -= 1;
                jelly_cleared += 1;
            }
            if let Some(color) = board_before.tiles[r][c].color() {
                color_cleared[color.index()] += 1;
            }
        }
        for &(pos, tile) in &spawns {
            board.tiles[pos.0][pos.1] = tile;
        }
        let (falls, collected) = compact_and_refill(board, &cleared);
        ingredients_collected += collected;

        waves.push(Wave {
            board_before,
            cleared: cleared.into_iter().collect(),
            spawned: spawns.iter().map(|(p, _)| *p).collect(),
            falls,
            board_after: board.clone(),
        });

        let (next_cleared, next_spawns) = find_matches_with_spawns(&board.tiles, &[]);
        cleared = next_cleared;
        spawns = next_spawns;
        if cleared.is_empty() {
            break;
        }
    }

    Resolution {
        mv,
        combo,
        waves,
        score_gained,
        jelly_cleared,
        ingredients_collected,
        color_cleared,
    }
}

// ── Board generation ────────────────────────────────────────────────────────────────

fn gen_plain_tiles(active_colors: &[Color], rng: &mut Rng) -> Tiles {
    let mut tiles = [[Tile::Plain(Color::Red); W]; H];
    for r in 0..H {
        for c in 0..W {
            loop {
                let color = Color::random_from(active_colors, rng);
                let left_two = c >= 2
                    && tiles[r][c - 1].color() == Some(color)
                    && tiles[r][c - 2].color() == Some(color);
                let up_two = r >= 2
                    && tiles[r - 1][c].color() == Some(color)
                    && tiles[r - 2][c].color() == Some(color);
                if !left_two && !up_two {
                    tiles[r][c] = Tile::Plain(color);
                    break;
                }
            }
        }
    }
    tiles
}

/// Rolls a fresh `Variant::Mystery` goal set: 1-3 distinct colors (`MYSTERY_MAX_COLORS`,
/// clamped to however many `active_colors` the board actually has, so a level with a
/// narrowed palette — see `LevelParams::color_count` — can never roll a goal color that
/// literally can't ever spawn) each with its own randomly rolled target in the range for
/// that goal count (`MYSTERY_TARGET_RANGES`) — "various targets," not one shared number,
/// per design. Draws from `active_colors` rather than always `Color::ALL` for the same
/// reason: picking a goal color the board never generates would be an unwinnable episode,
/// not just a hard one — matters in practice now that `Variant::Mystery` levels in
/// `LEVELS` can have a narrowed `color_count` (see `Game::new_level`).
fn gen_mystery_goals(active_colors: &[Color], rng: &mut Rng) -> Vec<MysteryGoal> {
    debug_assert!(
        !active_colors.is_empty(),
        "no active colors to pick a goal from"
    );
    let count =
        (rng.gen_range(1, (MYSTERY_MAX_COLORS + 1) as u64) as usize).min(active_colors.len());
    let mut colors = active_colors.to_vec();
    shuffle(&mut colors, rng);
    let (min, max) = MYSTERY_TARGET_RANGES[count - 1];
    colors
        .into_iter()
        .take(count)
        .map(|color| MysteryGoal {
            color,
            target: rng.gen_range(min as u64, max as u64 + 1) as u32,
            collected: 0,
        })
        .collect()
}

fn gen_board(
    variant: Variant,
    jelly_cell_count: usize,
    ingredients_target: u32,
    active_colors: &[Color],
) -> Board {
    // Seeded once here, drawing a single value from the ambient global RNG — every
    // draw after this point (initial fill, jelly/ingredient placement, reshuffle, and
    // later gravity refills across the board's whole lifetime) comes from this
    // board-local stream instead. See `Rng`'s doc comment for why.
    let mut rng = Rng::seeded(macroquad::rand::gen_range(0u64, u64::MAX));
    let mut tiles = gen_plain_tiles(active_colors, &mut rng);
    let mut jelly = [[0u8; W]; H];

    match variant {
        Variant::Jelly => {
            let mut placed = 0;
            while placed < jelly_cell_count {
                let r = rng.gen_range(0, H as u64) as usize;
                let c = rng.gen_range(0, W as u64) as usize;
                if jelly[r][c] == 0 {
                    jelly[r][c] = 1;
                    placed += 1;
                }
            }
        }
        Variant::Ingredients => {
            let mut cols: Vec<usize> = (0..W).collect();
            shuffle(&mut cols, &mut rng);
            for &c in cols.iter().take(ingredients_target as usize) {
                let r = rng.gen_range(0, 2) as usize;
                tiles[r][c] = Tile::Ingredient;
            }
        }
        Variant::Score | Variant::Timed | Variant::Mystery => {}
    }

    let mut board = Board {
        tiles,
        jelly,
        active_colors: active_colors.to_vec(),
        rng,
    };
    reshuffle(&mut board); // guarantees at least one legal move exists, same as any fresh board
    board
}

fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.gen_range(0, (i + 1) as u64) as usize;
        items.swap(i, j);
    }
}

/// Randomizes every `Plain`/`Bonus` tile's color in place (colorless `ColorBomb` and
/// `Ingredient` tiles are left untouched — reshuffling their identity would silently
/// break the ingredient-collection goal) until the board has at least one legal move
/// and no pre-existing match. Called both at board generation and whenever a resolved
/// board turns out to have no legal move left.
fn reshuffle(board: &mut Board) {
    let active_colors = board.active_colors.clone();
    loop {
        for row in board.tiles.iter_mut() {
            for tile in row.iter_mut() {
                match *tile {
                    Tile::Plain(_) => {
                        *tile = Tile::Plain(Color::random_from(&active_colors, &mut board.rng))
                    }
                    Tile::Bonus(_, special) => {
                        *tile =
                            Tile::Bonus(Color::random_from(&active_colors, &mut board.rng), special)
                    }
                    Tile::ColorBomb | Tile::Ingredient => {}
                }
            }
        }
        let (matched, _) = find_matches_with_spawns(&board.tiles, &[]);
        if !matched.is_empty() {
            continue;
        }
        let any_legal = (0..H).any(|r| {
            (0..W).any(|c| {
                (c + 1 < W && is_legal_swap(&board.tiles, (r, c), (r, c + 1)))
                    || (r + 1 < H && is_legal_swap(&board.tiles, (r, c), (r + 1, c)))
            })
        });
        if any_legal {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() {
        macroquad::rand::srand(12345);
    }

    #[test]
    fn fresh_board_has_no_matches_and_a_legal_move() {
        seed();
        for &variant in &[
            Variant::Score,
            Variant::Jelly,
            Variant::Ingredients,
            Variant::Timed,
            Variant::Mystery,
        ] {
            for _ in 0..20 {
                let game = Game::new(variant, 0);
                let (matched, _) = find_matches_with_spawns(&game.board.tiles, &[]);
                assert!(
                    matched.is_empty(),
                    "{variant:?} board started with a live match"
                );
                assert!(
                    !game.legal_moves().is_empty(),
                    "{variant:?} board started deadlocked"
                );
            }
        }
    }

    #[test]
    fn ingredients_board_places_exactly_the_target_count() {
        seed();
        for _ in 0..20 {
            let game = Game::new(Variant::Ingredients, 0);
            let count = game
                .board
                .tiles
                .iter()
                .flatten()
                .filter(|t| matches!(t, Tile::Ingredient))
                .count();
            assert_eq!(count, INGREDIENTS_TARGET as usize);
        }
    }

    /// Plays each variant to completion using `crate::solver`'s real move selection —
    /// the same driver `run_headless` uses — asserting it always terminates (no
    /// deadlock, no panic) and never applies a move `legal_moves` didn't offer.
    #[test]
    fn full_playthrough_terminates_for_every_variant() {
        seed();
        for &variant in &[
            Variant::Score,
            Variant::Jelly,
            Variant::Ingredients,
            Variant::Timed,
            Variant::Mystery,
        ] {
            for episode in 0..10 {
                let mut game = Game::new(variant, episode);
                let mut beam = crate::solver::new_beam_search();
                let mut steps = 0;
                loop {
                    if variant == Variant::Timed {
                        game.tick_time(1.0);
                    }
                    if game.phase != Phase::Playing {
                        break;
                    }
                    let legal = game.legal_moves();
                    let mv = crate::solver::choose_move(&mut beam, &game)
                        .unwrap_or_else(|| panic!("{variant:?} ep{episode} had no legal move"));
                    assert!(legal.contains(&mv));
                    game.apply(mv);
                    steps += 1;
                    assert!(steps < 10_000, "{variant:?} ep{episode} never terminated");
                }
                assert!(matches!(game.phase, Phase::Over(_)));
            }
        }
    }

    /// A background with no 2 same-colored cells ever adjacent in a row or column —
    /// checkerboarded across two colors, period 2, so no run the tests carve into it
    /// picks up an accidental extra background match.
    fn checkerboard() -> Tiles {
        let mut tiles = [[Tile::Plain(Color::Blue); W]; H];
        for (r, row) in tiles.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = Tile::Plain(if (r + c) % 2 == 0 {
                    Color::Blue
                } else {
                    Color::Purple
                });
            }
        }
        tiles
    }

    #[test]
    fn wrapped_and_color_bomb_spawn_from_l_shape_and_five_run() {
        seed();
        // Straight 5-run along a row spawns a ColorBomb at the run's middle.
        let mut tiles = checkerboard();
        for cell in tiles[0].iter_mut().take(5) {
            *cell = Tile::Plain(Color::Red);
        }
        let (cleared, spawns) = find_matches_with_spawns(&tiles, &[]);
        assert_eq!(cleared.len(), 4); // 5 matched minus the 1 spawn cell
        assert_eq!(spawns.len(), 1);
        assert!(matches!(spawns[0].1, Tile::ColorBomb));

        // An L-shape (3 across + 3 down sharing a corner) spawns a Wrapped tile at the
        // shared corner instead.
        let mut tiles = checkerboard();
        for cell in tiles[0].iter_mut().take(3) {
            *cell = Tile::Plain(Color::Green);
        }
        for row in tiles.iter_mut().take(3) {
            row[0] = Tile::Plain(Color::Green);
        }
        let (cleared, spawns) = find_matches_with_spawns(&tiles, &[]);
        assert_eq!(cleared.len(), 4); // 5 unique matched cells minus the 1 spawn cell
        assert_eq!(spawns.len(), 1);
        assert!(matches!(spawns[0].1, Tile::Bonus(_, Special::Wrapped)));
        assert_eq!(spawns[0].0, (0, 0));
    }

    #[test]
    fn row_and_col_clear_spawn_aligned_with_match_direction() {
        seed();
        let mut tiles = checkerboard();
        for cell in tiles[0].iter_mut().take(4) {
            *cell = Tile::Plain(Color::Red);
        }
        let (_, spawns) = find_matches_with_spawns(&tiles, &[]);
        assert_eq!(spawns.len(), 1);
        assert!(
            matches!(spawns[0].1, Tile::Bonus(_, Special::RowClear)),
            "a horizontal match-4 should spawn a tile that clears its row"
        );

        let mut tiles = checkerboard();
        for row in tiles.iter_mut().take(4) {
            row[0] = Tile::Plain(Color::Red);
        }
        let (_, spawns) = find_matches_with_spawns(&tiles, &[]);
        assert_eq!(spawns.len(), 1);
        assert!(
            matches!(spawns[0].1, Tile::Bonus(_, Special::ColClear)),
            "a vertical match-4 should spawn a tile that clears its column"
        );
    }

    #[test]
    fn color_bomb_is_consumed_by_its_own_solo_trigger() {
        seed();
        let mut board = Board {
            tiles: checkerboard(),
            jelly: [[0; W]; H],
            active_colors: Color::ALL.to_vec(),
            rng: Rng::seeded(42),
        };
        board.tiles[0][0] = Tile::ColorBomb;
        board.tiles[0][1] = Tile::Plain(Color::Red);
        let res = resolve(
            &mut board,
            Move {
                a: (0, 0),
                b: (0, 1),
            },
        );
        // Check the first wave's own `cleared` set directly rather than the final
        // (post-cascade) board — a random refill can coincidentally spawn an unrelated
        // *new* ColorBomb elsewhere, which would make a whole-board "no ColorBomb
        // anywhere" check flaky.
        assert!(
            res.waves[0].cleared.contains(&(0, 1)),
            "the ColorBomb should clear its own (post-swap) cell, not just the color it targeted: {:?}",
            res.waves[0].cleared
        );
        assert!(res.score_gained > 0);
    }

    #[test]
    fn solo_trigger_effect_anchors_on_the_specials_post_swap_position() {
        seed();
        // A RowClear tile swapped from (0,0) into (1,0) should clear row 1 (where it
        // lands), not row 0 (where it started).
        let mut board = Board {
            tiles: checkerboard(),
            jelly: [[0; W]; H],
            active_colors: Color::ALL.to_vec(),
            rng: Rng::seeded(42),
        };
        board.tiles[0][0] = Tile::Bonus(Color::Red, Special::RowClear);
        board.tiles[1][0] = Tile::Plain(Color::Blue);
        resolve(
            &mut board,
            Move {
                a: (0, 0),
                b: (1, 0),
            },
        );
        assert!(
            board.tiles[1]
                .iter()
                .all(|t| !matches!(t, Tile::Bonus(_, Special::RowClear))),
            "row 1 (the landing row) should have been cleared and refilled"
        );
    }

    /// The "headless level-linter" the todo backlog asked for: seed-sweeps every
    /// `LEVELS` entry via the real solver and flags any level whose win rate falls
    /// below a floor — a hand-authored level under this floor is a balance bug (the
    /// bot has no lookahead to eventually git gud), not "hard mode." Ignored by default
    /// since 12 levels x many seeds is slow in a debug build; run explicitly after
    /// touching `LEVELS` or the solver's weights:
    /// `cargo test --release -p match-3 level_line_win_rates -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn level_line_win_rates_stay_above_floor() {
        const SEEDS: u64 = 60;
        const FLOOR: f64 = 0.25;
        for (i, level) in LEVELS.iter().enumerate() {
            let mut wins = 0u64;
            for seed in 1..=SEEDS {
                macroquad::rand::srand(seed);
                let mut game = Game::new_level(*level, 0);
                let mut beam = crate::solver::new_beam_search();
                loop {
                    if level.variant == Variant::Timed {
                        game.tick_time(1.0);
                    }
                    if game.phase != Phase::Playing {
                        break;
                    }
                    let mv = crate::solver::choose_move(&mut beam, &game).expect("legal move");
                    game.apply(mv);
                }
                if matches!(game.phase, Phase::Over(Outcome::Won)) {
                    wins += 1;
                }
            }
            let rate = wins as f64 / SEEDS as f64;
            eprintln!(
                "level {} ({}): win rate {:.0}%",
                i + 1,
                level.name,
                rate * 100.0
            );
            assert!(
                rate >= FLOOR,
                "level {} ({}) win rate {:.0}% is below the {:.0}% floor",
                i + 1,
                level.name,
                rate * 100.0,
                FLOOR * 100.0
            );
        }
    }

    #[test]
    fn color_bomb_combo_consumes_itself() {
        seed();
        let mut board = Board {
            tiles: checkerboard(),
            jelly: [[0; W]; H],
            active_colors: Color::ALL.to_vec(),
            rng: Rng::seeded(42),
        };
        board.tiles[0][0] = Tile::ColorBomb;
        board.tiles[0][1] = Tile::Bonus(Color::Red, Special::Wrapped);
        let res = resolve(
            &mut board,
            Move {
                a: (0, 0),
                b: (0, 1),
            },
        );
        // Same reasoning as `color_bomb_is_consumed_by_its_own_solo_trigger` — check
        // the first wave directly rather than the final (post-cascade) board.
        assert!(
            res.waves[0].cleared.contains(&(0, 1)),
            "a ColorBomb+Wrapped combo should clear the ColorBomb tile itself, not just the target color: {:?}",
            res.waves[0].cleared
        );
    }
}
