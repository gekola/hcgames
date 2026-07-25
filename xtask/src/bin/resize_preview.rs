//! Downscales a preview PNG to a target width, preserving aspect ratio — produces the
//! small/mobile tier referenced by generate_index.rs's game-card `srcset`.
use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilterType, PngEncoder};
use image::imageops::FilterType;
use std::env;
use std::fs::File;

fn main() {
    let args: Vec<String> = env::args().collect();
    let (src, dst, width) = (&args[1], &args[2], args[3].parse::<u32>().unwrap());

    let img = image::open(src).unwrap();
    if img.width() <= width {
        // Source is already at or below the target (e.g. game2048's fixed-size, already-small
        // preview) — a "small" variant would only upscale it for no benefit, so skip it.
        // generate_index.rs falls back to a plain `src` (no srcset) when this file is absent.
        return;
    }
    let height = (img.height() as u64 * width as u64 / img.width() as u64) as u32;
    // Nearest, not a smoothing filter (Lanczos/CatmullRom/Triangle): these screenshots are
    // flat-shaded game UI (solid fills, hard edges), and any smoothing filter blends edge
    // pixels into a wide gradient of near-unique colors, which wrecks PNG's compressibility
    // far more than the resize saves — measured 640w Lanczos3 output *bigger* than the
    // original 900w source for klondike/spider/sudoku/minesweeper. Nearest keeps flat runs
    // flat and comes out smaller than the source despite fewer total pixels.
    let resized = img.resize(width, height, FilterType::Nearest).to_rgba8();

    // image's default `.save()` uses fast/low-effort PNG compression — spell out best
    // compression explicitly since these are build-time assets, not a latency-sensitive path.
    let encoder = PngEncoder::new_with_quality(
        File::create(dst).unwrap(),
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
