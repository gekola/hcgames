use beam_solver::{BeamSearch, SearchState};
use cards::card::Card;
use crate::game::{Game, Move, Phase, Variant};

/// See [[beam_solver]] docs and `games/spider/src/solver.rs` for the shared search
/// engine and its rationale; these constants and the move-scoring below are Klondike's
/// own tuning.
const BEAM_WIDTH: usize = 8;
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
            beam: BeamSearch::new(BEAM_WIDTH, BEAM_DEPTH, |m| {
                matches!(m, Move::DrawStock | Move::ResetStock)
            }),
        }
    }

    pub fn choose_move(&mut self, game: &Game) -> Option<Move> {
        self.beam.choose_move(game, is_pointless, score_root, score_core)
    }
}

/// is_pointless_tableau_move assumes a Klondike-style stock/waste fallback is always
/// there to draw from while waiting for a genuinely useful move, so it vetoes anything
/// that doesn't immediately uncover/foundation/empty a pile. Yukon has no stock at all —
/// it routinely *requires* lateral tableau shuffles (combining partial runs) before any
/// uncover becomes possible — so for Yukon that half is skipped and the beam search's own
/// revisit exclusion is what keeps it from thrashing. is_pointless_foundation_return
/// applies to both variants regardless: pulling a card off the foundation should always
/// earn its keep.
fn is_pointless(game: &Game, m: &Move) -> bool {
    let tableau_pointless = game.variant != Variant::Yukon && is_pointless_tableau_move(game, m);
    tableau_pointless || is_pointless_foundation_return(game, m)
}

/// TableauToTableau is only useful when it makes concrete immediate progress:
/// 1. Uncovers a face-down card, OR
/// 2. Exposes a card on `from` that can immediately go to the foundation, OR
/// 3. Empties a pile (no face-downs, whole run moved) when a King is available to fill it.
fn is_pointless_tableau_move(game: &Game, m: &Move) -> bool {
    let Move::TableauToTableau { from, n, to } = *m else {
        return false;
    };
    let n_up = game.n_up(from);

    // 1. Uncovers a face-down card
    if game.n_down[from] > 0 && n == n_up {
        return false;
    }

    // 2. Exposes a card ready for foundation
    let from_len = game.tableau[from].len();
    if from_len > n {
        let exposed = game.tableau[from][from_len - n - 1];
        if game.can_place_on_foundation(exposed) {
            return false;
        }
    }

    // 3. Empties the pile and a King is available to occupy it.
    //    But not if we're moving a King-led run to an empty column from a pile with no
    //    face-downs — that just swaps one free space for another.
    let bottom_card = game.tableau[from][from_len - n];
    let to_is_empty = game.tableau[to].is_empty();
    if n == n_up && game.n_down[from] == 0 && king_available(game)
        && !(bottom_card.rank == 12 && to_is_empty)
    {
        return false;
    }

    true
}

/// A card only leaves the foundation when it immediately pays for itself: placing it onto
/// `to` must let some other pile's face-up run land on top of it in a way that either
/// flips a face-down card, or exposes a card that can go straight to a foundation.
fn is_pointless_foundation_return(game: &Game, m: &Move) -> bool {
    let Move::FoundationToTableau { suit, to } = *m else {
        return false;
    };
    // Aces can never be built on in tableau.
    if game.foundations[suit].last().map_or(true, |c| c.rank == 0) {
        return true;
    }
    let mut preview = game.clone();
    preview.apply(*m);
    let unlocks_something = preview.legal_moves().iter().any(|&mv| {
        let Move::TableauToTableau { from, n, to: mv_to } = mv else {
            return false;
        };
        if mv_to != to {
            return false;
        }
        // Fully clears the face-up run, flipping the face-down card beneath it.
        if n == preview.n_up(from) && preview.n_down[from] > 0 {
            return true;
        }
        // Exposes a card that can go straight to a foundation.
        let from_len = preview.tableau[from].len();
        from_len > n && preview.can_place_on_foundation(preview.tableau[from][from_len - n - 1])
    });
    !unlocks_something
}

fn king_available(game: &Game) -> bool {
    if game.waste.last().map_or(false, |c| c.rank == 12) {
        return true;
    }
    // A King at the bottom of a face-up run that isn't already alone on an empty pile
    game.tableau.iter().enumerate().any(|(i, t)| {
        let nd = game.n_down[i];
        !t.is_empty() && t[nd].rank == 12 && !(nd == 0 && t.len() == 1)
    })
}

/// Ply-0 evaluation for the real decision: `score_core` plus a mobility term (how many
/// productive moves TableauToTableau leaves behind). Affordable here since, like Spider's
/// `score`, it only runs once per real candidate rather than at every later ply.
fn score_root(game: &Game, after: &Game, m: &Move) -> i32 {
    let mut s = score_core(game, after, m);

    if matches!(m, Move::TableauToTableau { .. }) {
        let follow_ups = after.legal_moves().iter().filter(|mv| !is_pointless(after, mv)).count() as i32;
        s += follow_ups;
    }

    s
}

/// Cheap per-move scorer used at every ply of the beam (including ply 0, via
/// `score_root`). `after.phase == Won` always wins outright — finishing the game beats
/// any other consideration, mirroring Spider's "banking a run always wins" rule.
fn score_core(game: &Game, after: &Game, m: &Move) -> i32 {
    if after.phase == Phase::Won {
        return 100_000;
    }

    match *m {
        Move::WasteToFoundation => 110,

        Move::TableauToFoundation(from) => {
            let uncovers = game.n_down[from] > 0
                && game.tableau[from].len() == game.n_down[from] + 1;
            100 + if uncovers { 15 } else { 0 }
        }

        Move::WasteToTableau(to) => {
            let card = *game.waste.last().unwrap();
            score_tab(game, card, to, false)
        }

        Move::TableauToTableau { from, n, to } => {
            let t = &game.tableau[from];
            let card = t[t.len() - n];
            let uncovers = game.n_down[from] > 0 && n == game.n_up(from);
            score_tab(game, card, to, uncovers)
        }

        Move::FoundationToTableau { suit, to } => {
            // Only useful if it enables uncovering a face-down card:
            // the pile `to` must have face-downs and placing here lets us
            // subsequently move the whole face-up run off another pile.
            // Approximate: reward if any pile has face-downs and placing here
            // extends a sequence that could move onto a pile with face-downs.
            let _ = (suit, to); // scoring is context-free; cycle detection handles misuse
            15
        }

        Move::DrawStock => 1,
        Move::ResetStock => 0,
    }
}

fn score_tab(game: &Game, card: Card, to: usize, uncovers: bool) -> i32 {
    let mut s = if uncovers { 30 } else { 5 };

    // Moving a King to an empty column is only valuable when it frees a face-down card.
    if card.rank == 12 && game.tableau[to].is_empty() {
        s += if uncovers { 20 } else { -3 };
    }

    s
}
