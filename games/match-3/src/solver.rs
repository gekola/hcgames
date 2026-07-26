use crate::game::{Board, Game, H, Move, Resolution, Tile, Variant, W};

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

/// Enumerates every legal move, resolves each on a scratch clone of the board, and picks
/// the highest-scoring one — a plain greedy 1-ply eval rather than `beam_solver`'s
/// depth-2 lookahead (unlike Tetris's single known-next-piece, there's no equivalent
/// "next" to look ahead into here: the tile a swap reveals depends on where gravity
/// happens to refill from, and an 8x8 board already offers on the order of 100 legal
/// swaps per move, so a second ply would multiply that branching factor rather than
/// sharpen a small candidate set). `None` only if `legal_moves()` is empty, which
/// `Game::apply`'s reshuffle-on-deadlock keeps from happening during normal play.
pub fn choose_move(game: &Game) -> Option<Move> {
    game.legal_moves()
        .into_iter()
        .map(|mv| (mv, score_resolution(game, &game.simulate(mv))))
        .max_by_key(|&(_, score)| score)
        .map(|(mv, _)| mv)
}
