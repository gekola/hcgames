use crate::game::{Game, Move, Phase};
use beam_solver::{BeamSearch, SearchState};

/// How many of the best candidate lines to keep alive at each ply of the search — a
/// small beam search (Bjarnason/Cazenave-adjacent: cheaper than a full rollout per
/// candidate, since it shares work across candidates by pruning to the best few after
/// every ply instead of running each one out to full depth independently).
const BEAM_WIDTH: usize = 32;
/// How many plies deep to search. Ply 0 is the real decision (one node per legal first
/// move); the remaining plies project forward with the cheap `score_core` step policy
/// to judge which of those first moves actually leads somewhere, rather than picking
/// whichever looks best after a single step.
const BEAM_DEPTH: u32 = 8;
/// Caps total `legal_moves()` candidates expanded per decision (all plies combined) — a
/// small number of pathological board states (several empty tableau columns colliding
/// with a long-run consolidation opportunity) blow `BEAM_WIDTH`/`BEAM_DEPTH`'s typical
/// cost up by 100x+; this bounds the worst case instead of gambling on it. See
/// `beam_solver::BeamSearch`'s `node_budget` docs. Tuned empirically against the live
/// tick budget (`TICK` in `main.rs`, 160ms) with margin for slower hardware.
const NODE_BUDGET: usize = 1500;

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
            beam: BeamSearch::new(BEAM_WIDTH, BEAM_DEPTH, NODE_BUDGET, |m| {
                matches!(m, Move::Deal)
            }),
        }
    }

    /// `debug` additionally dumps the full board (every column's face-up cards) and the
    /// raw legal-move list whenever no move is found, i.e. right as the game is about to
    /// go `Stuck` — the single most useful moment to see the board for diagnosing
    /// "solver reports Stuck but a human would see a move" reports, since by the time
    /// `--once`'s summary line prints, the interesting state is already gone.
    pub fn choose_move(&mut self, game: &Game, debug: bool) -> Option<Move> {
        let m = self.beam.choose_move(game, is_pointless, score, score_core);
        if m.is_none() && debug {
            diagnose_stuck(game);
        }
        m
    }
}

fn diagnose_stuck(game: &Game) {
    eprintln!(
        "stuck: stock={} completed={}/8 n_down={:?}",
        game.stock.len(),
        game.completed,
        game.n_down
    );
    for (i, col) in game.tableau.iter().enumerate() {
        eprintln!(
            "stuck: col{i} n_down={} up={:?}",
            game.n_down[i],
            &col[game.n_down[i]..]
        );
    }
    for i in 0..game.tableau.len() {
        if !game.tableau[i].is_empty() {
            eprintln!(
                "stuck: col{i} n_up={} suited_run_len={}",
                game.n_up(i),
                game.suited_run_len(i)
            );
        }
    }
    let raw = game.legal_moves();
    eprintln!("stuck: raw_legal_moves={}", raw.len());
    for m in &raw {
        let Move::TableauToTableau { from, n, to } = *m else {
            continue;
        };
        eprintln!(
            "stuck: candidate from={from} n={n} to={to} pointless={}",
            is_pointless(game, m)
        );
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

/// Truly pointless moves, filtered out entirely rather than merely down-scored:
///
/// 1. Relocating an entire already-fully-exposed pile onto another empty column: it
///    changes nothing but the column index. Everything else legal_moves() already
///    guarantees is rank-compatible with its destination (that's required to generate
///    the move at all), including landing on a different-suit card: that's ordinary
///    spider play — it can't move as a group later or bank, but it still consolidates a
///    pile and reduces clutter — so `score_core` prices it lower than a same-suit build
///    rather than this function rejecting it outright. Rejecting it outright was the
///    bug: it left "shuffle a parked card to another empty column" as the only move left
///    in exactly the position where "stack it on a rank-compatible card of a different
///    suit" was the obviously better play.
///
///    **Known remaining gap (reported by the user, not yet fixed — see project_spider
///    memory for the failed attempt and its data):** this rule only exempts a *whole*
///    exposed pile move between empties from being pointless; every *partial* peel into
///    an empty column is still unconditionally pointless too (via rule 2's fallthrough
///    below), even when `from` would stay non-empty and the move would genuinely reduce
///    the empty-column count — sometimes the *only* way to ever fill every column and
///    unlock a `Deal`. Confirmed via `--debug`: positions exist (several empty columns,
///    remaining piles each a single clean same-suit run) where the game reports Stuck
///    with `stock` still non-empty because of exactly this. The obvious fix — allow a
///    partial peel to empty whenever `from` stays non-empty — was A/B'd and caused a
///    severe regression (2-suit and 4-suit dropped to 0/30 wins, 1-suit avg moves nearly
///    tripled): the existing `-40` score penalty and revisit-exclusion were not enough to
///    keep the search from routinely picking it. Needs a narrower, still-undiscovered
///    condition (parallel to rule 2's "beats `from`'s own length" bar) before shipping.
/// 2. Peeling a partial slice off the top of an already-correctly-ordered same-suit run
///    (`n` less than the source's `suited_run_len`) UNLESS the resulting run at the
///    destination ends up strictly longer than `from`'s own run was before the peel —
///    i.e. this move only pays for "discarding" (breaking up) that source run if what
///    you get elsewhere is a genuinely bigger structure than what you had. Moving the
///    *whole* run (`n == suited_run_len`) is unaffected by this rule and always allowed.
///    A landing on an empty column, or on a different-suit card, always fails this test
///    (the resulting "run" is at most `n`, which is by definition less than
///    `suited_run_len(from)`), so this single condition also covers those cases (and the
///    known gap above) without a separate check.
///
///    Earlier, weaker attempts at this exception and why they failed, in order:
///    - "any destination" (no exception at all): the original, strictest form. Blocks a
///      genuinely useful move whenever the only way forward is landing a peeled slice on
///      a real card elsewhere — confirmed via `--debug`: a position with 6 empty columns,
///      4 clean single-suit-run piles, and exactly one real cross-pile extension
///      available, where this forced a false Stuck.
///    - "any non-empty destination": A/B'd, reopened the merge-oscillation exploit this
///      rule exists to prevent (1-suit win rate 22/30→7/30, avg moves 123→454) — once
///      peeling onto *any* real card is legal, `merge_gain²` scoring makes shuffling a
///      slice back and forth between two different growing piles look attractive at
///      every step, and revisit-exclusion doesn't catch it because each hop passes
///      through a distinct intermediate state.
///    - "same-suit merge only": narrower, but in the 1-suit variant *every* card is the
///      same suit, so this is a no-op restriction there and reopens the identical
///      1-suit regression.
///    - "same-suit merge AND beats the board's longest run anywhere": fixed the 1-suit
///      regression, but requires scanning every column on every candidate move.
///    - Landed on: same-suit merge AND beats `from`'s own run length specifically —
///      cheaper (no board scan) and, A/B'd, performs at least as well: 1-suit 21/30 (was
///      22/30 with the board-wide version — within noise), 2-suit 16/30 (was 14/30),
///      4-suit 3/30 (was 1/30). Also architecturally simpler than the two-tier
///      strict/relaxed fallback design tried in between — this is the *only* rule now,
///      active during normal search rather than gated behind "every other move is
///      pointless," so the beam's own multi-ply lookahead can find a multi-step
///      reassembly chain on its own instead of being limited to one rescued move at a
///      time.
/// 3. Parking on an empty column when a *lower-indexed* empty column is also available
///    for the same `(from, n)`: every empty column is interchangeable as a destination —
///    landing on column 3 vs column 7 produces isomorphic future states differing only by
///    which index is now empty instead of which is occupied — so only the lowest-indexed
///    empty column is ever generated as a candidate. With K empty columns this collapses
///    K duplicate branches (that a beam search or human would otherwise treat as distinct
///    options to evaluate) into 1; this is also exactly where the search's worst-case
///    combinatorial blowups were observed to come from.
fn is_pointless(game: &Game, m: &Move) -> bool {
    let Move::TableauToTableau { from, n, to } = *m else {
        return false; // Deal is never pointless
    };

    let dest_empty = game.tableau[to].is_empty();

    if dest_empty && n == game.n_up(from) && game.n_down[from] == 0 {
        return true;
    }
    if dest_empty && (0..to).any(|i| i != from && game.tableau[i].is_empty()) {
        return true;
    }

    if n < game.suited_run_len(from) {
        let moved_card = game.tableau[from][game.tableau[from].len() - n];
        let same_suit = !dest_empty && game.tableau[to].last().unwrap().suit == moved_card.suit;
        let new_len = if same_suit {
            game.suited_run_len(to) + n
        } else {
            n
        };
        return new_len <= game.suited_run_len(from);
    }

    false
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
