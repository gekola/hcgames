use crate::game::{Game, Move, Phase, column_heights, count_holes};
use beam_solver::{BeamSearch, SearchState};

/// See [[beam_solver]] docs for the shared search engine. Depth 2 means "score this
/// placement, then score the best-looking placement of the piece we already know comes
/// next" — a real known lookahead (this game always keeps `Game::queue` populated), not a
/// hypothetical. Width comfortably covers the ~34 placements a 10-wide board ever offers
/// a single piece, so nothing meaningful gets truncated at ply 0.
const BEAM_WIDTH: usize = 12;
const BEAM_DEPTH: u32 = 2;
/// `width * (typical placement count)` is already only a few hundred nodes for depth 2,
/// nowhere near pathological — this is a generous ceiling, not a tuned one.
const NODE_BUDGET: usize = 8_000;

impl SearchState for Game {
    type Move = Move;

    fn legal_moves(&self) -> Vec<Move> {
        Game::legal_moves(self)
    }

    fn apply(&mut self, m: Move) {
        Game::apply(self, m)
    }

    fn state_hash(&self) -> u64 {
        Game::state_hash(self)
    }

    fn is_terminal(&self) -> bool {
        self.phase != Phase::Playing
    }
}

pub struct Solver {
    beam: BeamSearch<Game>,
}

impl Solver {
    pub fn new() -> Self {
        Self {
            beam: BeamSearch::new(BEAM_WIDTH, BEAM_DEPTH, NODE_BUDGET, |_| false),
        }
    }

    pub fn choose_move(&mut self, game: &Game) -> Option<Move> {
        self.beam.choose_move(game, |_, _| false, score, score)
    }
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

// Weight set popularized by Yiyuan Lee's genetic-algorithm-tuned one-piece Tetris
// heuristic (a commonly reused reference point across open-source Tetris bots): favor
// clearing lines, penalize tall/uneven stacks and buried holes. Evaluates the *resulting*
// board absolutely (not a delta from before), which is why `Game::apply` records
// `last_lines_cleared` separately — the board itself doesn't show how a line vanished,
// only that it's gone.
const W_AGGREGATE_HEIGHT: f64 = -0.510066;
const W_LINES_CLEARED: f64 = 0.760666;
const W_HOLES: f64 = -0.35663;
const W_BUMPINESS: f64 = -0.184483;

fn score(_before: &Game, after: &Game, _m: &Move) -> i32 {
    let heights = column_heights(&after.board);
    let aggregate_height: i32 = heights.iter().sum();
    let holes = count_holes(&after.board, &heights);
    let bumpiness: i32 = heights.windows(2).map(|w| (w[0] - w[1]).abs()).sum();

    let raw = W_LINES_CLEARED * after.last_lines_cleared as f64
        + W_AGGREGATE_HEIGHT * aggregate_height as f64
        + W_HOLES * holes as f64
        + W_BUMPINESS * bumpiness as f64;
    // beam_solver compares i32 scores; scale up before rounding so the small float
    // weights above don't collapse to a handful of indistinguishable integer buckets.
    (raw * 1000.0).round() as i32
}
