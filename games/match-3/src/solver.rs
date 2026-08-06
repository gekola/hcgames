use crate::game::{Board, Game, H, Move, Phase, Resolution, Rng, Tile, Variant, W, preview_seed};

/// Extra weight per bonus tile consumed (colored bonus or `ColorBomb`) — encourages the
/// bot to actually chain into/detonate specials rather than only chasing raw `score_gained`,
/// which already rewards big clears but doesn't distinguish "used a bonus" from "got a
/// lucky big cascade".
const BONUS_TRIGGER_BONUS: i64 = 400;
const COMBO_BONUS: i64 = 1200;
/// Per-jelly-layer and per-ingredient weights are large relative to raw score points so
/// the bot prioritizes its actual win condition over score-maximizing even when a
/// higher-`score_gained` alternative move exists — a `Jelly`-variant board with plenty of
/// moves left should still race to clear jelly, not detour into big score-only combos.
const JELLY_WEIGHT: i64 = 300;
const INGREDIENT_WEIGHT: i64 = 500;
/// Per-row credit for merely moving an `Ingredient` tile closer to the bottom row —
/// without this, `ingredients_collected` (which only pays off the move that happens to
/// land one on row `H-1`) gives the greedy 1-ply solver no reason to ever prefer a swap
/// that clears cells *under* an ingredient over an equally-scoring swap elsewhere on the
/// board, so ingredients only drift down by accident. This is what actually steers the
/// bot toward the `Ingredients` goal.
const INGREDIENT_PROGRESS_WEIGHT: i64 = 180;

/// Below this many remaining jelly cells, clearing one outright dominates everything
/// else in the eval — see `score_resolution`. A *gentle* multiplier on `JELLY_WEIGHT`
/// here was tried and measurably didn't help (see CLAUDE.md's "Goal-aware solver tuning
/// attempted, didn't measurably help"): seed-sweep instrumentation of real losses showed
/// the bot was routinely passing up an available jelly-clearing move for a bigger
/// `COMBO_BONUS`/cascade elsewhere, because 2x `JELLY_WEIGHT` (600) still loses to
/// `COMBO_BONUS` (1200). A bonus large enough to *dominate* that comparison, not just
/// lean toward it, actually recovers those losses — validated with a held-out seed range
/// disjoint from the one used to pick this threshold/magnitude, to rule out overfitting.
const JELLY_ENDGAME_REMAINING: u32 = 4;
const JELLY_ENDGAME_BONUS: i64 = 5000;

/// Root-only (ply-0) extra emphasis on *immediately* clearing jelly in the endgame,
/// applied in `score_root` but **not** `score_step`. `JELLY_ENDGAME_BONUS` alone isn't
/// enough under the depth-2 beam: the beam scores whole lines by cumulative sum, and that
/// bonus is path-additive, so a plan that defers the winning jelly-clear to ply 2 earns
/// the *same* endgame bonus at ply 2 while also banking ply-0's incidental `score_gained`
/// on top — out-scoring "clear the last jelly now" by that incidental margin. But ply 2
/// rides a `preview_seed`-decorrelated phantom refill (see the RNG note in CLAUDE.md) that
/// won't materialize in real play, so the deferred win evaporates and the episode is lost
/// with 1-2 jelly left (L11 "Deep Jelly" near-miss losses — 33 of 38 losing episodes on a
/// seed sweep had the bot pass up an available immediate jelly-clear whose alternative
/// *also* scored higher, which a 1-ply eval structurally cannot do; only the depth-2
/// deferral can). Only the *root* move is ever actually applied, so crediting real
/// immediate jelly progress above a hypothetical later clear is correct, not a hack.
const JELLY_ENDGAME_ROOT_BONUS: i64 = 4000;

/// A `Mystery` move is scored by how many cells of `game.mystery_color` it clears.
/// `Jelly`'s dominance-threshold trick (`JELLY_ENDGAME_*`) was tried here too on the
/// theory it'd transfer, but an ablation (same weight, bonus zeroed) produced *identical*
/// win/loss outcomes across 450 seeds — unlike `Jelly`, an ordinary 3-match of the target
/// color already dumps 3 cells at `MYSTERY_WEIGHT` each in one shot, so the plain weight
/// already wins the moves that matter; there's no near-miss-losing-to-COMBO_BONUS pattern
/// to fix. Dropped rather than shipped as inert complexity.
const MYSTERY_WEIGHT: i64 = 300;

/// Extra weight per `Tile::Licorice` cell a move clears (an adjacent ordinary match or a
/// bonus effect's area — see that tile's doc comment), applied unconditionally regardless
/// of `Variant` — even outside `Variant::Licorice`, a Licorice cell sitting on some other
/// variant's board (`LevelParams::licorice_cell_count` is independent of `variant`)
/// reduces future cascade throughput near it, so without this the eval would have no
/// reason to ever prefer punching through a blocker over an equal-`score_gained` move
/// elsewhere. Deliberately smaller than `JELLY_WEIGHT`/`INGREDIENT_WEIGHT`/`MYSTERY_WEIGHT`
/// — those bias toward an actual win condition; this alone only biases toward keeping the
/// board open. See `LICORICE_GOAL_WEIGHT` for the additional weight that applies when
/// clearing Licorice *is* the win condition.
const LICORICE_WEIGHT: i64 = 200;

/// Extra weight per `Tile::Licorice` cell cleared, added on top of `LICORICE_WEIGHT`
/// only when `game.variant == Variant::Licorice` — mirrors `INGREDIENT_WEIGHT`'s
/// relationship to `Variant::Ingredients`: the baseline weight alone treats Licorice as
/// a nice-to-have, but when clearing it *is* the win condition, the eval needs to
/// actually chase it the way `JELLY_WEIGHT`/`INGREDIENT_WEIGHT` chase their own goals.
const LICORICE_GOAL_WEIGHT: i64 = 350;

/// Experimental: reward a move for leaving the cell directly *below* a still-uncollected
/// `Ingredient` primed (already has a same-color neighbor, one swap away from a match) —
/// clearing that cell is what actually pulls the ingredient down a row (verified against
/// `compact_and_refill`'s gravity: clearing below shifts the ingredient down, clearing
/// above does not). Speculative, unlike the shipped `JELLY_ENDGAME_*` fix: refill is
/// random, so priming a cell now doesn't guarantee anything lands there to complete the
/// match next turn. Only ship if it measurably beats baseline on multiple disjoint seed
/// ranges — `Ingredients`' baseline itself swings several points range to range (see
/// CLAUDE.md), so treat anything under ~5 points as noise, not signal.
const INGREDIENT_SETUP_BONUS: i64 = 60;

fn same_color(a: Tile, b: Tile) -> bool {
    matches!(
        (a, b),
        (Tile::Plain(x) | Tile::Bonus(x, _), Tile::Plain(y) | Tile::Bonus(y, _)) if x == y
    )
}

fn ingredient_setup_score(board: &Board) -> i64 {
    let mut primed = 0i64;
    for r in 0..H.saturating_sub(1) {
        for c in 0..W {
            if !matches!(board.tiles[r][c], Tile::Ingredient) {
                continue;
            }
            let below = board.tiles[r + 1][c];
            let has_neighbor = (r + 2 < H && same_color(board.tiles[r + 2][c], below))
                || (c > 0 && same_color(board.tiles[r + 1][c - 1], below))
                || (c + 1 < W && same_color(board.tiles[r + 1][c + 1], below));
            if has_neighbor {
                primed += 1;
            }
        }
    }
    primed
}

fn score_resolution(game: &Game, res: &Resolution) -> i64 {
    let mut s = res.score_gained as i64;
    s += res.jelly_cleared as i64 * JELLY_WEIGHT;
    if game.variant == Variant::Jelly && game.jelly_remaining <= JELLY_ENDGAME_REMAINING {
        s += res.jelly_cleared as i64 * JELLY_ENDGAME_BONUS;
    }
    s += res.ingredients_collected as i64 * INGREDIENT_WEIGHT;
    s += res.licorice_cleared as i64 * LICORICE_WEIGHT;
    if game.variant == Variant::Licorice {
        s += res.licorice_cleared as i64 * LICORICE_GOAL_WEIGHT;
    }
    if game.variant == Variant::Mystery {
        // Only reward progress on goals not yet met — otherwise clears of an
        // already-completed color would keep scoring, diluting the incentive to focus on
        // whichever goal(s) are still open when there's more than one.
        for goal in game.mystery_goals.iter().filter(|g| g.collected < g.target) {
            s += res.color_cleared[goal.color.index()] as i64 * MYSTERY_WEIGHT;
        }
    }
    if res.combo {
        s += COMBO_BONUS;
    }
    let bonus_tiles_touched = res.waves.iter().flat_map(|w| w.spawned.iter()).count() as i64;
    s += bonus_tiles_touched * BONUS_TRIGGER_BONUS / 2; // spawning a bonus is good but not as good as consuming one
    let ingredient_rows_fallen: i64 = res
        .waves
        .iter()
        .flat_map(|w| w.falls.iter())
        .filter(|f| matches!(f.tile, Tile::Ingredient))
        .map(|f| (f.to_row as i32 - f.from_row) as i64)
        .sum();
    s += ingredient_rows_fallen * INGREDIENT_PROGRESS_WEIGHT;

    if game.variant == Variant::Ingredients
        && let Some(last) = res.waves.last()
    {
        s += ingredient_setup_score(&last.board_after) * INGREDIENT_SETUP_BONUS;
    }

    s
}

// ── `beam_solver`-backed move selection (the default and only solver). Root CLAUDE.md's
// "Self-playing solver games" section notes match-3 doesn't obviously fit `beam_solver`'s
// shape — no known-next-tile to search a second real ply into, since the tile a swap
// reveals depends on where gravity refills from — but a direct measurement disproved that
// theoretical objection: depth-2 beam beat the old greedy 1-ply eval by 6-14pp win rate
// across every variant at ~1.5ms/move, so it was promoted to the default and greedy/hybrid
// were deleted. See `.notes/match3_solver_beam_todo.md` and this crate's `CLAUDE.md` for
// the full numbers.

impl beam_solver::SearchState for Game {
    type Move = Move;

    fn legal_moves(&self) -> Vec<Move> {
        // Resolves to the inherent `Game::legal_moves` (inherent methods always win
        // over trait methods in call-site resolution, even from inside this same impl),
        // not infinite recursion into this trait method.
        self.legal_moves()
    }

    fn apply(&mut self, m: Move) {
        // `beam_solver`'s engine only ever calls this on a scratch clone (never the
        // real session's `Game` — see its own `choose_move`, which clones before every
        // `apply`), so it's exploratory the same way `Game::simulate` is, and needs the
        // exact same fix: reseed from `preview_seed` before resolving for real, rather
        // than letting the clone continue the real board's inherited rng stream. Without
        // this, the beam's own line-advancing `apply` calls (used to generate each
        // further ply's candidates, not just to score one) would leak an oracle preview
        // of the actual future for whichever first move the search settles on — the
        // same bug `Game::simulate` had before `preview_seed` existed, just reached via
        // a different call path (this trait's `apply`, not `Game::simulate`). Also
        // discards the `Resolution` `Game::apply` returns — the beam engine only wants
        // the mutated state, not a persistent animation-ready world.
        self.board.rng = Rng::seeded(preview_seed(&self.board, m));
        self.apply(m);
    }

    fn state_hash(&self) -> u64 {
        state_hash(self)
    }

    fn is_terminal(&self) -> bool {
        self.phase != Phase::Playing
    }
}

fn state_hash(game: &Game) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    game.board.tiles.hash(&mut h);
    game.board.jelly.hash(&mut h);
    game.board.active_colors.hash(&mut h);
    game.score.hash(&mut h);
    game.moves_used.hash(&mut h);
    game.jelly_remaining.hash(&mut h);
    game.ingredients_collected.hash(&mut h);
    for goal in &game.mystery_goals {
        goal.color.hash(&mut h);
        goal.target.hash(&mut h);
        goal.collected.hash(&mut h);
    }
    h.finish()
}

/// Beam width/depth/node-budget. Measured against the old greedy 1-ply eval on
/// `HCG_SEED=1..500` (see the note above `SearchState`'s impl); re-tune here rather than
/// assuming these transfer from Klondike/Spider (which have a very different branching
/// profile — see `lib/beam_solver`'s CLAUDE.md).
const BEAM_WIDTH: usize = 6;
const BEAM_DEPTH: u32 = 2;
const BEAM_NODE_BUDGET: usize = 800;

pub type Beam = beam_solver::BeamSearch<Game>;

pub fn new_beam_search() -> Beam {
    beam_solver::BeamSearch::new(BEAM_WIDTH, BEAM_DEPTH, BEAM_NODE_BUDGET, |_| false)
}

/// Scores a single move by resolving it against `before` and running it through the same
/// tuned `score_resolution` used for both root and per-step beam scoring — the eval that
/// defines what "good" means move-to-move.
fn beam_score(before: &Game, mv: &Move) -> i32 {
    let res = before.simulate(*mv);
    score_resolution(before, &res) as i32
}

/// Root-ply scorer: `beam_score` plus the root-only jelly-endgame emphasis
/// (`JELLY_ENDGAME_ROOT_BONUS`) that makes an *immediate* winning jelly-clear dominate a
/// depth-2 plan that defers it onto a phantom refill. Used for the real, immediately-legal
/// first move only; `beam_score` (no root bonus) still scores every later ply.
fn beam_score_root(before: &Game, mv: &Move) -> i32 {
    let res = before.simulate(*mv);
    let mut s = score_resolution(before, &res);
    if before.variant == Variant::Jelly && before.jelly_remaining <= JELLY_ENDGAME_REMAINING {
        s += res.jelly_cleared as i64 * JELLY_ENDGAME_ROOT_BONUS;
    }
    s as i32
}

/// The sole move-selection entry point: a depth-2 `beam_solver` search scored by
/// `score_resolution`. `None` only if `legal_moves()` is empty, which `Game::apply`'s
/// reshuffle-on-deadlock keeps from happening during normal play.
pub fn choose_move(search: &mut Beam, game: &Game) -> Option<Move> {
    search.choose_move(
        game,
        |_, _| false,
        |before, _after, mv| beam_score_root(before, mv),
        |before, _after, mv| beam_score(before, mv),
    )
}
