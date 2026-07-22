use cards::card::Card;

pub const NUM_COLS: usize = 10;
pub const TOTAL_RUNS: u32 = 8; // 104 cards / 13 = 8 complete K..A runs to win, any variant

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    Deal,
    TableauToTableau { from: usize, n: usize, to: usize },
}

#[derive(Clone, PartialEq)]
pub enum Phase {
    Playing,
    Won,
    Stuck,
}

/// Builds the 104-card spider deck for a given suit-count variant (1, 2, or 4 distinct
/// suits, each repeated enough times to fill 104 cards — 8, 4, and 2 copies respectively).
fn spider_deck(n_suits: u8) -> Vec<Card> {
    let suits: &[u8] = match n_suits {
        1 => &[3],
        2 => &[3, 2],
        _ => &[0, 1, 2, 3],
    };
    let copies = 8 / suits.len();
    let mut deck = Vec::with_capacity(104);
    for _ in 0..copies {
        for &s in suits {
            for r in 0..13u8 {
                deck.push(Card { rank: r, suit: s });
            }
        }
    }
    for i in (1..deck.len()).rev() {
        let j = (macroquad::rand::rand() as usize) % (i + 1);
        deck.swap(i, j);
    }
    deck
}

#[derive(Clone)]
pub struct Game {
    /// Undealt cards; a Deal pops one onto each of the 10 tableau piles.
    pub stock: Vec<Card>,
    pub tableau: [Vec<Card>; NUM_COLS],
    /// face-down card count per tableau pile (bottom n_down[i] cards are face-down)
    pub n_down: [usize; NUM_COLS],
    /// completed K..A same-suit runs banked so far (out of TOTAL_RUNS)
    pub completed: u32,
    /// how many distinct suits this deal uses: 1, 2, or 4
    pub n_suits: u8,
    pub generation: u32,
    pub moves: u32,
    pub phase: Phase,
    /// increments on moves that neither uncover a face-down card, complete a run, nor
    /// deal fresh cards; resets otherwise. Catches the solver shuffling in place long
    /// before the `moves` hard cap would.
    pub no_progress: u32,
}

impl Game {
    pub fn new(generation: u32, n_suits: u8) -> Self {
        let mut deck = spider_deck(n_suits);

        let mut tableau: [Vec<Card>; NUM_COLS] = Default::default();
        let mut n_down = [0usize; NUM_COLS];

        // First 4 piles get 6 cards, remaining 6 piles get 5 (54 dealt); bottom cards
        // face-down, top card face-up. Remaining 50 cards go to stock for 5 later deals.
        for i in 0..NUM_COLS {
            let count = if i < 4 { 6 } else { 5 };
            for _ in 0..count {
                tableau[i].push(deck.pop().unwrap());
            }
            n_down[i] = count - 1;
        }

        Self {
            stock: deck,
            tableau,
            n_down,
            completed: 0,
            n_suits,
            generation,
            moves: 0,
            phase: Phase::Playing,
            no_progress: 0,
        }
    }

    pub fn n_up(&self, pile: usize) -> usize {
        self.tableau[pile].len() - self.n_down[pile]
    }

    pub fn can_place_on_tableau(&self, card: Card, pile: usize) -> bool {
        match self.tableau[pile].last() {
            None => true,
            Some(&top) => card.rank + 1 == top.rank,
        }
    }

    /// Length of the contiguous same-suit descending-by-1 run sitting at the top of
    /// `pile` (face-up cards only). A lone top card always counts as a run of 1.
    pub fn suited_run_len(&self, pile: usize) -> usize {
        let t = &self.tableau[pile];
        let nd = self.n_down[pile];
        let mut n = 1;
        while nd + n < t.len() {
            let cur = t[t.len() - n];
            let prev = t[t.len() - n - 1];
            if prev.suit == cur.suit && prev.rank == cur.rank + 1 {
                n += 1;
            } else {
                break;
            }
        }
        n
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        if self.phase != Phase::Playing {
            return vec![];
        }
        let mut moves = Vec::new();

        for from in 0..NUM_COLS {
            let t = &self.tableau[from];
            if t.is_empty() {
                continue;
            }
            let n_up = self.n_up(from);
            let max_run = self.suited_run_len(from);

            for n in 1..=n_up {
                if n > 1 && n > max_run {
                    break;
                }
                let card = t[t.len() - n];
                for to in 0..NUM_COLS {
                    if to == from {
                        continue;
                    }
                    if self.can_place_on_tableau(card, to) {
                        moves.push(Move::TableauToTableau { from, n, to });
                    }
                }
            }
        }

        if !self.stock.is_empty() && self.tableau.iter().all(|t| !t.is_empty()) {
            moves.push(Move::Deal);
        }

        moves
    }

    pub fn apply(&mut self, m: Move) {
        let completed_before = self.completed;
        let n_down_before: usize = self.n_down.iter().sum();

        match m {
            Move::Deal => {
                for i in 0..NUM_COLS {
                    let c = self.stock.pop().unwrap();
                    self.tableau[i].push(c);
                }
                for i in 0..NUM_COLS {
                    self.try_complete(i);
                }
            }
            Move::TableauToTableau { from, n, to } => {
                let split = self.tableau[from].len() - n;
                let cards: Vec<Card> = self.tableau[from].drain(split..).collect();
                self.tableau[to].extend(cards);
                self.flip_top(from);
                self.try_complete(to);
            }
        }

        self.moves += 1;

        let n_down_after: usize = self.n_down.iter().sum();
        let progressed = matches!(m, Move::Deal)
            || self.completed > completed_before
            || n_down_after < n_down_before;
        if progressed {
            self.no_progress = 0;
        } else {
            self.no_progress += 1;
        }

        self.check_phase();
    }

    pub fn state_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for c in &self.stock {
            c.hash(&mut h);
        }
        0u8.hash(&mut h);
        for (i, t) in self.tableau.iter().enumerate() {
            self.n_down[i].hash(&mut h);
            for c in t {
                c.hash(&mut h);
            }
            1u8.hash(&mut h);
        }
        self.completed.hash(&mut h);
        h.finish()
    }

    /// After removing cards from a pile, flip the newly exposed top card if it was face-down.
    fn flip_top(&mut self, pile: usize) {
        let len = self.tableau[pile].len();
        if len == 0 {
            self.n_down[pile] = 0;
        } else if self.n_down[pile] > 0 && self.n_down[pile] >= len {
            self.n_down[pile] = len - 1;
        }
    }

    /// A complete K..A same-suit run sitting at the top of `pile` is automatically banked.
    fn try_complete(&mut self, pile: usize) {
        if self.suited_run_len(pile) >= 13 {
            let len = self.tableau[pile].len();
            self.tableau[pile].truncate(len - 13);
            self.completed += 1;
            self.flip_top(pile);
        }
    }

    fn check_phase(&mut self) {
        if self.completed == TOTAL_RUNS {
            self.phase = Phase::Won;
            return;
        }
        // Lowered from 200: the solver now filters out moves that lead straight back
        // into an already-visited state before picking (see Solver::choose_move), so a
        // strict dead-end cycle is caught almost immediately. What's left needing this
        // cap is the slower case — wandering through many *distinct* but equally
        // pointless rearrangements of a handful of interchangeable cleared piles, which
        // state-hash dedup can't recognize as equivalent — and real progress in every
        // observed successful game resets this counter well under 100 moves anyway.
        if self.no_progress > 100 || self.moves > 4000 || self.legal_moves().is_empty() {
            self.phase = Phase::Stuck;
        }
    }
}
