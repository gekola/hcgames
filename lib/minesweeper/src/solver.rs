use crate::board::{Board, CellState};

pub struct Constraint {
    pub cells: Vec<usize>,
    pub mine_count: i32,
}

pub enum Action {
    Flag(usize),
    Open(usize),
    Guess(usize),
}

fn build_constraints(board: &Board) -> Vec<Constraint> {
    let mut out = Vec::new();
    for idx in 0..board.cells.len() {
        let cell = &board.cells[idx];
        if cell.state != CellState::Revealed || cell.adj_mines == 0 {
            continue;
        }
        let neighbors = board.neighbors(idx);
        let hidden: Vec<usize> = neighbors
            .iter()
            .copied()
            .filter(|&n| board.cells[n].state == CellState::Hidden)
            .collect();
        let flagged = neighbors
            .iter()
            .filter(|&&n| board.cells[n].state == CellState::Flagged)
            .count() as i32;
        let remaining = cell.adj_mines as i32 - flagged;
        if !hidden.is_empty() && remaining >= 0 {
            out.push(Constraint {
                cells: hidden,
                mine_count: remaining,
            });
        }
    }
    out
}

fn reduce(constraints: &mut Vec<Constraint>) {
    let orig_len = constraints.len();
    for i in 0..orig_len {
        for j in 0..orig_len {
            if i == j {
                continue;
            }
            // Check if constraints[i] ⊆ constraints[j]
            if constraints[i]
                .cells
                .iter()
                .all(|c| constraints[j].cells.contains(c))
                && constraints[i].cells.len() < constraints[j].cells.len()
            {
                let new_cells: Vec<usize> = constraints[j]
                    .cells
                    .iter()
                    .copied()
                    .filter(|c| !constraints[i].cells.contains(c))
                    .collect();
                let new_count = constraints[j].mine_count - constraints[i].mine_count;
                if new_count >= 0 && new_count <= new_cells.len() as i32 && !new_cells.is_empty() {
                    constraints.push(Constraint {
                        cells: new_cells,
                        mine_count: new_count,
                    });
                }
            }
        }
    }
}

pub fn next_action(board: &Board) -> Action {
    let mut constraints = build_constraints(board);
    reduce(&mut constraints);

    // Certain mine: mine_count == hidden count
    for c in &constraints {
        if !c.cells.is_empty() && c.mine_count == c.cells.len() as i32 {
            return Action::Flag(c.cells[0]);
        }
    }

    // Certain safe: mine_count == 0
    for c in &constraints {
        if c.mine_count == 0
            && let Some(&idx) = c.cells.first()
        {
            return Action::Open(idx);
        }
    }

    // Guess: lowest mine_prob hidden cell
    let best = (0..board.cells.len())
        .filter(|&i| board.cells[i].state == CellState::Hidden)
        .min_by(|&a, &b| {
            board.cells[a]
                .mine_prob
                .partial_cmp(&board.cells[b].mine_prob)
                .unwrap()
        });

    match best {
        Some(idx) => Action::Guess(idx),
        None => Action::Open(0),
    }
}

pub fn update_probs(board: &mut Board) {
    let total_hidden = board
        .cells
        .iter()
        .filter(|c| c.state == CellState::Hidden)
        .count() as f32;
    let total_flagged = board
        .cells
        .iter()
        .filter(|c| c.state == CellState::Flagged)
        .count() as i32;
    let remaining_mines = (board.mine_count - total_flagged).max(0) as f32;
    let global = if total_hidden > 0.0 {
        (remaining_mines / total_hidden).clamp(0.0, 1.0)
    } else {
        0.0
    };

    for cell in &mut board.cells {
        if cell.state == CellState::Hidden {
            cell.mine_prob = global;
        }
    }

    let mut constraints = build_constraints(board);
    reduce(&mut constraints);

    for c in &constraints {
        if c.cells.is_empty() {
            continue;
        }
        if c.mine_count == c.cells.len() as i32 {
            for &idx in &c.cells {
                board.cells[idx].mine_prob = 1.0;
            }
        } else if c.mine_count == 0 {
            for &idx in &c.cells {
                board.cells[idx].mine_prob = 0.0;
            }
        } else {
            let local = c.mine_count as f32 / c.cells.len() as f32;
            for &idx in &c.cells {
                if board.cells[idx].mine_prob > 0.0 && board.cells[idx].mine_prob < 1.0 {
                    board.cells[idx].mine_prob = board.cells[idx].mine_prob.max(local);
                }
            }
        }
    }
}
