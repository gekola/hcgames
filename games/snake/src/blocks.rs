use macroquad::prelude::*;
use crate::{Pt, COLS, ROWS, GRID, DIRS};

pub fn generate_blocks() -> [bool; GRID] {
    let cx = COLS / 2;
    let cy = ROWS / 2;

    for _ in 0..2000 {
        let mut blocks = [false; GRID];
        let n_rects = rand::gen_range(4i32, 9);
        for _ in 0..n_rects {
            let w = rand::gen_range(2i32, 5);
            let h = rand::gen_range(2i32, 4);
            let x = rand::gen_range(1, COLS - w - 1);
            let y = rand::gen_range(1, ROWS - h - 1);
            // skip if overlaps snake spawn zone
            if (x..x + w).any(|bx| (y..y + h).any(|by| (bx - cx).abs() <= 5 && (by - cy).abs() <= 4)) {
                continue;
            }
            for bx in x..x + w {
                for by in y..y + h {
                    blocks[(by * COLS + bx) as usize] = true;
                }
            }
        }
        if is_valid_layout(&blocks) {
            return blocks;
        }
    }
    [false; GRID]
}

fn is_valid_layout(blocks: &[bool; GRID]) -> bool {
    // Find start and count total passable in a single pass
    let mut start = None;
    let mut total_passable = 0usize;
    for i in 0..GRID {
        if !blocks[i] {
            if start.is_none() { start = Some(i); }
            total_passable += 1;
        }
    }
    let start = match start {
        Some(i) => i,
        None => return false,
    };

    let mut visited = [false; GRID];
    let mut queue = [0usize; GRID];
    let (mut qh, mut qt) = (0, 0);
    visited[start] = true;
    queue[qt] = start; qt += 1;
    let mut reachable = 1usize;

    while qh < qt {
        let cur = queue[qh]; qh += 1;
        let p = Pt { x: (cur % COLS as usize) as i32, y: (cur / COLS as usize) as i32 };
        for (dx, dy) in DIRS {
            let nb = p.shifted(dx, dy);
            if !nb.in_bounds() { continue; }
            let ni = nb.idx();
            if visited[ni] || blocks[ni] { continue; }
            visited[ni] = true;
            queue[qt] = ni; qt += 1;
            reachable += 1;
        }
    }

    if reachable != total_passable { return false; }
    // Dead-end check omitted: a degree-1 node's sole neighbor is always an AP,
    // so no_articulation_points subsumes it.
    no_articulation_points(blocks)
}

// Tarjan's AP via iterative DFS: returns true if passable graph has no articulation points.
// An AP is a cell whose removal disconnects the passable region — a geometric bottleneck trap.
fn no_articulation_points(blocks: &[bool; GRID]) -> bool {
    let start = match (0..GRID).find(|&i| !blocks[i]) {
        Some(i) => i,
        None => return true,
    };

    let mut disc     = [u32::MAX; GRID];
    let mut low      = [0u32; GRID];
    let mut par      = [GRID; GRID]; // GRID = no parent (sentinel)
    let mut children = [0u32; GRID];
    let mut timer    = 0u32;

    disc[start] = timer; low[start] = timer; timer += 1;
    let mut stk: Vec<(usize, usize)> = vec![(start, 0)]; // (node, next_dir_idx)

    while !stk.is_empty() {
        let (u, di) = *stk.last().unwrap();

        if di >= 4 {
            stk.pop();
            if let Some(&(p, _)) = stk.last() {
                if low[u] < low[p] { low[p] = low[u]; }
                // non-root AP: subtree under u can't reach above p without going through p
                if par[p] != GRID && low[u] >= disc[p] { return false; }
            }
            continue;
        }

        stk.last_mut().unwrap().1 += 1;

        let pu = Pt { x: (u % COLS as usize) as i32, y: (u / COLS as usize) as i32 };
        let (dx, dy) = DIRS[di];
        let pv = pu.shifted(dx, dy);
        if !pv.in_bounds() { continue; }
        let v = pv.idx();
        if blocks[v] { continue; }

        if disc[v] == u32::MAX {
            par[v] = u;
            children[u] += 1;
            disc[v] = timer; low[v] = timer; timer += 1;
            stk.push((v, 0));
        } else if v != par[u] {
            if disc[v] < low[u] { low[u] = disc[v]; }
        }
    }

    children[start] <= 1 // root AP iff it has >1 DFS tree children
}
