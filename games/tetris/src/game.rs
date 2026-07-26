use crate::generator::PieceGenerator;
use std::collections::VecDeque;

pub const W: usize = 10;
pub const H: usize = 20;

/// Real-piece `queue` is kept refilled to at least this many entries ahead of `current`
/// (see `Game::refill`) — one more than the solver's beam search depth actually needs
/// (`solver::BEAM_DEPTH` is 2: current piece + one known lookahead), so there's always at
/// least one spare for the "NEXT" UI preview to show even mid-search.
pub const LOOKAHEAD: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Piece {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl Piece {
    pub const ALL: [Piece; 7] = [
        Piece::I,
        Piece::O,
        Piece::T,
        Piece::S,
        Piece::Z,
        Piece::J,
        Piece::L,
    ];

    /// Cells (col, row) of this piece's spawn orientation within its `size x size`
    /// bounding box — the standard Tetris Guideline shapes. `rotation_states` derives
    /// every other orientation from this by rotating the whole box, so only one shape
    /// per piece needs transcribing here.
    fn base_shape(self) -> ([(i32, i32); 4], i32) {
        match self {
            Piece::I => ([(0, 1), (1, 1), (2, 1), (3, 1)], 4),
            Piece::O => ([(0, 0), (1, 0), (0, 1), (1, 1)], 2),
            Piece::T => ([(1, 0), (0, 1), (1, 1), (2, 1)], 3),
            Piece::S => ([(1, 0), (2, 0), (0, 1), (1, 1)], 3),
            Piece::Z => ([(0, 0), (1, 0), (1, 1), (2, 1)], 3),
            Piece::J => ([(0, 0), (0, 1), (1, 1), (2, 1)], 3),
            Piece::L => ([(2, 0), (0, 1), (1, 1), (2, 1)], 3),
        }
    }
}

pub type Cell = Option<Piece>;
pub type Board = [[Cell; W]; H];

/// Rotates `cells` 90° clockwise within a `size x size` box — the standard
/// `(x, y) -> (size-1-y, x)` transform. Must be applied to the *unnormalized* shape each
/// step (not the shrunk/normalized one `rotation_states` stores) or successive rotations
/// drift out of the box they're meant to rotate within.
fn rotate_cw(cells: [(i32, i32); 4], size: i32) -> [(i32, i32); 4] {
    let mut out = [(0, 0); 4];
    for (i, &(x, y)) in cells.iter().enumerate() {
        out[i] = (size - 1 - y, x);
    }
    out
}

/// Shifts `cells` so the minimum x and y are both 0 — the form every placement/rendering
/// call actually wants (offsets from a piece-local origin), independent of which box size
/// the shape was rotated within.
fn normalize(cells: [(i32, i32); 4]) -> [(i32, i32); 4] {
    let min_x = cells.iter().map(|c| c.0).min().unwrap();
    let min_y = cells.iter().map(|c| c.1).min().unwrap();
    let mut out = [(0, 0); 4];
    for (i, &(x, y)) in cells.iter().enumerate() {
        out[i] = (x - min_x, y - min_y);
    }
    out
}

/// Every distinct rotation of `piece`, normalized to offsets from `(0, 0)`. `O` yields 1
/// (rotationally symmetric), `I`/`S`/`Z` yield 2, `T`/`J`/`L` yield 4 — deduplicated by
/// comparing sorted cell sets, not just counted, so a piece's fold symmetry doesn't need
/// to be hardcoded anywhere. No wall-kick tables: this game never rotates a piece that's
/// already resting against the board (the solver enumerates every (rotation, column) pair
/// and hard-drops it fresh from the top), so kicks — which only matter for rotating a
/// piece that's already mid-fall against neighbors — don't apply here.
pub fn rotation_states(piece: Piece) -> Vec<[(i32, i32); 4]> {
    let (base, size) = piece.base_shape();
    let mut states: Vec<[(i32, i32); 4]> = Vec::new();
    let mut cur = base;
    for _ in 0..4 {
        let norm = normalize(cur);
        let mut key = norm;
        key.sort_unstable();
        let seen = states.iter().any(|s| {
            let mut k = *s;
            k.sort_unstable();
            k == key
        });
        if !seen {
            states.push(norm);
        }
        cur = rotate_cw(cur, size);
    }
    states
}

/// Writes `shape` (already offset by `col`/`row`) into `board` as `piece`-colored cells.
/// Shared by `Game::apply` (the authoritative lock) and the renderer (reconstructing the
/// pre-line-clear board for the lock-flash frame) so both draw from the same logic.
pub fn place_cells(board: &mut Board, piece: Piece, shape: &[(i32, i32); 4], col: i32, row: i32) {
    for &(dx, dy) in shape {
        board[(row + dy) as usize][(col + dx) as usize] = Some(piece);
    }
}

/// Row indices that are completely filled — shared by `Game::apply` (to actually clear
/// them) and the renderer (to know which rows to flash before they vanish).
pub fn full_rows(board: &Board) -> Vec<usize> {
    (0..H)
        .filter(|&r| board[r].iter().all(|c| c.is_some()))
        .collect()
}

/// Height of each column — the tallest filled cell's distance from the floor, 0 if the
/// column is empty. Used by the solver's board-evaluation heuristic.
pub fn column_heights(board: &Board) -> [i32; W] {
    let mut heights = [0i32; W];
    for c in 0..W {
        for (r, row) in board.iter().enumerate() {
            if row[c].is_some() {
                heights[c] = (H - r) as i32;
                break;
            }
        }
    }
    heights
}

/// Empty cells with a filled cell somewhere above them in the same column — buried holes
/// a piece can no longer drop straight into. Used by the solver's board-evaluation
/// heuristic.
pub fn count_holes(board: &Board, heights: &[i32; W]) -> i32 {
    let mut holes = 0;
    for c in 0..W {
        let top = H as i32 - heights[c];
        if top < 0 {
            continue;
        }
        holes += board[(top as usize)..]
            .iter()
            .filter(|row| row[c].is_none())
            .count() as i32;
    }
    holes
}

/// A final resting placement for `Game::current` — not a step of player input, since this
/// game has no player: the solver picks the rotation and column directly and the piece
/// hard-drops there in one move. `row` is precomputed by `legal_moves` (the same drop
/// simulation `apply` would otherwise have to repeat) so `apply` can't disagree with the
/// placement the solver actually scored.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Placement {
    pub rot: u8,
    pub col: i32,
    pub row: i32,
}
pub type Move = Placement;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    GameOver,
}

#[derive(Clone)]
pub struct Game {
    pub board: Board,
    pub current: Piece,
    pub queue: VecDeque<Piece>,
    pub score: u32,
    pub lines: u32,
    pub level: u32,
    pub generation: u32,
    pub phase: Phase,
    /// Lines cleared by the most recent `apply` call — read by the solver's heuristic
    /// (which scores a resulting board, not a delta, so the "lines" term needs this
    /// rather than `self.lines`) and by the renderer (to know whether to play the
    /// lock-flash phase at all).
    pub last_lines_cleared: u32,
}

impl Game {
    pub fn new(generation: u32, piece_gen: &mut PieceGenerator) -> Self {
        let current = piece_gen.next();
        let mut queue = VecDeque::new();
        for _ in 0..LOOKAHEAD {
            queue.push_back(piece_gen.next());
        }
        Self {
            board: [[None; W]; H],
            current,
            queue,
            score: 0,
            lines: 0,
            level: 0,
            generation,
            phase: Phase::Playing,
            last_lines_cleared: 0,
        }
    }

    /// Tops the queue back up to `LOOKAHEAD`. Must be called from the real game loop
    /// (headless or windowed) before every `Solver::choose_move` — never from inside a
    /// search-cloned `Game`, since it draws from `piece_gen`'s RNG and `SearchState::apply`
    /// must stay deterministic (see `beam_solver`'s docs). The search only ever consumes
    /// from the pre-filled queue it was handed, same as Klondike/Spider drawing from an
    /// already-shuffled deck rather than shuffling mid-search.
    pub fn refill(&mut self, piece_gen: &mut PieceGenerator) {
        while self.queue.len() < LOOKAHEAD {
            self.queue.push_back(piece_gen.next());
        }
    }

    /// Every (rotation, column) placement `current` could hard-drop into right now, each
    /// already carrying its landing row.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut out = Vec::new();
        for (rot, shape) in rotation_states(self.current).into_iter().enumerate() {
            let width = shape.iter().map(|c| c.0).max().unwrap() + 1;
            for col in 0..=(W as i32 - width) {
                if let Some(row) = self.drop_row(&shape, col) {
                    out.push(Placement {
                        rot: rot as u8,
                        col,
                        row,
                    });
                }
            }
        }
        out
    }

    /// Simulates a hard drop of `shape` at `col`, starting top-aligned (`row = 0`).
    /// `None` means even the top-aligned position collides — the stack under these
    /// columns has grown too tall for this piece/rotation to enter the board at all.
    fn drop_row(&self, shape: &[(i32, i32); 4], col: i32) -> Option<i32> {
        let collides = |row: i32| {
            shape.iter().any(|&(dx, dy)| {
                let r = row + dy;
                r >= H as i32 || self.board[r as usize][(col + dx) as usize].is_some()
            })
        };
        if collides(0) {
            return None;
        }
        let mut row = 0;
        while !collides(row + 1) {
            row += 1;
        }
        Some(row)
    }

    /// FNV-1a over the board plus current/near-future pieces — cheap revisit dedup for
    /// the beam search. In practice a repeated hash is essentially impossible here (every
    /// placement changes the board), but `beam_solver` expects every `SearchState` to
    /// provide one.
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |b: u8| {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        };
        for row in &self.board {
            for cell in row {
                mix(cell.map(|p| p as u8 + 1).unwrap_or(0));
            }
        }
        mix(self.current as u8);
        for &p in self.queue.iter().take(2) {
            mix(p as u8 + 1);
        }
        h
    }

    /// Locks `current` into `m`'s placement, clears completed lines, and advances to the
    /// next piece from `queue`. Sets `phase` to `GameOver` if the new `current` has
    /// nowhere left to go — the board has topped out under every column that piece's
    /// shape could occupy.
    pub fn apply(&mut self, m: Move) {
        let shape = rotation_states(self.current)[m.rot as usize];
        place_cells(&mut self.board, self.current, &shape, m.col, m.row);

        let cleared_rows = full_rows(&self.board);
        let cleared = cleared_rows.len() as u32;
        if cleared > 0 {
            let mut new_rows: Vec<[Cell; W]> = Vec::with_capacity(H);
            for r in 0..H {
                if !cleared_rows.contains(&r) {
                    new_rows.push(self.board[r]);
                }
            }
            while new_rows.len() < H {
                new_rows.insert(0, [None; W]);
            }
            for (r, row) in new_rows.into_iter().enumerate() {
                self.board[r] = row;
            }
        }
        self.last_lines_cleared = cleared;
        self.lines += cleared;
        self.score += line_score(cleared, self.level);
        self.level = self.lines / 10;

        self.current = self
            .queue
            .pop_front()
            .expect("queue must be refilled before apply — see Game::refill");
        if self.legal_moves().is_empty() {
            self.phase = Phase::GameOver;
        }
    }
}

/// Classic (NES-style) line-clear scoring: 1/2/3/4 lines at once score progressively more
/// than the sum of clearing them separately would, scaled up by the current level.
fn line_score(cleared: u32, level: u32) -> u32 {
    let base = match cleared {
        1 => 40,
        2 => 100,
        3 => 300,
        4 => 1200,
        _ => 0,
    };
    base * (level + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_counts_match_known_symmetry() {
        assert_eq!(rotation_states(Piece::O).len(), 1);
        assert_eq!(rotation_states(Piece::I).len(), 2);
        assert_eq!(rotation_states(Piece::S).len(), 2);
        assert_eq!(rotation_states(Piece::Z).len(), 2);
        assert_eq!(rotation_states(Piece::T).len(), 4);
        assert_eq!(rotation_states(Piece::J).len(), 4);
        assert_eq!(rotation_states(Piece::L).len(), 4);
    }

    #[test]
    fn every_rotation_is_four_connected_cells_from_the_origin() {
        for piece in Piece::ALL {
            for shape in rotation_states(piece) {
                assert_eq!(shape.iter().map(|c| c.0).min(), Some(0));
                assert_eq!(shape.iter().map(|c| c.1).min(), Some(0));
                // Edge-connectivity: every cell reachable from cell 0 via unit steps.
                let mut reached = vec![shape[0]];
                loop {
                    let before = reached.len();
                    for &(x, y) in &shape {
                        let touches = reached
                            .iter()
                            .any(|&(rx, ry)| (rx - x).abs() + (ry - y).abs() == 1);
                        if touches && !reached.contains(&(x, y)) {
                            reached.push((x, y));
                        }
                    }
                    if reached.len() == before {
                        break;
                    }
                }
                assert_eq!(reached.len(), 4, "{piece:?} shape {shape:?} not connected");
            }
        }
    }

    #[test]
    fn four_rotations_return_to_the_start() {
        for piece in Piece::ALL {
            let (base, size) = piece.base_shape();
            let mut cur = base;
            for _ in 0..4 {
                cur = rotate_cw(cur, size);
            }
            let mut a = normalize(base);
            let mut b = normalize(cur);
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b);
        }
    }
}
