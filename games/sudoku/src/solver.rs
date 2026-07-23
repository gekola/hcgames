use crate::game::{
    CELLS, Game, Move, Technique, bit, box_cells, box_of, col, col_cells, row, row_cells,
};
use std::collections::HashSet;

/// Plays sudoku the way a human with a pencil does: fill whatever's forced (naked/hidden
/// single), else narrow candidates via a locked-candidate deduction, else — only when logic
/// alone can't move the puzzle forward — take a best-guess cell and place its known-correct
/// digit (see `Technique::Guess` / `Game::solution`). No search/backtracking is needed here
/// since the puzzle's unique solution is already known from generation, so a "guess" never
/// has to be undone; this is why sudoku doesn't use `lib/beam_solver` like Klondike/Spider —
/// there's nothing to score or search among, just a fixed technique escalation order.
pub struct Solver;

impl Solver {
    pub fn new() -> Self {
        Solver
    }

    pub fn choose_move(&mut self, game: &Game) -> Option<Move> {
        find_naked_single(game)
            .or_else(|| find_hidden_single(game))
            .or_else(|| find_locked_candidate(game))
            .or_else(|| find_guess(game))
    }
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

fn find_naked_single(game: &Game) -> Option<Move> {
    for idx in 0..CELLS {
        if game.grid[idx] == 0 && game.candidates[idx].count_ones() == 1 {
            let digit = game.candidates[idx].trailing_zeros() as u8 + 1;
            return Some(Move::Place {
                idx,
                digit,
                technique: Technique::NakedSingle,
            });
        }
    }
    None
}

fn find_hidden_single(game: &Game) -> Option<Move> {
    for r in 0..9 {
        if let Some(m) = hidden_single_in(game, row_cells(r)) {
            return Some(m);
        }
    }
    for c in 0..9 {
        if let Some(m) = hidden_single_in(game, col_cells(c)) {
            return Some(m);
        }
    }
    for b in 0..9 {
        if let Some(m) = hidden_single_in(game, box_cells(b)) {
            return Some(m);
        }
    }
    None
}

fn hidden_single_in(game: &Game, cells: impl Iterator<Item = usize> + Clone) -> Option<Move> {
    for d in 1..=9u8 {
        let mask = bit(d);
        let mut found = None;
        let mut count = 0;
        for idx in cells.clone() {
            if game.grid[idx] == 0 && game.candidates[idx] & mask != 0 {
                count += 1;
                found = Some(idx);
                if count > 1 {
                    break;
                }
            }
        }
        if count == 1 {
            return Some(Move::Place {
                idx: found.unwrap(),
                digit: d,
                technique: Technique::HiddenSingle,
            });
        }
    }
    None
}

/// Pointing (box -> row/col) and box-line reduction (row/col -> box): when a digit's
/// remaining candidate cells within one unit all fall inside a second, smaller unit, it
/// can be eliminated from the rest of that second unit. One elimination per call, so each
/// deduction is individually visible rather than applying a whole batch at once.
fn find_locked_candidate(game: &Game) -> Option<Move> {
    for b in 0..9 {
        for d in 1..=9u8 {
            let mask = bit(d);
            let cells: Vec<usize> = box_cells(b)
                .filter(|&idx| game.grid[idx] == 0 && game.candidates[idx] & mask != 0)
                .collect();
            if cells.len() < 2 {
                continue;
            }
            let rows: HashSet<usize> = cells.iter().map(|&idx| row(idx)).collect();
            if rows.len() == 1 {
                let r = *rows.iter().next().unwrap();
                if let Some(idx) = row_cells(r).find(|&idx| {
                    box_of(idx) != b && game.grid[idx] == 0 && game.candidates[idx] & mask != 0
                }) {
                    return Some(Move::Narrow {
                        idx,
                        digit: d,
                        technique: Technique::LockedCandidate,
                    });
                }
            }
            let cols: HashSet<usize> = cells.iter().map(|&idx| col(idx)).collect();
            if cols.len() == 1 {
                let c = *cols.iter().next().unwrap();
                if let Some(idx) = col_cells(c).find(|&idx| {
                    box_of(idx) != b && game.grid[idx] == 0 && game.candidates[idx] & mask != 0
                }) {
                    return Some(Move::Narrow {
                        idx,
                        digit: d,
                        technique: Technique::LockedCandidate,
                    });
                }
            }
        }
    }

    for r in 0..9 {
        if let Some(m) = box_line_reduction(game, row_cells(r), |idx| row(idx) != r) {
            return Some(m);
        }
    }
    for c in 0..9 {
        if let Some(m) = box_line_reduction(game, col_cells(c), |idx| col(idx) != c) {
            return Some(m);
        }
    }
    None
}

fn box_line_reduction(
    game: &Game,
    line: impl Iterator<Item = usize> + Clone,
    outside_line: impl Fn(usize) -> bool,
) -> Option<Move> {
    for d in 1..=9u8 {
        let mask = bit(d);
        let cells: Vec<usize> = line
            .clone()
            .filter(|&idx| game.grid[idx] == 0 && game.candidates[idx] & mask != 0)
            .collect();
        if cells.len() < 2 {
            continue;
        }
        let boxes: HashSet<usize> = cells.iter().map(|&idx| box_of(idx)).collect();
        if boxes.len() == 1 {
            let b = *boxes.iter().next().unwrap();
            if let Some(idx) = box_cells(b).find(|&idx| {
                outside_line(idx) && game.grid[idx] == 0 && game.candidates[idx] & mask != 0
            }) {
                return Some(Move::Narrow {
                    idx,
                    digit: d,
                    technique: Technique::LockedCandidate,
                });
            }
        }
    }
    None
}

/// Last resort: pick the emptiest cell (fewest remaining candidates) and place its known
/// digit from `game.solution`. Only reached when no logical technique makes progress.
fn find_guess(game: &Game) -> Option<Move> {
    let mut best: Option<(usize, u32)> = None;
    for idx in 0..CELLS {
        if game.grid[idx] == 0 {
            let cnt = game.candidates[idx].count_ones();
            if best.is_none_or(|(_, bc)| cnt < bc) {
                best = Some((idx, cnt));
            }
        }
    }
    let (idx, _) = best?;
    Some(Move::Place {
        idx,
        digit: game.solution[idx],
        technique: Technique::Guess,
    })
}
