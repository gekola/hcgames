use macroquad::rand;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GridKind {
    Square,
    Hex,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Hidden,
    Flagged,
    Revealed,
}

pub struct Cell {
    pub is_mine: bool,
    pub state: CellState,
    pub adj_mines: u8,
    pub mine_prob: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Phase {
    FirstClick,
    Playing,
    GameOver(f64),
    Won(f64),
}

pub struct Board {
    pub kind: GridKind,
    pub cols: i32,
    pub rows: i32,
    pub cells: Vec<Cell>,
    pub mine_count: i32,
    pub phase: Phase,
    pub last_action: Option<usize>,
    pub last_was_flag: bool,
    pub hit_mine: Option<usize>,
}

impl Board {
    pub fn new(kind: GridKind) -> Self {
        let (cols, rows, mine_count) = match kind {
            GridKind::Square => (26, 20, 83),
            GridKind::Hex => (18, 15, 43),
        };
        let init_prob = mine_count as f32 / (cols * rows) as f32;
        let cells = (0..cols * rows)
            .map(|_| Cell {
                is_mine: false,
                state: CellState::Hidden,
                adj_mines: 0,
                mine_prob: init_prob,
            })
            .collect();
        Self {
            kind,
            cols,
            rows,
            cells,
            mine_count,
            phase: Phase::FirstClick,
            last_action: None,
            last_was_flag: false,
            hit_mine: None,
        }
    }

    pub fn neighbors(&self, idx: usize) -> Vec<usize> {
        let col = (idx as i32) % self.cols;
        let row = (idx as i32) / self.cols;

        let offsets: &[(i32, i32)] = match self.kind {
            GridKind::Square => &[
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ],
            GridKind::Hex => {
                // flat-top hexagons, odd-column shifted down
                if col % 2 == 0 {
                    &[(1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (0, 1)]
                } else {
                    &[(1, 0), (1, 1), (0, 1), (-1, 0), (-1, 1), (0, -1)]
                }
            }
        };

        offsets
            .iter()
            .filter_map(|&(dc, dr)| {
                let nc = col + dc;
                let nr = row + dr;
                if nc >= 0 && nc < self.cols && nr >= 0 && nr < self.rows {
                    Some((nr * self.cols + nc) as usize)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn place_mines(&mut self, exclude: usize) {
        let excluded: std::collections::HashSet<usize> = {
            let mut s = std::collections::HashSet::new();
            s.insert(exclude);
            for n in self.neighbors(exclude) {
                s.insert(n);
            }
            s
        };

        let mut pool: Vec<usize> = (0..self.cells.len())
            .filter(|i| !excluded.contains(i))
            .collect();
        let n = (self.mine_count as usize).min(pool.len());
        for i in 0..n {
            let j = i + rand::gen_range(0, pool.len() - i);
            pool.swap(i, j);
            self.cells[pool[i]].is_mine = true;
        }

        for idx in 0..self.cells.len() {
            let count = self
                .neighbors(idx)
                .into_iter()
                .filter(|&n| self.cells[n].is_mine)
                .count();
            self.cells[idx].adj_mines = count as u8;
        }
    }

    pub fn reveal(&mut self, start: usize) {
        if self.cells[start].state != CellState::Hidden {
            return;
        }

        let mut queue = vec![start];
        while let Some(idx) = queue.pop() {
            if self.cells[idx].state != CellState::Hidden {
                continue;
            }
            self.cells[idx].state = CellState::Revealed;

            if self.cells[idx].is_mine {
                self.hit_mine = Some(idx);
                self.phase = Phase::GameOver(macroquad::miniquad::date::now());
                for c in &mut self.cells {
                    if c.is_mine {
                        c.state = CellState::Revealed;
                    }
                }
                return;
            }

            if self.cells[idx].adj_mines == 0 {
                for n in self.neighbors(idx) {
                    if self.cells[n].state == CellState::Hidden {
                        queue.push(n);
                    }
                }
            }
        }

        let unrevealed = self
            .cells
            .iter()
            .filter(|c| !c.is_mine && c.state != CellState::Revealed)
            .count();
        if unrevealed == 0 {
            self.phase = Phase::Won(macroquad::miniquad::date::now());
        }
    }

    pub fn flag(&mut self, idx: usize) {
        if self.cells[idx].state == CellState::Hidden {
            self.cells[idx].state = CellState::Flagged;
        }
    }

    pub fn remaining_mines(&self) -> i32 {
        let flagged = self
            .cells
            .iter()
            .filter(|c| c.state == CellState::Flagged)
            .count() as i32;
        self.mine_count - flagged
    }
}
