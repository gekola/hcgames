pub const N: usize = 4;

pub const SNAKE: [[f64; N]; N] = [
    [32768.0, 16384.0, 8192.0, 4096.0],
    [256.0, 512.0, 1024.0, 2048.0],
    [128.0, 64.0, 32.0, 16.0],
    [1.0, 2.0, 4.0, 8.0],
];

// --- Board transforms ---

pub fn flip_h(mut b: [[u32; N]; N]) -> [[u32; N]; N] {
    for row in &mut b {
        row.reverse();
    }
    b
}

pub fn transpose(b: [[u32; N]; N]) -> [[u32; N]; N] {
    let mut t = [[0u32; N]; N];
    for i in 0..N {
        for j in 0..N {
            t[j][i] = b[i][j];
        }
    }
    t
}

pub fn rot90(b: [[u32; N]; N]) -> [[u32; N]; N] {
    flip_h(transpose(b))
}

pub fn prep(board: [[u32; N]; N], dir: u8) -> [[u32; N]; N] {
    match dir {
        0 => board,
        1 => flip_h(board),
        2 => transpose(board),
        _ => flip_h(transpose(board)),
    }
}

pub fn unprep(board: [[u32; N]; N], dir: u8) -> [[u32; N]; N] {
    match dir {
        0 => board,
        1 => flip_h(board),
        2 => transpose(board),
        _ => transpose(flip_h(board)),
    }
}

// --- AI heuristic ---

pub fn heuristic(b: &[[u32; N]; N]) -> f64 {
    let dot = |b: &[[u32; N]; N]| -> f64 {
        (0..N)
            .flat_map(|r| (0..N).map(move |c| b[r][c] as f64 * SNAKE[r][c]))
            .sum::<f64>()
    };
    let mut board = *b;
    let mut best = f64::NEG_INFINITY;
    for _ in 0..4 {
        best = best.max(dot(&board)).max(dot(&flip_h(board)));
        board = rot90(board);
    }
    best
}

// --- Slide ---

// Returns the merged row and points scored.
pub fn merge_row(row: [u32; N]) -> ([u32; N], u32) {
    let mut buf = [0u32; N];
    let mut k = 0usize;
    let mut pts = 0u32;
    for &v in &row {
        if v != 0 {
            buf[k] = v;
            k += 1;
        }
    }
    let mut i = 0;
    while i + 1 < k {
        if buf[i] == buf[i + 1] {
            buf[i] *= 2;
            pts += buf[i];
            for j in i + 1..k - 1 {
                buf[j] = buf[j + 1];
            }
            buf[k - 1] = 0;
            k -= 1;
        }
        i += 1;
    }
    (buf, pts)
}

// Returns None if the board didn't change.
pub fn slide(board: [[u32; N]; N], dir: u8) -> Option<([[u32; N]; N], u32)> {
    let p = prep(board, dir);
    let mut sp = p;
    let mut pts = 0u32;
    for r in 0..N {
        let (row, rpts) = merge_row(p[r]);
        sp[r] = row;
        pts += rpts;
    }
    if sp == p {
        None
    } else {
        Some((unprep(sp, dir), pts))
    }
}

// --- Expectimax ---

pub fn expectimax(board: [[u32; N]; N], depth: u8, player: bool) -> f64 {
    if depth == 0 {
        return heuristic(&board);
    }
    if player {
        let mut best = f64::NEG_INFINITY;
        let mut any = false;
        for dir in 0..4u8 {
            if let Some((nb, _)) = slide(board, dir) {
                any = true;
                let s = expectimax(nb, depth - 1, false);
                if s > best {
                    best = s;
                }
            }
        }
        if any { best } else { heuristic(&board) }
    } else {
        let mut empties = [(0usize, 0usize); N * N];
        let mut count = 0usize;
        for (r, row) in board.iter().enumerate() {
            for (c, &cell) in row.iter().enumerate() {
                if cell == 0 {
                    empties[count] = (r, c);
                    count += 1;
                }
            }
        }
        if count == 0 {
            return heuristic(&board);
        }
        let n = count as f64;
        empties[..count]
            .iter()
            .map(|&(r, c)| {
                let mut b2 = board;
                b2[r][c] = 2;
                let s2 = expectimax(b2, depth - 1, true);
                let mut b4 = board;
                b4[r][c] = 4;
                let s4 = expectimax(b4, depth - 1, true);
                (0.9 * s2 + 0.1 * s4) / n
            })
            .sum()
    }
}

// --- Game logic ---

pub fn can_move(board: &[[u32; N]; N]) -> bool {
    for r in 0..N {
        for c in 0..N {
            if board[r][c] == 0 {
                return true;
            }
            if r + 1 < N && board[r + 1][c] == board[r][c] {
                return true;
            }
            if c + 1 < N && board[r][c + 1] == board[r][c] {
                return true;
            }
        }
    }
    false
}

pub fn choose_dir(board: &[[u32; N]; N]) -> Option<u8> {
    let empty = board
        .iter()
        .flat_map(|r| r.iter())
        .filter(|&&v| v == 0)
        .count();
    let depth: u8 = match empty {
        0..=3 => 5,
        4..=9 => 4,
        _ => 3,
    };
    let mut best = f64::NEG_INFINITY;
    let mut result = None;
    for dir in 0..4u8 {
        if let Some((nb, _)) = slide(*board, dir) {
            let s = expectimax(nb, depth, false);
            if s > best {
                best = s;
                result = Some(dir);
            }
        }
    }
    result
}
