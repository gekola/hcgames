use crate::game::{
    Board, Color, DEATH_ROW, DeterministicHasher, Game, Move, Outcome, Phase, Resolution, Rng,
    max_row, preview_seed,
};

// ── Scoring weights ──────────────────────────────────────────────────────────────
//
// Initial, reasoned-but-*not*-yet-measured weights — root CLAUDE.md's "Self-playing
// solver games" section is explicit that win rate must be checked empirically via a
// `--no-ui --once` `HCG_SEED` sweep, not assumed from the weights alone (see
// match-3/klondike/spider's own solver-tuning history for what that process looks
// like in practice). A smoke-tested-only starting point; see this crate's CLAUDE.md
// for the "not yet tuned" follow-up note.
const POP_WEIGHT: i64 = 15;
/// Floaters (a cascade the shot merely *set up*, not a direct match) are worth more
/// than a same-size direct pop — they're the higher-skill outcome and are what
/// actually keeps the board's height down fastest.
const FLOATER_WEIGHT: i64 = 40;
/// Per-row penalty on the post-move board's deepest bubble — height is the actual
/// loss condition (`DEATH_ROW`), so this should dominate once a shot noticeably worsens it.
const HEIGHT_PENALTY: i64 = 25;
/// Added on top of `HEIGHT_PENALTY`, divided by however many rows of margin are left
/// before `DEATH_ROW` — negligible early (margin 8 adds ~38) but dominant right at the
/// edge (margin 1 adds 300), because the *marginal* cost of one more row is not
/// constant: losing a row of safety when there's plenty left barely matters, losing the
/// last one ends the episode. A flat per-row penalty alone let a merely-decent pop
/// outscore a move that pushed the board within one row of `DEATH_ROW`.
const HEIGHT_MARGIN_PENALTY: i64 = 300;
/// A move whose resulting state is `Outcome::Lost` used to be scored by ordinary
/// height/pop math like any other move — nothing in `score_resolution` ever looked at
/// `after.phase`, so a losing move could still come out ahead if it happened to pop or
/// floater a lot on the way down. This is what actually enforces "never choose a losing
/// move over a surviving one, regardless of how much it pops" — large enough that no
/// realistic pop/floater haul on a single shot can outweigh it.
const LOSE_PENALTY: i64 = 100_000;
const NO_POP_PENALTY: i64 = 20;
/// Extra penalty when a no-pop shot's own newly-placed bubble has zero same-color
/// neighbors — a color stranded with no near-term way to complete a match.
const ISOLATED_SINGLETON_PENALTY: i64 = 120;
/// Per same-color adjacent pair left on the board — a proxy for "shots away from
/// popping," rewarding board states with live near-matches over ones without.
const ADJACENT_PAIR_BONUS: i64 = 10;
/// Per color left with exactly one bubble on the board — clutters the board without
/// being reachable as a real 3-match target most turns.
const ORPHAN_COLOR_PENALTY: i64 = 60;

fn is_isolated(board: &Board, pos: (i32, i32), color: Color) -> bool {
    const NEIGHBOR_OFFSETS: [(i32, i32); 6] = [(2, 0), (-2, 0), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    !NEIGHBOR_OFFSETS
        .iter()
        .any(|&(dc, dr)| board.cells.get(&(pos.0 + dc, pos.1 + dr)) == Some(&color))
}

fn adjacent_pairs(board: &Board) -> u32 {
    // Only the three "positive-dc" offsets, so each pair is counted from exactly one
    // side of it rather than twice (once per bubble).
    const HALF_OFFSETS: [(i32, i32); 3] = [(2, 0), (1, 1), (1, -1)];
    let mut count = 0;
    for (&(col, row), &color) in &board.cells {
        for &(dc, dr) in &HALF_OFFSETS {
            if board.cells.get(&(col + dc, row + dr)) == Some(&color) {
                count += 1;
            }
        }
    }
    count
}

fn orphan_colors(board: &Board) -> u32 {
    let mut counts = std::collections::HashMap::new();
    for &color in board.cells.values() {
        *counts.entry(color).or_insert(0u32) += 1;
    }
    counts.values().filter(|&&c| c == 1).count() as u32
}

fn score_resolution(res: &Resolution, after: &Game) -> i64 {
    let board_after = &after.board;
    let mut s = 0i64;
    s += res.popped.len() as i64 * POP_WEIGHT;
    s += res.floaters.len() as i64 * FLOATER_WEIGHT;
    if res.popped.is_empty() {
        s -= NO_POP_PENALTY;
        if is_isolated(board_after, res.mv.target, res.color) {
            s -= ISOLATED_SINGLETON_PENALTY;
        }
    }
    let row = max_row(board_after);
    s -= row as i64 * HEIGHT_PENALTY;
    let margin = (DEATH_ROW - row).max(1);
    s -= HEIGHT_MARGIN_PENALTY / margin as i64;
    s += adjacent_pairs(board_after) as i64 * ADJACENT_PAIR_BONUS;
    s -= orphan_colors(board_after) as i64 * ORPHAN_COLOR_PENALTY;
    if after.phase == Phase::Over(Outcome::Lost) {
        s -= LOSE_PENALTY;
    }
    s
}

// ── `beam_solver` integration ────────────────────────────────────────────────────
//
// Same hybrid shape root CLAUDE.md's "Self-playing solver games" section describes for
// this game: many simultaneously-legal target cells to score against each other (the
// match-3/beam_solver shape) plus a real animated flight per shot (the arrow-blocks/
// game2048 virtual-dt shape) — `game.rs`/`main.rs` split the two apart, this file only
// deals with move *selection*.

impl beam_solver::SearchState for Game {
    type Move = Move;

    fn legal_moves(&self) -> Vec<Move> {
        // Resolves to the inherent `Game::legal_moves`, not infinite recursion into
        // this trait method — inherent methods always win at the call site, even from
        // inside this same impl (see match-3's `solver.rs` for the same note).
        self.legal_moves()
    }

    fn apply(&mut self, m: Move) {
        // Only ever called by `beam_solver` on a scratch clone (see its own
        // `choose_move`), so this needs the same RNG-decorrelation fix
        // `Game::simulate` has — reseed from `preview_seed` before resolving, rather
        // than letting the clone continue this game's real future stream. See `Rng`'s
        // doc comment (game.rs).
        self.rng = Rng::seeded(preview_seed(&self.board, m));
        self.apply(m); // inherent Game::apply, not recursion — see `legal_moves` above.
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
    let mut entries: Vec<((i32, i32), Color)> =
        game.board.cells.iter().map(|(&k, &v)| (k, v)).collect();
    entries.sort_unstable_by_key(|&(k, _)| k);
    let mut h = DeterministicHasher(0x2545_F491_4F6C_DD1D);
    entries.hash(&mut h);
    game.current_color.hash(&mut h);
    game.next_color.hash(&mut h);
    game.score.hash(&mut h);
    game.shots_used.hash(&mut h);
    game.shots_until_descend.hash(&mut h);
    h.finish()
}

/// `legal_moves()` here costs ~165 raymarches (one per sampled angle), noticeably more
/// per call than match-3's O(1)-per-candidate swap check — but at depth 2, only
/// `1 + BEAM_WIDTH` nodes ever call it per real move (one root call, then one per
/// surviving ply-1 line), so the actual per-move cost stays small. Re-measure
/// (`--no-ui --once --debug`, `--release`) before raising either width or depth — see
/// this crate's CLAUDE.md.
const BEAM_WIDTH: usize = 6;
const BEAM_DEPTH: u32 = 2;
const BEAM_NODE_BUDGET: usize = 600;

pub type Beam = beam_solver::BeamSearch<Game>;

pub fn new_beam_search() -> Beam {
    beam_solver::BeamSearch::new(BEAM_WIDTH, BEAM_DEPTH, BEAM_NODE_BUDGET, |_| false)
}

fn beam_score(before: &Game, mv: &Move) -> i32 {
    let (after, res) = before.simulate(*mv);
    score_resolution(&res, &after) as i32
}

/// No root-only bonus term (yet) — unlike match-3's jelly-endgame case, there's no
/// measured near-miss pattern here to justify one; see this crate's CLAUDE.md. Root and
/// step plies share the same scorer for now.
pub fn choose_move(search: &mut Beam, game: &Game) -> Option<Move> {
    search.choose_move(
        game,
        |_, _| false,
        |before, _after, mv| beam_score(before, mv),
        |before, _after, mv| beam_score(before, mv),
    )
}
