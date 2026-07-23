pub const N: usize = 9;
pub const CELLS: usize = 81;
const FULL_MASK: u16 = 0b1_1111_1111;

pub fn row(idx: usize) -> usize {
    idx / N
}
pub fn col(idx: usize) -> usize {
    idx % N
}
pub fn box_of(idx: usize) -> usize {
    (row(idx) / 3) * 3 + col(idx) / 3
}

pub fn row_cells(r: usize) -> impl Iterator<Item = usize> + Clone {
    (0..N).map(move |c| r * N + c)
}
pub fn col_cells(c: usize) -> impl Iterator<Item = usize> + Clone {
    (0..N).map(move |r| r * N + c)
}
pub fn box_cells(b: usize) -> impl Iterator<Item = usize> + Clone {
    let br = (b / 3) * 3;
    let bc = (b % 3) * 3;
    (0..N).map(move |k| (br + k / 3) * N + bc + k % 3)
}

pub fn bit(digit: u8) -> u16 {
    1u16 << (digit - 1)
}

fn shuffled<T>(mut v: Vec<T>) -> Vec<T> {
    for i in (1..v.len()).rev() {
        let j = (macroquad::rand::rand() as usize) % (i + 1);
        v.swap(i, j);
    }
    v
}

fn used_mask(grid: &[u8; CELLS], idx: usize) -> u16 {
    let mut used = 0u16;
    for cell in row_cells(row(idx))
        .chain(col_cells(col(idx)))
        .chain(box_cells(box_of(idx)))
    {
        if grid[cell] != 0 {
            used |= bit(grid[cell]);
        }
    }
    used
}

pub fn compute_candidates(grid: &[u8; CELLS]) -> [u16; CELLS] {
    let mut cands = [0u16; CELLS];
    for idx in 0..CELLS {
        if grid[idx] == 0 {
            cands[idx] = FULL_MASK & !used_mask(grid, idx);
        }
    }
    cands
}

/// Randomized recursive backtracking fill — fast in practice for a full 9x9 grid since
/// shuffling each cell's candidate order before descending rarely needs deep backtracking.
fn fill(grid: &mut [u8; CELLS], idx: usize) -> bool {
    if idx == CELLS {
        return true;
    }
    let mask = FULL_MASK & !used_mask(grid, idx);
    for d in shuffled((1..=9u8).filter(|&d| mask & bit(d) != 0).collect()) {
        grid[idx] = d;
        if fill(grid, idx + 1) {
            return true;
        }
        grid[idx] = 0;
    }
    false
}

pub fn generate_solved_grid() -> [u8; CELLS] {
    let mut grid = [0u8; CELLS];
    fill(&mut grid, 0);
    grid
}

/// Counts solutions of `grid` up to `limit` (mutates and restores `grid` via backtracking).
/// Picks the emptiest cell first (MRV) each step, which keeps this fast enough to call
/// once per candidate clue removal during puzzle carving.
fn solve_count(grid: &mut [u8; CELLS], limit: usize) -> usize {
    let cands = compute_candidates(grid);
    let mut best: Option<(usize, u16, u32)> = None;
    let mut any_empty = false;
    for idx in 0..CELLS {
        if grid[idx] == 0 {
            any_empty = true;
            let cnt = cands[idx].count_ones();
            if cnt == 0 {
                return 0;
            }
            if best.is_none_or(|(_, _, bc)| cnt < bc) {
                best = Some((idx, cands[idx], cnt));
            }
        }
    }
    let Some((idx, mask, _)) = best else {
        return if any_empty { 0 } else { 1 };
    };
    let mut total = 0;
    for d in 1..=9u8 {
        if mask & bit(d) == 0 {
            continue;
        }
        grid[idx] = d;
        total += solve_count(grid, limit - total);
        grid[idx] = 0;
        if total >= limit {
            break;
        }
    }
    total
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    /// Carving stops once this many clues remain (or removal attempts run out first).
    /// Fewer clues -> more empty cells -> the solver leans harder on locked-candidate
    /// eliminations and guesses instead of plain naked/hidden singles.
    fn min_clues(self) -> usize {
        match self {
            Difficulty::Easy => 40,
            Difficulty::Medium => 32,
            Difficulty::Hard => 26,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        }
    }
}

/// Randomly removes clues from a fully solved grid one at a time, keeping a removal only
/// when the puzzle still has a unique solution (checked via `solve_count` capped at 2).
fn carve(solution: &[u8; CELLS], difficulty: Difficulty) -> [u8; CELLS] {
    let mut grid = *solution;
    let mut clues = CELLS;
    let min_clues = difficulty.min_clues();
    for idx in shuffled((0..CELLS).collect()) {
        if clues <= min_clues {
            break;
        }
        let saved = grid[idx];
        grid[idx] = 0;
        let mut test = grid;
        if solve_count(&mut test, 2) == 1 {
            clues -= 1;
        } else {
            grid[idx] = saved;
        }
    }
    grid
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Technique {
    NakedSingle,
    HiddenSingle,
    LockedCandidate,
    Guess,
}

/// `Place` fills a cell (the sure/forced digit, or the solver's best guess). `Narrow`
/// eliminates one candidate from one cell without filling anything — the visible result of
/// a locked-candidate deduction, shown so the viewer can see *why* a later single fires.
#[derive(Clone, Copy, Debug)]
pub enum Move {
    Place {
        idx: usize,
        digit: u8,
        technique: Technique,
    },
    Narrow {
        idx: usize,
        digit: u8,
        technique: Technique,
    },
}

#[derive(Clone, Copy, PartialEq)]
pub enum Phase {
    Playing,
    Solved,
}

#[derive(Clone)]
pub struct Game {
    pub grid: [u8; CELLS],
    pub given: [bool; CELLS],
    /// The full solved grid, known up front since it's generated before carving. Only the
    /// solver's `Guess` fallback reads this — logical techniques never consult it.
    pub solution: [u8; CELLS],
    pub candidates: [u16; CELLS],
    /// How each non-given cell got its digit, for the renderer to color-code moves by
    /// technique. `None` for empty cells and givens.
    pub filled_by: [Option<Technique>; CELLS],
    pub difficulty: Difficulty,
    pub generation: u32,
    pub moves: u32,
    pub phase: Phase,
}

impl Game {
    pub fn new(difficulty: Difficulty, generation: u32) -> Self {
        let solution = generate_solved_grid();
        let grid = carve(&solution, difficulty);
        let given = grid.map(|v| v != 0);
        let candidates = compute_candidates(&grid);
        Self {
            grid,
            given,
            solution,
            candidates,
            filled_by: [None; CELLS],
            difficulty,
            generation,
            moves: 0,
            phase: Phase::Playing,
        }
    }

    pub fn clue_count(&self) -> usize {
        self.given.iter().filter(|&&g| g).count()
    }

    pub fn apply(&mut self, m: Move) {
        match m {
            Move::Place {
                idx,
                digit,
                technique,
            } => {
                self.grid[idx] = digit;
                self.candidates[idx] = 0;
                self.filled_by[idx] = Some(technique);
                let d = bit(digit);
                for cell in row_cells(row(idx))
                    .chain(col_cells(col(idx)))
                    .chain(box_cells(box_of(idx)))
                {
                    self.candidates[cell] &= !d;
                }
            }
            Move::Narrow { idx, digit, .. } => {
                self.candidates[idx] &= !bit(digit);
            }
        }
        self.moves += 1;
        if self.grid.iter().all(|&v| v != 0) {
            self.phase = Phase::Solved;
        }
    }
}
