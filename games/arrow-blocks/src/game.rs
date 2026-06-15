use std::collections::HashSet;

use crate::{Dir, FIELD_H, FIELD_W, VIEW_COLS, VIEW_ROWS};

use crate::puzzle::shuffle;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockState {
    Idle,
    Considered,
    Exiting,
    ReturnFwd,
    ReturnBack,
    Gone,
}

pub struct Block {
    pub col: i32,
    pub row: i32,
    pub dir: Dir,
    pub state: BlockState,
    pub anim_t: f32,
    pub anim_max_dist: f32,
    pub blocking_dist: f32, // Exiting: stop counting as occupied after traveling this far
}

impl Block {
    pub fn vis_offset(&self, cell: f32) -> (f32, f32) {
        let (dc, dr) = self.dir.delta();
        let frac = match self.state {
            BlockState::Exiting   => self.anim_t,
            BlockState::ReturnFwd => self.anim_t / 0.5,
            BlockState::ReturnBack => 1.0 - (self.anim_t - 0.5) / 0.5,
            _ => 0.0,
        };
        let dist = self.anim_max_dist * frac * cell;
        (dc as f32 * dist, dr as f32 * dist)
    }
}

// Activating is gone — blocks animate on their own; AI cycles independently.
#[derive(Clone, Copy, Debug)]
pub enum Phase {
    Selecting,
    Considering { idx: usize, until: f64 }, // brief dim-highlight before fire
    Pause       { until: f64 },             // short gap after firing
    Done        { since: f64 },
}

pub struct Game {
    pub blocks: Vec<Block>,
    pub cam_x: f32,
    pub cam_y: f32,
    pub cam_tx: f32,
    pub cam_ty: f32,
    pub phase: Phase,
    pub level: usize,
}

impl Game {
    pub fn new(level: usize) -> Self {
        let assignments = crate::puzzle::generate(level, FIELD_W, FIELD_H);
        let blocks = assignments
            .into_iter()
            .map(|((col, row), dir)| Block {
                col, row, dir,
                state: BlockState::Idle,
                anim_t: 0.0,
                anim_max_dist: 0.0,
                blocking_dist: 0.0,
            })
            .collect();
        let cx = (FIELD_W / 2) as f32;
        let cy = (FIELD_H / 2) as f32;
        Game {
            blocks,
            cam_x: cx, cam_y: cy,
            cam_tx: cx, cam_ty: cy,
            phase: Phase::Selecting,
            level,
        }
    }

    pub fn tick(&mut self, dt: f32, now: f64) {
        // Camera smooth-follow
        let k = 1.0 - (-3.5f32 * dt).exp();
        self.cam_x += (self.cam_tx - self.cam_x) * k;
        self.cam_y += (self.cam_ty - self.cam_y) * k;

        // Advance ALL block animations independently each frame
        for block in &mut self.blocks {
            let dist = block.anim_max_dist.max(1.0);
            match block.state {
                BlockState::Exiting => {
                    block.anim_t += dt * 12.0 / dist;
                    if block.anim_t >= 1.0 { block.state = BlockState::Gone; }
                }
                BlockState::ReturnFwd => {
                    block.anim_t += dt * 12.0 / dist;
                    if block.anim_t >= 1.0 {
                        block.state = BlockState::ReturnBack;
                        block.anim_t = 0.5;
                    }
                }
                BlockState::ReturnBack => {
                    block.anim_t += dt * 12.0 / dist;
                    if block.anim_t >= 1.0 { block.state = BlockState::Idle; }
                }
                _ => {}
            }
        }

        // AI state machine — no longer waits for animation to complete
        match self.phase {
            Phase::Selecting => {
                match self.find_target() {
                    Some(idx) => {
                        self.blocks[idx].state = BlockState::Considered;
                        // Only re-center camera if block is outside the visible area
                        if !self.block_in_view(idx) {
                            self.cam_tx = self.blocks[idx].col as f32;
                            self.cam_ty = self.blocks[idx].row as f32;
                        }
                        self.phase = Phase::Considering { idx, until: now + 0.35 };
                    }
                    None => {
                        if self.remaining() == 0 {
                            self.phase = Phase::Done { since: now };
                        } else {
                            // Some blocks still animating out; check again soon
                            self.phase = Phase::Pause { until: now + 0.05 };
                        }
                    }
                }
            }

            Phase::Considering { idx, until } => {
                if now >= until {
                    self.activate(idx);
                    self.phase = Phase::Pause { until: now + 0.15 };
                }
            }

            Phase::Pause { until } => {
                if now >= until {
                    self.phase = Phase::Selecting;
                }
            }

            Phase::Done { .. } => {}
        }
    }

    // True when block is well within the viewport — no need to pan.
    fn block_in_view(&self, idx: usize) -> bool {
        let b = &self.blocks[idx];
        let dx = (b.col as f32 - self.cam_x).abs();
        let dy = (b.row as f32 - self.cam_y).abs();
        dx < VIEW_COLS as f32 * 0.45 && dy < VIEW_ROWS as f32 * 0.45
    }

    fn activate(&mut self, idx: usize) {
        if self.is_path_clear(idx) {
            let anim_dist    = self.cells_to_edge(idx) as f32;
            let blocking     = self.cells_to_exit_figure(idx) as f32;
            let b = &mut self.blocks[idx];
            b.state = BlockState::Exiting;
            b.anim_t = 0.0;
            b.anim_max_dist = anim_dist;
            b.blocking_dist = blocking;
        } else {
            let dist = self.cells_to_blocker(idx).max(1) as f32;
            let b = &mut self.blocks[idx];
            b.state = BlockState::ReturnFwd;
            b.anim_t = 0.0;
            b.anim_max_dist = dist;
        }
    }

    fn figure_bounds(&self) -> (i32, i32, i32, i32) {
        let mut min_c = i32::MAX; let mut max_c = i32::MIN;
        let mut min_r = i32::MAX; let mut max_r = i32::MIN;
        for b in &self.blocks {
            if b.state != BlockState::Gone {
                min_c = min_c.min(b.col); max_c = max_c.max(b.col);
                min_r = min_r.min(b.row); max_r = max_r.max(b.row);
            }
        }
        (min_c, max_c, min_r, max_r)
    }

    fn cells_to_exit_figure(&self, idx: usize) -> i32 {
        let b = &self.blocks[idx];
        let (min_c, max_c, min_r, max_r) = self.figure_bounds();
        let pad = 3;
        match b.dir {
            Dir::Up    => (b.row - min_r + pad).max(1),
            Dir::Down  => (max_r - b.row + pad).max(1),
            Dir::Left  => (b.col - min_c + pad).max(1),
            Dir::Right => (max_c - b.col + pad).max(1),
        }
    }

    fn cells_to_edge(&self, idx: usize) -> i32 {
        let b = &self.blocks[idx];
        match b.dir {
            Dir::Up    => b.row + 1,
            Dir::Down  => FIELD_H - b.row,
            Dir::Left  => b.col + 1,
            Dir::Right => FIELD_W - b.col,
        }
    }

    pub fn is_path_clear(&self, idx: usize) -> bool {
        let b = &self.blocks[idx];
        let (dc, dr) = b.dir.delta();
        let (mut c, mut r) = (b.col + dc, b.row + dr);
        while c >= 0 && r >= 0 && c < FIELD_W && r < FIELD_H {
            if self.occupied(c, r, idx) { return false; }
            c += dc; r += dr;
        }
        true
    }

    fn cells_to_blocker(&self, idx: usize) -> i32 {
        let b = &self.blocks[idx];
        let (dc, dr) = b.dir.delta();
        let (mut c, mut r) = (b.col + dc, b.row + dr);
        let mut dist = 1;
        while c >= 0 && r >= 0 && c < FIELD_W && r < FIELD_H {
            if self.occupied(c, r, idx) { return dist; }
            c += dc; r += dr; dist += 1;
        }
        dist
    }

    fn occupied(&self, col: i32, row: i32, exclude: usize) -> bool {
        self.blocks.iter().enumerate().any(|(i, b)| {
            i != exclude
                && b.state != BlockState::Gone
                && b.col == col
                && b.row == row
                && !(b.state == BlockState::Exiting
                    && b.anim_t * b.anim_max_dist >= b.blocking_dist)
        })
    }

    fn blocker_in_dir(&self, idx: usize) -> Option<usize> {
        let b = &self.blocks[idx];
        let (dc, dr) = b.dir.delta();
        let (mut c, mut r) = (b.col + dc, b.row + dr);
        while c >= 0 && r >= 0 && c < FIELD_W && r < FIELD_H {
            if let Some(i) = self.blocks.iter().position(|ob| {
                ob.state != BlockState::Gone && ob.col == c && ob.row == r
            }) {
                if i != idx { return Some(i); }
            }
            c += dc; r += dr;
        }
        None
    }

    fn find_target(&self) -> Option<usize> {
        let mut idxs: Vec<usize> = (0..self.blocks.len())
            .filter(|&i| matches!(self.blocks[i].state, BlockState::Idle | BlockState::Considered))
            .collect();
        shuffle(&mut idxs);

        for &start in &idxs {
            let mut visited = HashSet::new();
            let mut cur = start;
            loop {
                if !visited.insert(cur) { break; }
                // Never target a block that is already animating
                if !matches!(self.blocks[cur].state, BlockState::Idle | BlockState::Considered) {
                    break;
                }
                if self.is_path_clear(cur) { return Some(cur); }
                match self.blocker_in_dir(cur) {
                    Some(b) => cur = b,
                    None    => return Some(cur),
                }
            }
        }
        None
    }

    pub fn remaining(&self) -> usize {
        self.blocks.iter().filter(|b| b.state != BlockState::Gone).count()
    }
}
