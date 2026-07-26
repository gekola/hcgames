use crate::game::Piece;
use macroquad::rand;
use std::collections::VecDeque;

/// Piece-generation algorithm, matching the RNG shape of well-known real Tetris
/// implementations rather than one made-up distribution. See each `next_*` method for
/// the specific behavior being approximated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenMode {
    /// Modern Guideline standard (used by every officially licensed Tetris since ~2001,
    /// and most competitive clones): shuffle all 7 pieces into a "bag", deal it
    /// one-by-one, then shuffle a fresh bag — every piece appears exactly once per 7,
    /// bounding the longest possible drought/flood at 12 pieces (2 bags).
    Bag7,
    /// The original 1989 NES Tetris randomizer: uniform over all 7 pieces, with a single
    /// reroll if the result matches the immediately preceding piece. This is a faithful
    /// behavioral simplification of the real 8-entry-table-with-reroll algorithm (the
    /// original also treats an invalid 8th table slot as a same-as-current signal) —
    /// famous for occasionally producing long same-piece "droughts" (especially of `I`)
    /// that the real reroll doesn't fully prevent, which is the point of including it as
    /// a distinct mode from `Bag7`.
    Classic,
    /// Arika's "Grand Master" series randomizer: reroll a candidate piece (uniform over
    /// all 7) up to 4 times if it's one of the last 4 pieces dealt, accepting whatever the
    /// 5th roll is regardless. Softer anti-repeat than `Bag7` (still allows droughts,
    /// just much shorter ones) without being a strict permutation. TGM additionally never
    /// deals `S` or `Z` as the very first piece of a game, avoiding a start that forces an
    /// immediate overhang.
    Tgm,
    /// Pure uniform random, no history of any kind — the naive randomizer many casual/
    /// early clones actually shipped before bagging became standard. Kept as its own mode
    /// specifically because it *can* drought or flood, unlike the other three.
    Memoryless,
}

pub struct PieceGenerator {
    mode: GenMode,
    bag: Vec<Piece>,
    prev: Option<Piece>,
    history: VecDeque<Piece>,
    first: bool,
}

fn random_piece() -> Piece {
    Piece::ALL[rand::gen_range(0usize, Piece::ALL.len())]
}

impl PieceGenerator {
    pub fn new(mode: GenMode) -> Self {
        Self {
            mode,
            bag: Vec::new(),
            prev: None,
            history: VecDeque::new(),
            first: true,
        }
    }

    pub fn next(&mut self) -> Piece {
        let piece = match self.mode {
            GenMode::Bag7 => self.next_bag7(),
            GenMode::Classic => self.next_classic(),
            GenMode::Tgm => self.next_tgm(),
            GenMode::Memoryless => random_piece(),
        };
        self.first = false;
        piece
    }

    fn next_bag7(&mut self) -> Piece {
        if self.bag.is_empty() {
            self.bag = Piece::ALL.to_vec();
            // Fisher-Yates.
            for i in (1..self.bag.len()).rev() {
                let j = rand::gen_range(0usize, i + 1);
                self.bag.swap(i, j);
            }
        }
        self.bag.pop().unwrap()
    }

    fn next_classic(&mut self) -> Piece {
        let mut candidate = random_piece();
        if Some(candidate) == self.prev {
            candidate = random_piece();
        }
        self.prev = Some(candidate);
        candidate
    }

    fn next_tgm(&mut self) -> Piece {
        let candidate = if self.first {
            // Never open on S or Z, matching TGM's first-piece rule.
            const OPENERS: [Piece; 4] = [Piece::I, Piece::J, Piece::L, Piece::T];
            OPENERS[rand::gen_range(0usize, OPENERS.len())]
        } else {
            let mut c = random_piece();
            for _ in 0..4 {
                if !self.history.contains(&c) {
                    break;
                }
                c = random_piece();
            }
            c
        };
        self.history.push_back(candidate);
        if self.history.len() > 4 {
            self.history.pop_front();
        }
        candidate
    }
}
