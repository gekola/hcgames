use cards::card::{Card, shuffled_deck};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    DrawStock,
    ResetStock,
    WasteToFoundation,
    WasteToTableau(usize),
    TableauToFoundation(usize),
    TableauToTableau { from: usize, n: usize, to: usize },
    FoundationToTableau { suit: usize, to: usize },
}

#[derive(Clone, PartialEq)]
pub enum Phase {
    Playing,
    Won,
    Stuck,
}

#[derive(Clone)]
pub struct Game {
    pub stock: Vec<Card>,
    pub waste: Vec<Card>,
    pub foundations: [Vec<Card>; 4],
    pub tableau: [Vec<Card>; 7],
    /// face-down card count per tableau pile (bottom n_down[i] cards are face-down)
    pub n_down: [usize; 7],
    pub draw_count: u8,
    pub generation: u32,
    pub moves: u32,
    pub phase: Phase,
    /// increments on DrawStock/ResetStock, resets on any other move; detects stock cycle
    pub no_progress: u32,
}

impl Game {
    pub fn new(generation: u32) -> Self {
        let draw_count = if generation % 2 == 0 { 1 } else { 3 };
        let mut deck = shuffled_deck();

        let mut tableau: [Vec<Card>; 7] = Default::default();
        let mut n_down = [0usize; 7];

        // Pile i gets i+1 cards; bottom i are face-down, top 1 face-up.
        for i in 0..7 {
            for _ in 0..=i {
                tableau[i].push(deck.pop().unwrap());
            }
            n_down[i] = i;
        }

        // Remaining 24 cards go face-down to stock (top = last element).
        Self {
            stock: deck,
            waste: Vec::new(),
            foundations: Default::default(),
            tableau,
            n_down,
            draw_count,
            generation,
            moves: 0,
            phase: Phase::Playing,
            no_progress: 0,
        }
    }

    pub fn can_place_on_foundation(&self, card: Card) -> bool {
        self.foundations[card.suit as usize].len() == card.rank as usize
    }

    pub fn can_place_on_tableau(&self, card: Card, pile: usize) -> bool {
        let t = &self.tableau[pile];
        if t.is_empty() {
            card.rank == 12 // King to empty column
        } else {
            let top = t[t.len() - 1];
            card.color_bit() != top.color_bit() && card.rank + 1 == top.rank
        }
    }

    pub fn n_up(&self, pile: usize) -> usize {
        self.tableau[pile].len() - self.n_down[pile]
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        if self.phase != Phase::Playing {
            return vec![];
        }
        let mut moves = Vec::new();

        // Waste top → foundation or tableau
        if let Some(&c) = self.waste.last() {
            if self.can_place_on_foundation(c) {
                moves.push(Move::WasteToFoundation);
            }
            for i in 0..7 {
                if self.can_place_on_tableau(c, i) {
                    moves.push(Move::WasteToTableau(i));
                }
            }
        }

        // Tableau → foundation or tableau
        for from in 0..7 {
            let t = &self.tableau[from];
            if t.is_empty() {
                continue;
            }
            let n_up = self.n_up(from);

            // Only top card can go to foundation
            let top = t[t.len() - 1];
            if self.can_place_on_foundation(top) {
                moves.push(Move::TableauToFoundation(from));
            }

            // Any face-up sub-sequence (bottom card of the group must fit target)
            for n in 1..=n_up {
                let card = t[t.len() - n];
                for to in 0..7 {
                    if to == from {
                        continue;
                    }
                    if self.can_place_on_tableau(card, to) {
                        moves.push(Move::TableauToTableau { from, n, to });
                    }
                }
            }
        }

        // Foundation → tableau (to reorganise and unblock face-downs)
        for suit in 0..4 {
            if let Some(&c) = self.foundations[suit].last() {
                for to in 0..7 {
                    if self.can_place_on_tableau(c, to) {
                        moves.push(Move::FoundationToTableau { suit, to });
                    }
                }
            }
        }

        // Stock / waste cycling
        if !self.stock.is_empty() {
            moves.push(Move::DrawStock);
        } else if !self.waste.is_empty() {
            moves.push(Move::ResetStock);
        }

        moves
    }

    pub fn apply(&mut self, m: Move) {
        match m {
            Move::DrawStock => {
                let n = (self.draw_count as usize).min(self.stock.len());
                for _ in 0..n {
                    let c = self.stock.pop().unwrap();
                    self.waste.push(c);
                }
            }
            Move::ResetStock => {
                while let Some(c) = self.waste.pop() {
                    self.stock.push(c);
                }
            }
            Move::WasteToFoundation => {
                let c = self.waste.pop().unwrap();
                self.foundations[c.suit as usize].push(c);
            }
            Move::WasteToTableau(i) => {
                let c = self.waste.pop().unwrap();
                self.tableau[i].push(c);
            }
            Move::TableauToFoundation(from) => {
                let c = self.tableau[from].pop().unwrap();
                self.foundations[c.suit as usize].push(c);
                self.flip_top(from);
            }
            Move::TableauToTableau { from, n, to } => {
                let split = self.tableau[from].len() - n;
                let cards: Vec<Card> = self.tableau[from].drain(split..).collect();
                self.tableau[to].extend(cards);
                self.flip_top(from);
            }
            Move::FoundationToTableau { suit, to } => {
                let c = self.foundations[suit].pop().unwrap();
                self.tableau[to].push(c);
            }
        }

        self.moves += 1;
        match m {
            Move::ResetStock => self.no_progress += 1, // full lap completed
            Move::DrawStock => {}                       // mid-lap, don't count
            _ => self.no_progress = 0,                 // productive move resets lap counter
        }
        self.check_phase();
    }

    pub fn state_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for c in &self.stock { c.hash(&mut h); }
        0u8.hash(&mut h);
        for c in &self.waste { c.hash(&mut h); }
        1u8.hash(&mut h);
        for f in &self.foundations {
            for c in f { c.hash(&mut h); }
            2u8.hash(&mut h);
        }
        for (i, t) in self.tableau.iter().enumerate() {
            self.n_down[i].hash(&mut h);
            for c in t { c.hash(&mut h); }
            3u8.hash(&mut h);
        }
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

    fn check_phase(&mut self) {
        if self.foundations.iter().all(|f| f.len() == 13) {
            self.phase = Phase::Won;
            return;
        }
        // Draw-1: give up after 3 full laps with no productive move, or 2000 total moves.
        // Draw-3: allow more laps (harder to find cards) and far more total moves.
        let (max_laps, max_moves) = if self.draw_count == 1 {
            (3, 2000)
        } else {
            (6, 8000)
        };
        if self.no_progress > max_laps || self.moves > max_moves {
            self.phase = Phase::Stuck;
        }
    }
}
