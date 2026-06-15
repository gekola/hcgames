use macroquad::prelude::*;
use std::collections::VecDeque;
use crate::{Pt, COLS, ROWS, GRID, DIRS, BLOCK_SENTINEL};
use crate::blocks::generate_blocks;

pub struct Game {
    pub body: VecDeque<Pt>,
    pub dir: (i32, i32),
    pub food: Pt,
    pub score: u32,
    pub generation: u32,
    pub blocks: [bool; GRID],
    pub ticks_hungry: u32,
}

impl Game {
    pub fn new(generation: u32) -> Self {
        let blocks = generate_blocks();
        let head = Pt { x: COLS / 2, y: ROWS / 2 };
        let body = VecDeque::from([head]);
        let food = spawn_food(&body, &blocks);
        Self { body, dir: (1, 0), food, score: 0, generation, blocks, ticks_hungry: 0 }
    }

    pub fn tick(&mut self) -> bool {
        let dir = self.choose_dir();
        self.dir = dir;
        let next = self.body[0].shifted(dir.0, dir.1);
        if !next.in_bounds() || self.body.contains(&next) || self.blocks[next.idx()] {
            return false;
        }
        self.body.push_front(next);
        if next == self.food {
            self.score += 1;
            self.ticks_hungry = 0;
            self.food = spawn_food(&self.body, &self.blocks);
        } else {
            self.ticks_hungry += 1;
            self.body.pop_back();
        }
        true
    }

    fn choose_dir(&self) -> (i32, i32) {
        let head = self.body[0];
        let n = self.body.len();
        let tail = *self.body.back().unwrap();
        let bg = self.body_grid();

        if let Some((path_len, dir)) = self.bfs_to(head, self.food, &bg) {
            let still_blocked = n.saturating_sub(path_len.saturating_sub(1));
            let desperate = self.ticks_hungry > (n as u32).saturating_mul(2);
            let next = head.shifted(dir.0, dir.1);
            if desperate
                || (self.time_flood(self.food, still_blocked, &bg) > n
                    && self.bfs_to(self.food, tail, &bg).is_some()
                    && self.bfs_to(next, tail, &bg).is_some())
            {
                return dir;
            }
        }

        // Safe-greedy: among moves with flood >= n, pick min distance to food.
        // Replaces blind tail-chase that loops forever when food is inside a body cave.
        self.safe_greedy_dir(&bg)
    }

    // BFS from food (blocks-only walls, body transparent) → per-cell distance map.
    fn food_dist_map(&self, bg: &[u16; GRID]) -> [u16; GRID] {
        let mut dist = [u16::MAX; GRID];
        let mut queue = [Pt { x: 0, y: 0 }; GRID];
        let (mut qh, mut qt) = (0, 0);
        dist[self.food.idx()] = 0;
        queue[qt] = self.food; qt += 1;
        while qh < qt {
            let cur = queue[qh]; qh += 1;
            let d = dist[cur.idx()];
            for (dx, dy) in DIRS {
                let nb = cur.shifted(dx, dy);
                if !nb.in_bounds() { continue; }
                let ni = nb.idx();
                if dist[ni] != u16::MAX || bg[ni] == BLOCK_SENTINEL { continue; }
                dist[ni] = d + 1;
                queue[qt] = nb; qt += 1;
            }
        }
        dist
    }

    // Tail-chase as safe baseline; other flood-safe dirs can improve if closer to food.
    fn safe_greedy_dir(&self, bg: &[u16; GRID]) -> (i32, i32) {
        let head = self.body[0];
        let n = self.body.len();
        let tail = *self.body.back().unwrap();
        let fdist = self.food_dist_map(bg);
        let tail_dir = self.bfs_to(head, tail, bg).map(|(_, d)| d);

        // Only use tail_dir as baseline if it also has enough reachable space; otherwise a
        // flood-safe direction farther from food must be allowed to win over a pocket tail-path.
        let tail_dir_safe = tail_dir.filter(|&(dx, dy)| {
            let nb = head.shifted(dx, dy);
            self.time_flood(nb, n.saturating_sub(1), bg) >= n
        });

        let mut best = tail_dir_safe;
        let mut best_dist = best
            .map(|(dx, dy)| fdist[head.shifted(dx, dy).idx()])
            .unwrap_or(u16::MAX);

        for (dx, dy) in DIRS {
            if n > 1 && (dx, dy) == (-self.dir.0, -self.dir.1) { continue; }
            if tail_dir == Some((dx, dy)) { continue; }
            let nb = head.shifted(dx, dy);
            if !nb.in_bounds() { continue; }
            if bg[nb.idx()] != u16::MAX { continue; }
            if self.time_flood(nb, n.saturating_sub(1), bg) < n { continue; }
            // Reject moves that seal the tail off (e.g. filling the last cell of a row).
            if self.bfs_to(nb, tail, bg).is_none() { continue; }
            let d = fdist[nb.idx()];
            if d < best_dist { best_dist = d; best = Some((dx, dy)); }
        }

        // Fall back to tail_dir even if flood is low, then max_space as last resort.
        best.or(tail_dir).unwrap_or_else(|| self.max_space_dir(bg))
    }

    // body_grid[cell] = body index, BLOCK_SENTINEL if block, u16::MAX if empty
    fn body_grid(&self) -> [u16; GRID] {
        let mut bg = [u16::MAX; GRID];
        for i in 0..GRID {
            if self.blocks[i] { bg[i] = BLOCK_SENTINEL; }
        }
        for (j, &seg) in self.body.iter().enumerate() {
            bg[seg.idx()] = j as u16;
        }
        bg
    }

    fn bfs_to(&self, from: Pt, to: Pt, bg: &[u16; GRID]) -> Option<(usize, (i32, i32))> {
        if from == to { return None; }
        let n = self.body.len();

        let mut dist   = [u16::MAX; GRID];
        let mut parent = [u16::MAX; GRID];
        let mut queue  = [Pt { x: 0, y: 0 }; GRID];
        let (mut qh, mut qt) = (0, 0);

        dist[from.idx()] = 0;
        parent[from.idx()] = from.idx() as u16;
        queue[qt] = from; qt += 1;

        while qh < qt {
            let cur = queue[qh]; qh += 1;
            let t = dist[cur.idx()] as usize;
            for (dx, dy) in DIRS {
                let nb = cur.shifted(dx, dy);
                if !nb.in_bounds() { continue; }
                let ni = nb.idx();
                if dist[ni] != u16::MAX { continue; }
                let t1 = t + 1;
                let bj = bg[ni];
                if bj == BLOCK_SENTINEL { continue; }
                if bj != u16::MAX && n.saturating_sub(bj as usize) >= t1 { continue; }
                dist[ni] = t1 as u16;
                parent[ni] = cur.idx() as u16;
                if nb == to {
                    let mut c = nb;
                    loop {
                        let pi = parent[c.idx()] as usize;
                        let p = Pt { x: (pi % COLS as usize) as i32, y: (pi / COLS as usize) as i32 };
                        if p == from { return Some((t1, (c.x - p.x, c.y - p.y))); }
                        c = p;
                    }
                }
                queue[qt] = nb; qt += 1;
            }
        }
        None
    }

    fn time_flood(&self, start: Pt, still_blocked: usize, bg: &[u16; GRID]) -> usize {
        let mut visited = [false; GRID];
        let mut queue   = [Pt { x: 0, y: 0 }; GRID];
        let (mut qh, mut qt) = (0, 0);
        visited[start.idx()] = true;
        queue[qt] = start; qt += 1;
        let mut count = 1usize;

        while qh < qt {
            let cur = queue[qh]; qh += 1;
            for (dx, dy) in DIRS {
                let nb = cur.shifted(dx, dy);
                if !nb.in_bounds() { continue; }
                let ni = nb.idx();
                if visited[ni] { continue; }
                let bj = bg[ni];
                if bj == BLOCK_SENTINEL { continue; }
                if bj != u16::MAX && (bj as usize) < still_blocked { continue; }
                visited[ni] = true;
                queue[qt] = nb; qt += 1;
                count += 1;
            }
        }
        count
    }

    fn max_space_dir(&self, bg: &[u16; GRID]) -> (i32, i32) {
        let head = self.body[0];
        let n = self.body.len();
        let mut best = self.dir;
        let mut best_n = 0;

        for (dx, dy) in DIRS {
            if n > 1 && (dx, dy) == (-self.dir.0, -self.dir.1) { continue; }
            let nb = head.shifted(dx, dy);
            if !nb.in_bounds() { continue; }
            let bj = bg[nb.idx()];
            if bj != u16::MAX { continue; }
            let count = self.time_flood(nb, n.saturating_sub(1), bg);
            if count > best_n { best_n = count; best = (dx, dy); }
        }
        best
    }
}

fn spawn_food(body: &VecDeque<Pt>, blocks: &[bool; GRID]) -> Pt {
    loop {
        let p = Pt { x: rand::gen_range(0, COLS), y: rand::gen_range(0, ROWS) };
        if !body.contains(&p) && !blocks[p.idx()] {
            return p;
        }
    }
}
