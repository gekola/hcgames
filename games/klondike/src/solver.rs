use std::collections::HashSet;
use cards::card::Card;
use crate::game::{Game, Move};

pub struct Solver {
    visited: HashSet<u64>,
}

impl Solver {
    pub fn new() -> Self {
        Self { visited: HashSet::new() }
    }

    pub fn choose_move(&mut self, game: &Game) -> Option<Move> {
        let moves: Vec<Move> = game.legal_moves()
            .into_iter()
            .filter(|m| !is_pointless_tableau_move(game, m))
            .filter(|m| !is_pointless_foundation_return(game, m))
            .collect();

        if moves.is_empty() {
            return None;
        }

        self.visited.insert(game.state_hash());

        moves.into_iter().max_by_key(|&m| {
            let is_draw = matches!(m, Move::DrawStock | Move::ResetStock);

            let mut preview = game.clone();
            preview.apply(m);
            let next_hash = preview.state_hash();

            let revisit_penalty = if !is_draw && self.visited.contains(&next_hash) {
                -1000
            } else {
                0
            };

            score(game, &m) + revisit_penalty
        })
    }
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

fn is_pointless_foundation_return(game: &Game, m: &Move) -> bool {
    let Move::FoundationToTableau { suit, to } = *m else {
        return false;
    };
    // Aces can never be built on in tableau.
    if game.foundations[suit].last().map_or(true, |c| c.rank == 0) {
        return true;
    }
    // Only worthwhile if placing this card onto `to` immediately enables a pile that has
    // face-down cards to stack there (uncovering move becomes available).
    let mut preview = game.clone();
    preview.apply(*m);
    let enables_uncover = preview.legal_moves().iter().any(|&mv| {
        matches!(mv, Move::TableauToTableau { from, n, to: mv_to }
            if mv_to == to && n == preview.n_up(from) && preview.n_down[from] > 0)
    });
    !enables_uncover
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

fn score(game: &Game, m: &Move) -> i32 {
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
