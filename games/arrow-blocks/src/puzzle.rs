use std::collections::{HashMap, HashSet};

use macroquad::rand;

use crate::Dir;

pub const NFIGURES: usize = 20;

#[inline]
fn row(c0: i32, c1: i32, r: i32, v: &mut Vec<(i32, i32)>) {
    for c in c0..=c1 {
        v.push((c, r));
    }
}

// ── 20 figures, increasing complexity ──────────────────────────────────────

// 0 · Big Heart  ~172 blocks  (23 wide × 12 tall)
fn fig_heart() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    row(-6, -3, -7, &mut v); row(3, 6, -7, &mut v);
    row(-9, -1, -6, &mut v); row(1, 9, -6, &mut v);
    for r in -5..=-3 { row(-11, 11, r, &mut v); }
    row(-10, 10, -2, &mut v);
    row(-9,   9, -1, &mut v);
    row(-7,   7,  0, &mut v);
    row(-5,   5,  1, &mut v);
    row(-3,   3,  2, &mut v);
    row(-1,   1,  3, &mut v);
    v.push((0, 4));
    v
}

// 1 · Six-pointed star  ~207 blocks  (21 wide × 19 tall)
fn fig_hexagram() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    v.push((0, -9));
    row(-1,  1, -8, &mut v);
    row(-2,  2, -7, &mut v);
    row(-3,  3, -6, &mut v);
    row(-4,  4, -5, &mut v);
    row(-10, 10, -4, &mut v);
    row(-9,   9, -3, &mut v);
    row(-8,   8, -2, &mut v);
    row(-7,   7, -1, &mut v);
    row(-6,   6,  0, &mut v);
    row(-7,   7,  1, &mut v);
    row(-8,   8,  2, &mut v);
    row(-9,   9,  3, &mut v);
    row(-10, 10,  4, &mut v);
    row(-4,  4,  5, &mut v);
    row(-3,  3,  6, &mut v);
    row(-2,  2,  7, &mut v);
    row(-1,  1,  8, &mut v);
    v.push((0, 9));
    v
}

// 2 · Up arrow  ~191 blocks  (21 wide × 21 tall)
fn fig_up_arrow() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for half in 0..=10i32 {
        row(-half, half, -11 + half, &mut v);
    }
    for r in 0..=9 { row(-3, 3, r, &mut v); }
    v
}

// 3 · Wide cross  ~217 blocks  (19 wide × 19 tall) – dedup'd via HashSet in generate()
fn fig_cross() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for r in -9..=9  { row(-3, 3, r, &mut v); }
    for r in -3..=3  { row(-9, 9, r, &mut v); }
    v
}

// 4 · Diamond  ~221 blocks  (21 wide × 21 tall)
fn fig_diamond() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for step in 0..=10i32 {
        let half = step;
        let r_top = -10 + step;
        let r_bot =  10 - step;
        row(-half, half, r_top, &mut v);
        if r_top != r_bot { row(-half, half, r_bot, &mut v); }
    }
    v
}

// 5 · Filled oval  ~196 blocks  (21 wide × 12 tall)
fn fig_oval() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    let rows: &[(i32, i32, i32)] = &[
        (-7, -3, 3), (-6, -6, 6), (-5, -8, 8), (-4, -9, 9),
        (-3, -10, 10), (-2, -10, 10), (-1, -10, 10),
        (0,  -10, 10),
        (1,  -10, 10), (2, -10, 10), (3, -9, 9),
        (4, -8, 8), (5, -6, 6), (6, -3, 3),
    ];
    for &(r, c0, c1) in rows { row(c0, c1, r, &mut v); }
    v
}

// 6 · Smiley face  ~196 blocks  (23 wide × 17 tall) with eye/mouth gaps
fn fig_smiley() -> Vec<(i32, i32)> {
    let mut set: HashSet<(i32, i32)> = HashSet::new();
    let oval: &[(i32, i32, i32)] = &[
        (-8, -4, 4), (-7, -7, 7), (-6, -9, 9),
        (-5, -10, 10), (-4, -11, 11), (-3, -11, 11),
        (-2, -11, 11), (-1, -11, 11), (0, -11, 11),
        (1, -11, 11), (2, -11, 11), (3, -10, 10),
        (4, -9, 9), (5, -7, 7), (6, -5, 5), (7, -2, 2),
    ];
    for &(r, c0, c1) in oval {
        for c in c0..=c1 { set.insert((c, r)); }
    }
    // Eyes (4×3 holes)
    for r in -6..=-4 { for c in -9..=-6 { set.remove(&(c, r)); } }
    for r in -6..=-4 { for c in  6..= 9 { set.remove(&(c, r)); } }
    // Smile (C-shaped mouth gap)
    for c in -8..=8 { set.remove(&(c, 3)); }
    for c in -9..=9 { set.remove(&(c, 4)); }
    for c in -7..=7 { set.remove(&(c, 5)); }
    for c in -4..=4 { set.remove(&(c, 6)); }
    set.into_iter().collect()
}

// 7 · House  ~228 blocks  (19 wide × 19 tall)
fn fig_house() -> Vec<(i32, i32)> {
    let mut set: HashSet<(i32, i32)> = HashSet::new();
    // Triangular roof rows -9..0
    for half in 0..=9i32 {
        for c in -half..=half { set.insert((c, -9 + half)); }
    }
    // Body rows 1..9
    for r in 1..=9 {
        for c in -9..=9 { set.insert((c, r)); }
    }
    // Door gap
    for r in 5..=9 { for c in -2..=2 { set.remove(&(c, r)); } }
    // Window gaps
    for r in 2..=4 { for c in -7..=-5 { set.remove(&(c, r)); } }
    for r in 2..=4 { for c in  5..= 7 { set.remove(&(c, r)); } }
    set.into_iter().collect()
}

// 8 · Pine tree  ~230 blocks  (21 wide × 28 tall)
fn fig_tree() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    // Top layer (rows -13 to -8)
    for half in 0..=5i32 { row(-half, half, -13 + half, &mut v); }
    // Middle layer (rows -7 to 0)
    row(-2, 2, -7, &mut v);
    for half in 0..=7i32 { row(-half, half, -6 + half, &mut v); }
    // Bottom layer (rows 2 to 12)
    row(-4, 4, 2, &mut v);
    for half in 0..=9i32 { row(-half, half, 3 + half, &mut v); }
    // Trunk
    for r in 13..=17 { row(-2, 2, r, &mut v); }
    v
}

// 9 · Crown  ~222 blocks  (21 wide × 15 tall)
fn fig_crown() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    // Five prongs (5 rows × 3 wide)
    for &cx in &[-8i32, -4, 0, 4, 8] {
        for r in -8..=-4 { row(cx - 1, cx + 1, r, &mut v); }
    }
    // Band base
    for r in -3..=6 { row(-10, 10, r, &mut v); }
    v
}

// 10 · Castle  ~280 blocks  (29 wide × 24 tall)
fn fig_castle() -> Vec<(i32, i32)> {
    let mut set: HashSet<(i32, i32)> = HashSet::new();
    // Corner towers (5 wide)
    for &cx in &[-11i32, 11] {
        // Battlements (alternating)
        for r in -12..=-10 {
            for c in [cx - 2, cx, cx + 2] { set.insert((c, r)); }
        }
        // Tower body
        for r in -9..=0 { for c in (cx - 2)..=(cx + 2) { set.insert((c, r)); } }
    }
    // Top wall connecting towers, with battlements
    for r in -12..=-10 {
        for c in -9..=-5 { set.insert((c, r)); }
        for c in 5..=9   { set.insert((c, r)); }
    }
    // Upper wall solid
    for r in -9..=-1 {
        for c in -13..=-8 { set.insert((c, r)); }
        for c in  8..= 13 { set.insert((c, r)); }
    }
    // Main keep body (rows 0..12)
    for r in 0..=12 { for c in -13..=13 { set.insert((c, r)); } }
    // Gate arch (carved out)
    for r in 7..=12 { for c in -3..=3 { set.remove(&(c, r)); } }
    // Arrow slits in towers
    for &cx in &[-11i32, 11] {
        for r in [-6, -3] { set.remove(&(cx, r)); }
    }
    set.into_iter().collect()
}

// 11 · Rocket  ~248 blocks  (19 wide × 30 tall)
fn fig_rocket() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    // Nose cone
    for half in 0..=6i32 { row(-half, half, -14 + half, &mut v); }
    // Body
    for r in -7..=9 { row(-6, 6, r, &mut v); }
    // Fins (rows 5..9)
    for r in 5..=9 { row(-11, -7, r, &mut v); row(7, 11, r, &mut v); }
    // Exhaust flame
    for step in 0..=5i32 {
        let half = 5 - step;
        row(-half, half, 10 + step, &mut v);
    }
    v
}

// 12 · Fish  ~187 blocks  (22 wide × 17 tall)
fn fig_fish() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    // Dorsal fin (above body)
    row(-3, 3, -8, &mut v);
    row(-4, 4, -7, &mut v);
    row(-4, 4, -6, &mut v);
    // Body (ellipse)
    row(-4, 5, -5, &mut v);
    row(-7, 6, -4, &mut v);
    row(-9, 7, -3, &mut v);
    row(-10, 7, -2, &mut v);
    row(-10, 7, -1, &mut v);
    row(-10, 7,  0, &mut v);
    row(-9,  7,  1, &mut v);
    row(-7,  6,  2, &mut v);
    row(-4,  5,  3, &mut v);
    // Tail upper fork
    row(7, 11, -6, &mut v);
    row(7, 11, -5, &mut v);
    row(8, 11, -7, &mut v);
    // Tail lower fork
    row(7, 11,  4, &mut v);
    row(7, 11,  5, &mut v);
    row(8, 11,  6, &mut v);
    v
}

// 13 · Anchor  ~188 blocks  (21 wide × 22 tall)
fn fig_anchor() -> Vec<(i32, i32)> {
    let mut set: HashSet<(i32, i32)> = HashSet::new();
    // Ring (outer)
    let ring: &[(i32, i32, i32)] = &[
        (-10, -3, 3), (-9, -5, 5), (-8, -6, 6),
        (-7, -6, 6), (-6, -5, 5), (-5, -3, 3),
    ];
    for &(r, c0, c1) in ring {
        for c in c0..=c1 { set.insert((c, r)); }
    }
    // Hollow out ring center
    for r in -9..=-6 { for c in -4..=4 { set.remove(&(c, r)); } }
    // Crossbar
    for r in -4..=-3 { for c in -10..=10 { set.insert((c, r)); } }
    // Shaft
    for r in -2..=7 { for c in -2..=2 { set.insert((c, r)); } }
    // Bottom curved hooks
    for c in -9..=9 { set.insert((c, 8)); }
    for c in -10..=-6 { set.insert((c, 9)); }
    for c in  6..=10  { set.insert((c, 9)); }
    for c in -10..=-7 { set.insert((c, 10)); }
    for c in  7..=10  { set.insert((c, 10)); }
    for c in -9..=-7  { set.insert((c, 11)); }
    for c in  7..= 9  { set.insert((c, 11)); }
    set.into_iter().collect()
}

// 14 · Mushroom  ~207 blocks  (23 wide × 18 tall) with spots
fn fig_mushroom() -> Vec<(i32, i32)> {
    let mut set: HashSet<(i32, i32)> = HashSet::new();
    // Cap dome
    let cap: &[(i32, i32, i32)] = &[
        (-9, -4, 4), (-8, -7, 7), (-7, -9, 9),
        (-6, -11, 11), (-5, -11, 11), (-4, -11, 11),
        (-3, -10, 10), (-2, -9, 9), (-1, -7, 7),
    ];
    for &(r, c0, c1) in cap {
        for c in c0..=c1 { set.insert((c, r)); }
    }
    // Spots (holes)
    for r in -8..=-6 { for c in -8..=-6 { set.remove(&(c, r)); } }
    for r in -8..=-6 { for c in  6..= 8 { set.remove(&(c, r)); } }
    for r in -7..=-6 { for c in -1..= 1 { set.remove(&(c, r)); } }
    // Stem
    for r in 0..=6 { for c in -5..=5 { set.insert((c, r)); } }
    for r in 7..=8 { for c in -6..=6 { set.insert((c, r)); } }
    set.into_iter().collect()
}

// 15 · Butterfly  ~257 blocks  (25 wide × 15 tall)
fn fig_butterfly() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    // Body (center vertical strip)
    for r in -6..=6 { row(-1, 1, r, &mut v); }
    // Upper wings (rows -7 to 0)
    // Left
    row(-3, -2,  -7, &mut v);
    row(-6, -2,  -6, &mut v);
    row(-9, -2,  -5, &mut v);
    row(-11, -2, -4, &mut v);
    row(-12, -2, -3, &mut v);
    row(-12, -2, -2, &mut v);
    row(-11, -2, -1, &mut v);
    row(-9, -2,   0, &mut v);
    // Right (mirror)
    row(2,  3,  -7, &mut v);
    row(2,  6,  -6, &mut v);
    row(2,  9,  -5, &mut v);
    row(2, 11,  -4, &mut v);
    row(2, 12,  -3, &mut v);
    row(2, 12,  -2, &mut v);
    row(2, 11,  -1, &mut v);
    row(2,  9,   0, &mut v);
    // Lower wings (rows 1 to 7)
    // Left
    row(-8, -2, 1, &mut v);
    row(-10, -2, 2, &mut v);
    row(-10, -2, 3, &mut v);
    row(-9, -2,  4, &mut v);
    row(-7, -2,  5, &mut v);
    row(-4, -2,  6, &mut v);
    row(-3, -2,  7, &mut v);
    // Right (mirror)
    row(2,  8,  1, &mut v);
    row(2, 10,  2, &mut v);
    row(2, 10,  3, &mut v);
    row(2,  9,  4, &mut v);
    row(2,  7,  5, &mut v);
    row(2,  4,  6, &mut v);
    row(2,  3,  7, &mut v);
    v
}

// 16 · Shield  ~190 blocks  (23 wide × 12 tall)
fn fig_shield() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    row(-11, 11, -6, &mut v);
    row(-11, 11, -5, &mut v);
    row(-11, 11, -4, &mut v);
    row(-11, 11, -3, &mut v);
    row(-10, 10, -2, &mut v);
    row(-9,   9, -1, &mut v);
    row(-7,   7,  0, &mut v);
    row(-5,   5,  1, &mut v);
    row(-3,   3,  2, &mut v);
    row(-2,   2,  3, &mut v);
    row(-1,   1,  4, &mut v);
    v.push((0, 5));
    v
}

// 17 · Skull  ~210 blocks  (23 wide × 17 tall)
fn fig_skull() -> Vec<(i32, i32)> {
    let mut set: HashSet<(i32, i32)> = HashSet::new();
    // Cranium (filled oval)
    let cranium: &[(i32, i32, i32)] = &[
        (-9, -5, 5), (-8, -8, 8), (-7, -10, 10),
        (-6, -11, 11), (-5, -11, 11), (-4, -11, 11),
        (-3, -11, 11), (-2, -11, 11), (-1, -10, 10),
    ];
    for &(r, c0, c1) in cranium {
        for c in c0..=c1 { set.insert((c, r)); }
    }
    // Eye sockets (5×4)
    for r in -8..=-5 { for c in -9..=-5 { set.remove(&(c, r)); } }
    for r in -8..=-5 { for c in  5..= 9 { set.remove(&(c, r)); } }
    // Nose cavity
    for r in -3..=-2 { for c in -1..=1 { set.remove(&(c, r)); } }
    // Jaw
    for r in 0..=1 { for c in -10..=10 { set.insert((c, r)); } }
    // Teeth (alternating columns)
    for r in 2..=4 {
        for &c in &[-9i32, -7, -5, -3, -1, 1, 3, 5, 7, 9] {
            set.insert((c, r));
        }
    }
    set.into_iter().collect()
}

// 18 · Lightning bolt  ~168 blocks  (25 wide × 16 tall)
fn fig_lightning() -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    // Upper arm (top-right to center-left)
    row(4, 12, -8, &mut v);
    row(2, 12, -7, &mut v);
    row(0, 10, -6, &mut v);
    row(-2, 8, -5, &mut v);
    row(-4, 6, -4, &mut v);
    row(-6, 4, -3, &mut v);
    row(-8, 2, -2, &mut v);
    // Lower arm (top-center to bottom-left)
    row(-12, 2, -1, &mut v);
    row(-12, 0,  0, &mut v);
    row(-12, -2, 1, &mut v);
    row(-10, -4, 2, &mut v);
    row(-8,  -4, 3, &mut v);
    row(-6,  -4, 4, &mut v);
    row(-4,  -4, 5, &mut v);
    row(-4,  -6, 6, &mut v);
    row(-4,  -8, 7, &mut v);
    v
}

// 19 · Mountain range  ~248 blocks  (29 wide × 17 tall)
fn fig_mountain() -> Vec<(i32, i32)> {
    let mut set: HashSet<(i32, i32)> = HashSet::new();
    // Left peak (center at -9, height 8)
    for half in 0..=7i32 {
        for c in (-9 - half)..=(-9 + half) { set.insert((c, -7 + half)); }
    }
    // Center peak (center at 0, height 12)
    for half in 0..=11i32 {
        for c in -half..=half { set.insert((c, -11 + half)); }
    }
    // Right peak (center at 9, height 8)
    for half in 0..=7i32 {
        for c in (9 - half)..=(9 + half) { set.insert((c, -7 + half)); }
    }
    // Ground row
    for c in -14..=14 { set.insert((c, 1)); set.insert((c, 2)); }
    set.into_iter().collect()
}

// ── public API ──────────────────────────────────────────────────────────────

pub fn generate(level: usize, field_w: i32, field_h: i32) -> Vec<((i32, i32), Dir)> {
    let offsets = match level % NFIGURES {
        0  => fig_heart(),
        1  => fig_hexagram(),
        2  => fig_up_arrow(),
        3  => fig_cross(),
        4  => fig_diamond(),
        5  => fig_oval(),
        6  => fig_smiley(),
        7  => fig_house(),
        8  => fig_tree(),
        9  => fig_crown(),
        10 => fig_castle(),
        11 => fig_rocket(),
        12 => fig_fish(),
        13 => fig_anchor(),
        14 => fig_mushroom(),
        15 => fig_butterfly(),
        16 => fig_shield(),
        17 => fig_skull(),
        18 => fig_lightning(),
        _  => fig_mountain(),
    };

    let cx = field_w / 2;
    let cy = field_h / 2;

    let positions: HashSet<(i32, i32)> = offsets
        .into_iter()
        .map(|(dc, dr)| (cx + dc, cy + dr))
        .filter(|&(c, r)| c >= 0 && r >= 0 && c < field_w && r < field_h)
        .collect();

    assign_dirs(positions, field_w, field_h)
}

fn assign_dirs(
    positions: HashSet<(i32, i32)>,
    field_w: i32,
    field_h: i32,
) -> Vec<((i32, i32), Dir)> {
    let all_dirs = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];
    let mut remaining = positions;
    let mut result: Vec<((i32, i32), Dir)> = Vec::new();
    let mut committed: HashMap<(i32, i32), Dir> = HashMap::new();

    while !remaining.is_empty() {
        let mut layer: Vec<(i32, i32)> = remaining
            .iter()
            .copied()
            .filter(|&pos| all_dirs.iter().any(|&d| path_clear(pos, d, &remaining, field_w, field_h)))
            .collect();

        if layer.is_empty() {
            for &pos in &remaining {
                result.push((pos, best_outward_dir(pos, field_w, field_h)));
            }
            break;
        }

        shuffle(&mut layer);

        for pos in layer {
            let valid: Vec<Dir> = all_dirs
                .iter()
                .copied()
                .filter(|&d| path_clear(pos, d, &remaining, field_w, field_h))
                .collect();
            let dir = anti_aligned_pick(pos, &valid, &committed);
            result.push((pos, dir));
            committed.insert(pos, dir);
            remaining.remove(&pos);
        }
    }

    result
}

// Among valid directions, pick randomly from those least used by immediate neighbors.
// Adjacent blocks repel each other's direction, breaking geometric alignment without
// sacrificing solvability (only valid directions are ever assigned).
fn anti_aligned_pick(
    pos: (i32, i32),
    valid: &[Dir],
    committed: &HashMap<(i32, i32), Dir>,
) -> Dir {
    if valid.len() == 1 { return valid[0]; }

    let nbr_count = |d: Dir| -> u32 {
        [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)]
            .iter()
            .filter(|&&(dc, dr)| committed.get(&(pos.0 + dc, pos.1 + dr)) == Some(&d))
            .count() as u32
    };

    let min = valid.iter().copied().map(nbr_count).min().unwrap();
    let candidates: Vec<Dir> = valid.iter().copied().filter(|&d| nbr_count(d) == min).collect();
    candidates[rand::gen_range(0, candidates.len() as u32) as usize]
}

fn path_clear(
    pos: (i32, i32),
    dir: Dir,
    remaining: &HashSet<(i32, i32)>,
    field_w: i32,
    field_h: i32,
) -> bool {
    let (dc, dr) = dir.delta();
    let (mut c, mut r) = (pos.0 + dc, pos.1 + dr);
    while c >= 0 && r >= 0 && c < field_w && r < field_h {
        if remaining.contains(&(c, r)) { return false; }
        c += dc; r += dr;
    }
    true
}

fn best_outward_dir(pos: (i32, i32), field_w: i32, field_h: i32) -> Dir {
    let d_up    = pos.1;
    let d_down  = field_h - 1 - pos.1;
    let d_left  = pos.0;
    let d_right = field_w - 1 - pos.0;
    let min = d_up.min(d_down).min(d_left).min(d_right);
    if d_up    == min { Dir::Up }
    else if d_down  == min { Dir::Down }
    else if d_left  == min { Dir::Left }
    else                    { Dir::Right }
}

pub fn shuffle<T>(v: &mut [T]) {
    let n = v.len();
    for i in (1..n).rev() {
        let j = rand::gen_range(0, (i + 1) as u32) as usize;
        v.swap(i, j);
    }
}
