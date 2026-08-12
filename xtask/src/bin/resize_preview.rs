//! Downscales a preview PNG into the responsive `preview-<w>.png` tiers referenced by
//! generate_index.rs's game-card `srcset`. Usage: `resize_preview <preview.png> <out-dir>`.
use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilterType, PngEncoder};
use image::{GenericImageView, RgbaImage};
use std::env;
use std::fs::File;
use std::path::Path;

/// Tier widths, as exact fractions of the game's native preview width — 1/4, 1/3, 1/2 and
/// 2/3, skipping any that wouldn't divide evenly.
///
/// Deriving them from the source instead of using fixed widths is worth far more than it
/// looks. A downscale by an exact integer ratio maps whole blocks of source pixels onto
/// one output pixel, so a flat region stays exactly one color; an arbitrary ratio spreads
/// each edge across a fringe of near-unique averaged colors, which PNG then has to store.
/// The resolutions barely differ, the file sizes are not close (KiB):
///
/// | game        | 480w | 450w (1/2) | 640w | 600w (2/3) | native |
/// |-------------|------|------------|------|------------|--------|
/// | match-3     | 49.1 |       30.2 | 73.2 |       58.4 |  105.3 |
/// | minesweeper | 17.0 |       10.2 | 23.6 |       13.4 |   36.7 |
/// | sudoku      | 24.6 |       17.9 | 35.9 |       28.7 |   50.9 |
///
/// This is also what stops the largest tier being pointless: at 640w it was 70-80% of the
/// native file, so serving it barely beat serving the original.
const TIER_FRACTIONS: [(u32, u32); 4] = [(1, 4), (1, 3), (1, 2), (2, 3)];

/// Exact area-average ("box") downscale: every output pixel is the mean of the source
/// pixels its footprint covers.
///
/// The obvious alternatives both fail on flat-shaded game UI. A smoothing filter
/// (Lanczos3/CatmullRom/Triangle) blends every hard edge into a wide gradient of near-
/// unique colors, which wrecks PNG compressibility far more than the resize saves —
/// measured 640w Lanczos3 output *bigger* than the 900w source for several games.
/// Nearest is the opposite extreme: smallest of all, but at a non-integer ratio it drops
/// whole pixel rows, which erased sudoku's thin cell borders and reduced the HUD line to
/// mush at 320w.
///
/// Area-average sits between them and is the only one that holds up at the smallest tier.
/// Measured 320w output (KiB), source → nearest / area / triangle / lanczos3:
///
/// | game        | src  | nearest | area | triangle | lanczos3 |
/// |-------------|------|---------|------|----------|----------|
/// | sudoku      | 50.9 |     6.7 | 14.9 |     19.1 |     28.8 |
/// | minesweeper | 36.7 |     3.8 | 13.7 |     23.0 |     43.5 |
/// | match-3     |105.3 |    12.5 | 26.5 |     37.7 |     49.4 |
///
/// Area costs ~2x nearest's bytes and stays well under every smoothing filter — and since
/// a visitor is served exactly one tier, even an area-scaled small tier undercuts the
/// single 640w-nearest variant this replaced, while looking considerably better.
fn area_resize(img: &image::DynamicImage, tw: u32) -> RgbaImage {
    let (sw, sh) = img.dimensions();
    let th = (sh as u64 * tw as u64 / sw as u64) as u32;
    let src = img.to_rgba8();
    let mut out = RgbaImage::new(tw, th);
    for oy in 0..th {
        let y0 = (oy as u64 * sh as u64 / th as u64) as u32;
        let y1 = ((((oy + 1) as u64 * sh as u64).div_ceil(th as u64)) as u32).min(sh);
        for ox in 0..tw {
            let x0 = (ox as u64 * sw as u64 / tw as u64) as u32;
            let x1 = ((((ox + 1) as u64 * sw as u64).div_ceil(tw as u64)) as u32).min(sw);
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0..y1.max(y0 + 1) {
                for x in x0..x1.max(x0 + 1) {
                    let p = src.get_pixel(x, y).0;
                    r += p[0] as u64;
                    g += p[1] as u64;
                    b += p[2] as u64;
                    a += p[3] as u64;
                    n += 1;
                }
            }
            let m = |v: u64| (v / n) as u8;
            out.put_pixel(ox, oy, image::Rgba([m(r), m(g), m(b), m(a)]));
        }
    }
    out
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let (src, out_dir) = (Path::new(&args[1]), Path::new(&args[2]));

    // One decode for every tier, rather than one process (and one decode) per width.
    let img = image::open(src).unwrap();
    let native = img.width();
    for (num, den) in TIER_FRACTIONS {
        // A fraction that doesn't divide evenly would defeat the whole point (see
        // TIER_FRACTIONS) — game2048's 500px preview has no exact 1/3 or 2/3, so it just
        // gets fewer tiers. generate_index.rs reads whichever ones exist and falls back to
        // a plain `src` with no `srcset` when there are none.
        if (native * num) % den != 0 {
            continue;
        }
        let width = native * num / den;
        let resized = area_resize(&img, width);

        // image's default `.save()` uses fast/low-effort PNG compression — spell out best
        // compression explicitly since these are build-time assets, not a latency-sensitive path.
        let encoder = PngEncoder::new_with_quality(
            File::create(out_dir.join(format!("preview-{width}.png"))).unwrap(),
            CompressionType::Best,
            PngFilterType::Adaptive,
        );
        encoder
            .write_image(
                &resized,
                resized.width(),
                resized.height(),
                ExtendedColorType::Rgba8,
            )
            .unwrap();
    }
}
