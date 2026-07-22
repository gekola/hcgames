use crate::card::Card;
use macroquad::prelude::*;

pub const RANK_STRS: [&str; 13] = [
    "A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K",
];

const TAU: f32 = std::f32::consts::TAU;
const PI: f32 = std::f32::consts::PI;

fn suit_color(suit: u8) -> Color {
    if suit == 1 || suit == 2 {
        Color::new(0.80, 0.04, 0.04, 1.0)
    } else {
        Color::new(0.05, 0.05, 0.08, 1.0)
    }
}

// ── Rounded rectangle helpers ─────────────────────────────────────────────────

fn draw_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    draw_rectangle(x + r, y, w - 2.0 * r, h, color);
    draw_rectangle(x, y + r, r, h - 2.0 * r, color);
    draw_rectangle(x + w - r, y + r, r, h - 2.0 * r, color);
    draw_circle(x + r, y + r, r, color);
    draw_circle(x + w - r, y + r, r, color);
    draw_circle(x + r, y + h - r, r, color);
    draw_circle(x + w - r, y + h - r, r, color);
}

fn corner_arc(cx: f32, cy: f32, outer_r: f32, start_deg: f32, thickness: f32, color: Color) {
    let segs = 6u32;
    let step = 90.0_f32.to_radians() / segs as f32;
    let start = start_deg.to_radians();
    let inner = (outer_r - thickness).max(0.0);
    for i in 0..segs {
        let a0 = start + i as f32 * step;
        let a1 = a0 + step;
        let v = [
            vec2(cx + inner * a0.cos(), cy + inner * a0.sin()),
            vec2(cx + outer_r * a0.cos(), cy + outer_r * a0.sin()),
            vec2(cx + outer_r * a1.cos(), cy + outer_r * a1.sin()),
            vec2(cx + inner * a1.cos(), cy + inner * a1.sin()),
        ];
        draw_triangle(v[0], v[1], v[2], color);
        draw_triangle(v[0], v[2], v[3], color);
    }
}

fn draw_rounded_rect_border(x: f32, y: f32, w: f32, h: f32, r: f32, t: f32, color: Color) {
    draw_rectangle(x + r, y, w - 2.0 * r, t, color);
    draw_rectangle(x + r, y + h - t, w - 2.0 * r, t, color);
    draw_rectangle(x, y + r, t, h - 2.0 * r, color);
    draw_rectangle(x + w - t, y + r, t, h - 2.0 * r, color);
    corner_arc(x + r, y + r, r, 180.0, t, color);
    corner_arc(x + w - r, y + r, r, 270.0, t, color);
    corner_arc(x + w - r, y + h - r, r, 0.0, t, color);
    corner_arc(x + r, y + h - r, r, 90.0, t, color);
}

// ── Suit shape helpers ────────────────────────────────────────────────────────

// Shared Bezier-flare stem used by both clubs and spades.
// Fans from `bot` outward along two quadratic Bezier curves to left/right corners.
// Works for both normal (bot_y > apex_y) and flipped (bot_y < apex_y) orientation.
fn draw_stem(cx: f32, apex_y: f32, bot_y: f32, bw: f32, col: Color) {
    let apex = vec2(cx, apex_y);
    let pr = vec2(cx + bw, bot_y);
    let pl = vec2(cx - bw, bot_y);
    let ctrl = vec2(cx, apex_y + (bot_y - apex_y) * 0.70);
    let bz = |p0: Vec2, p1: Vec2, p2: Vec2, t: f32| -> Vec2 {
        let u = 1.0 - t;
        p0 * (u * u) + p1 * (2.0 * t * u) + p2 * (t * t)
    };
    let bot = vec2(cx, bot_y);
    let n = 20u32;
    for i in 0..n {
        let (f, g) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        draw_triangle(bot, bz(apex, ctrl, pr, f), bz(apex, ctrl, pr, g), col);
        draw_triangle(bot, bz(apex, ctrl, pl, f), bz(apex, ctrl, pl, g), col);
    }
}

// ── Suit shapes ───────────────────────────────────────────────────────────────
// y_sign: 1.0 = normal, -1.0 = flipped 180° around (cx, cy)

fn draw_diamond(cx: f32, cy: f32, size: f32, col: Color, y_sign: f32) {
    let hw = size * 0.40;
    let hh = size * 0.54;
    draw_triangle(
        vec2(cx, cy - hh * y_sign),
        vec2(cx - hw, cy),
        vec2(cx + hw, cy),
        col,
    );
    draw_triangle(
        vec2(cx, cy + hh * y_sign),
        vec2(cx - hw, cy),
        vec2(cx + hw, cy),
        col,
    );
}

fn draw_heart(cx: f32, cy: f32, size: f32, col: Color, y_sign: f32) {
    let sx = size * 0.026;
    let sy = size * 0.031;
    let y_c = -2.7_f32;
    let pt = |t: f32| {
        let x = 16.0 * t.sin().powi(3);
        let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
        vec2(cx + x * sx, cy + (y_c - y) * sy * y_sign)
    };
    for i in 0..32u32 {
        let t0 = i as f32 / 32.0 * TAU;
        let t1 = (i + 1) as f32 / 32.0 * TAU;
        draw_triangle(vec2(cx, cy), pt(t0), pt(t1), col);
    }
}

fn draw_spade(cx: f32, cy: f32, size: f32, col: Color, y_sign: f32) {
    let sx = size * 0.026;
    let sy = size * 0.028;
    let y_c = -2.7_f32;
    let body_cy = cy - size * 0.10 * y_sign;

    let pt = |t: f32| {
        let x = 16.0 * t.sin().powi(3);
        let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
        vec2(cx + x * sx, body_cy + (y - y_c) * sy * y_sign)
    };
    let notch_y = body_cy + 7.7 * sy * y_sign;
    for i in 0..32u32 {
        let t0 = i as f32 / 32.0 * TAU;
        let t1 = (i + 1) as f32 / 32.0 * TAU;
        draw_triangle(vec2(cx, notch_y), pt(t0), pt(t1), col);
    }

    let stem_bot = cy + size * 0.38 * y_sign;
    draw_stem(cx, notch_y, stem_bot, size * 0.18, col);
}

fn draw_club(cx: f32, cy: f32, size: f32, col: Color, y_sign: f32) {
    let r = size * 0.23;
    let ca = vec2(cx, cy - r * 0.65 * y_sign);
    let cb = vec2(cx - r * 0.88, cy + r * 0.36 * y_sign);
    let cc = vec2(cx + r * 0.88, cy + r * 0.36 * y_sign);

    if col.a > 0.99 {
        // Opaque: macroquad's draw_circle has soft anti-aliased edges that make the
        // three lobes visually distinct; overlapping same-color opaque fills are fine.
        draw_circle(ca.x, ca.y, r, col);
        draw_circle(cb.x, cb.y, r, col);
        draw_circle(cc.x, cc.y, r, col);
    } else {
        // Semi-transparent (e.g. ghost on empty slot): overlapping fills composite the
        // alpha multiple times, making overlap zones visually darker than single regions.
        // Trace the union boundary with a single triangle fan — no pixel drawn twice.
        let centers = [ca, cb, cc];
        let centroid = (ca + cb + cc) / 3.0;
        const N: usize = 72;
        let mut pts = [Vec2::ZERO; N];
        for (i, pt) in pts.iter_mut().enumerate() {
            let angle = i as f32 / N as f32 * TAU;
            let dir = vec2(angle.cos(), angle.sin());
            let mut max_t = 0.0f32;
            for &c in &centers {
                let oc = centroid - c;
                let b = dir.dot(oc);
                let disc = b * b - (oc.dot(oc) - r * r);
                if disc >= 0.0 {
                    let t = -b + disc.sqrt();
                    if t > max_t {
                        max_t = t;
                    }
                }
            }
            *pt = centroid + dir * max_t;
        }
        for i in 0..N {
            draw_triangle(centroid, pts[i], pts[(i + 1) % N], col);
        }
    }

    let st = cy + r * 0.35 * y_sign;
    let bbot = st + size * 0.40 * y_sign;
    draw_stem(cx, st, bbot, size * 0.18, col);
}

/// Draw a suit symbol centered at (cx, cy) fitting within `size` pixels.
pub fn draw_suit_symbol(cx: f32, cy: f32, size: f32, suit: u8, col: Color) {
    match suit {
        0 => draw_club(cx, cy, size, col, 1.0),
        1 => draw_diamond(cx, cy, size, col, 1.0),
        2 => draw_heart(cx, cy, size, col, 1.0),
        3 => draw_spade(cx, cy, size, col, 1.0),
        _ => {}
    }
}

fn draw_suit_symbol_flipped(cx: f32, cy: f32, size: f32, suit: u8, col: Color) {
    match suit {
        0 => draw_club(cx, cy, size, col, -1.0),
        1 => draw_diamond(cx, cy, size, col, -1.0),
        2 => draw_heart(cx, cy, size, col, -1.0),
        3 => draw_spade(cx, cy, size, col, -1.0),
        _ => {}
    }
}

// ── Card faces / backs ────────────────────────────────────────────────────────

pub fn draw_card_face(x: f32, y: f32, w: f32, h: f32, card: Card) {
    let r = (w * 0.07).max(3.0);
    draw_rounded_rect(x + 2.0, y + 2.0, w, h, r, Color::new(0.0, 0.0, 0.0, 0.28));
    draw_rounded_rect(x, y, w, h, r, Color::new(0.97, 0.96, 0.93, 1.0));
    draw_rounded_rect_border(x, y, w, h, r, 2.5, Color::new(0.45, 0.45, 0.50, 1.0));

    let col = suit_color(card.suit);
    let rank = RANK_STRS[card.rank as usize];
    let pad = (w * 0.07).max(3.0);
    let fs = (h * 0.175).max(9.0);
    let sym = fs * 0.72; // fits within cap-height of font line

    let rd = measure_text(rank, None, fs as u16, 1.0);
    // offset from text baseline to vertical center of glyphs (~cap-height midpoint)
    let mid = fs * 0.38;

    // top-left: [rank][suit] on same line
    let tl_base = y + pad + fs;
    draw_text(rank, x + pad, tl_base, fs, col);
    draw_suit_symbol(
        x + pad + rd.width + fs * 0.06 + sym * 0.5,
        tl_base - mid,
        sym,
        card.suit,
        col,
    );

    // centre suit (large)
    let big = (h * 0.36).max(12.0);
    draw_suit_symbol(x + w * 0.5, y + h * 0.52, big, card.suit, col);

    // bottom-right: same layout as top-left but rotated 180°.
    // With rotation=PI the text anchor extends LEFT from br_x.
    let br_x = x + w - pad;
    let br_y = y + h - pad - fs;
    draw_suit_symbol_flipped(
        br_x - rd.width - fs * 0.06 - sym * 0.5,
        br_y + mid,
        sym,
        card.suit,
        col,
    );
    draw_text_ex(
        rank,
        br_x,
        br_y,
        TextParams {
            font_size: fs as u16,
            rotation: PI,
            color: col,
            ..Default::default()
        },
    );
}

pub fn draw_card_back(x: f32, y: f32, w: f32, h: f32) {
    let r = (w * 0.07).max(3.0);
    draw_rounded_rect(x, y, w, h, r, Color::new(0.13, 0.24, 0.62, 1.0));
    draw_rounded_rect_border(x, y, w, h, r, 2.5, Color::new(0.55, 0.60, 0.84, 0.90));
    let m = (w * 0.10).max(3.0);
    draw_rounded_rect_border(
        x + m,
        y + m,
        w - 2.0 * m,
        h - 2.0 * m,
        (r * 0.5).max(1.5),
        1.5,
        Color::new(0.32, 0.48, 0.85, 0.50),
    );
}

pub fn draw_empty_slot(x: f32, y: f32, w: f32, h: f32) {
    let r = (w * 0.07).max(3.0);
    draw_rounded_rect(x, y, w, h, r, Color::new(0.08, 0.10, 0.16, 0.50));
    draw_rounded_rect_border(x, y, w, h, r, 1.5, Color::new(0.28, 0.33, 0.50, 0.45));
}
