use crate::game::{Game, Move, Phase};
use beam_solver::{BeamSearch, SearchState};

/// How many of the best candidate lines to keep alive at each ply of the search — a
/// small beam search (Bjarnason/Cazenave-adjacent: cheaper than a full rollout per
/// candidate, since it shares work across candidates by pruning to the best few after
/// every ply instead of running each one out to full depth independently).
const BEAM_WIDTH: usize = 8;
/// How many plies deep to search. Ply 0 is the real decision (one node per legal first
/// move); the remaining plies project forward with the cheap `score_core` step policy
/// to judge which of those first moves actually leads somewhere, rather than picking
/// whichever looks best after a single step.
const BEAM_DEPTH: u32 = 5;

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
            beam: BeamSearch::new(BEAM_WIDTH, BEAM_DEPTH, |m| matches!(m, Move::Deal)),
        }
    }

    pub fn choose_move(&mut self, game: &Game) -> Option<Move> {
        self.beam.choose_move(game, is_pointless, score, score_core)
    }
}

/// Whether `m` is unambiguous, real progress — uncovers a face-down card, completes a
/// run, or unlocks a deal by filling the last empty column — as opposed to a merely
/// legal shuffle that satisfies `is_pointless`'s very loose bar without actually
/// achieving anything. Used to decide whether Deal has a genuine alternative worth
/// preferring, not just *a* legal move.
fn is_real_progress(game: &Game, m: &Move) -> bool {
    let Move::TableauToTableau { from, n, to } = *m else {
        return false; // Deal itself never counts as an alternative to Deal
    };

    if game.n_down[from] > 0 && n == game.n_up(from) {
        return true; // uncovers
    }

    let mut after = game.clone();
    after.apply(*m);
    if after.completed > game.completed {
        return true; // completes a run
    }

    game.tableau[to].is_empty()
        && !game.stock.is_empty()
        && after.tableau.iter().all(|t| !t.is_empty())
}

/// The only move that's truly pointless — filtered out entirely rather than merely
/// down-scored — is relocating an entire already-fully-exposed pile onto another empty
/// column: it changes nothing but the column index. Everything else legal_moves() already
/// guarantees is rank-compatible with its destination (that's required to generate the
/// move at all), including landing on a different-suit card: that's ordinary spider play
/// — it can't move as a group later or bank, but it still consolidates a pile and reduces
/// clutter — so `score_core` prices it lower than a same-suit build rather than this
/// function rejecting it outright. Rejecting it outright was the bug: it left "shuffle a
/// parked card to another empty column" as the only move left in exactly the position
/// where "stack it on a rank-compatible card of a different suit" was the obviously
/// better play.
fn is_pointless(game: &Game, m: &Move) -> bool {
    let Move::TableauToTableau { from, n, to } = *m else {
        return false; // Deal is never pointless
    };

    game.tableau[to].is_empty() && n == game.n_up(from) && game.n_down[from] == 0
}

/// Full evaluation used for the top-level move choice: `score_core` plus a mobility
/// term (how many productive moves remain after this one). The mobility lookahead calls
/// `legal_moves` again, so it's deliberately excluded from `score_core` — that function
/// also drives each step of `rollout_bonus`'s multi-ply playout, where paying for mobility
/// at every simulated ply would multiply cost by the rollout depth for little benefit.
fn score(game: &Game, after: &Game, m: &Move) -> i32 {
    let mut s = score_core(game, after, m);

    if matches!(m, Move::TableauToTableau { .. }) {
        let follow_ups = after
            .legal_moves()
            .iter()
            .filter(|mv| !is_pointless(after, mv))
            .count() as i32;
        s += follow_ups;
    }

    s
}

fn score_core(game: &Game, after: &Game, m: &Move) -> i32 {
    match *m {
        // Only 5 total deals ever happen in a game — once the stock is empty, that's
        // it. A flat score here previously either lost to ordinary tableau shuffling
        // (leaving a filled-last-empty-column position stuck unable to deal) or, when
        // raised to fix that, *beat* ordinary tableau moves outright (they only score
        // ~7-15) and burned through several deals back-to-back before the board got a
        // chance to reorganize the fresh cards in between. Checking against *any*
        // legal non-pointless move (an earlier attempt at this) was still wrong the
        // other way: `is_pointless` barely filters anything now, so nearly every
        // position has *some* legal shuffle, and Deal lost to it even when the board
        // was genuinely locked and fresh cards were the only real hope — confirmed by
        // a real trace where 3 deals sat unused while the solver cycled a locked
        // position for 100+ moves. Deal only needs to win against *real* progress
        // (an uncover, a completion, or unlocking a future deal) — not against noise.
        Move::Deal => {
            let has_real_alt = game
                .legal_moves()
                .into_iter()
                .any(|mv| is_real_progress(game, &mv));
            if has_real_alt { 5 } else { 45 }
        }

        Move::TableauToTableau { from, n, to } => {
            if after.completed > game.completed {
                return 1000; // banking a run always wins over anything else
            }

            let uncovers = game.n_down[from] > 0 && n == game.n_up(from);
            let dest_was_empty = game.tableau[to].is_empty();

            // Filling the very last empty column (with cards still left to deal)
            // unlocks a Deal on the next move — often the only way out of a position
            // where nothing else makes progress — so it's rewarded instead of treated
            // like ordinary, undesirable free-column parking.
            let unlocks_deal = dest_was_empty
                && !game.stock.is_empty()
                && after.tableau.iter().all(|t| !t.is_empty());

            // Parking on a free column without an immediate uncover (and without
            // unlocking a deal) is judged almost entirely by `rollout_bonus` (does it
            // actually pay off within the next few moves?) rather than by this static
            // score. Without this early-out, the length/merge bonuses below would make
            // moving *any* long run onto a free column look attractive regardless of
            // whether it sets up real follow-up progress, spending spider's scarcest
            // resource on whatever card happened to be sitting on top.
            if dest_was_empty && !uncovers && !unlocks_deal {
                return -40;
            }

            let mut s = if uncovers { 60 } else { 5 };
            s += n as i32 * 2; // moving longer intact runs frees more later

            if unlocks_deal {
                s += 100;
            }

            // An emptied column is spider's most valuable asset — a free staging spot
            // for any card — so it's worth far more than an ordinary uncover.
            if after.tableau[from].is_empty() {
                s += 120;
            }

            // Reward same-suit consolidation by the SIZE OF THE NEW MERGE, not by how
            // long the resulting run happens to be in total: scaling with the total
            // (`run_len_after²`) rewarded relocating an already-long run onto one more
            // compatible card with a huge score every time, regardless of whether
            // anything new was actually achieved — which let the solver drag a 9-card
            // run back and forth between the same few piles indefinitely, since each
            // hop "scored" as if it were fresh progress. Scaling with just the gain
            // keeps a single real merge worth a fixed, modest amount no matter how big
            // the block being moved is.
            let run_len_after = after.suited_run_len(to) as i32;
            let merge_gain = run_len_after - n as i32;
            if merge_gain > 0 {
                s += merge_gain * merge_gain * 10;
            }

            s
        }
    }
}
