use macroquad::prelude::*;
use minesweeper::board::GridKind;

fn conf() -> Conf {
    Conf {
        window_title: "Minesweeper".to_owned(),
        window_width: 900,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let cli = minesweeper::parse_cli_args();
    let kind = cli.variant.unwrap_or(GridKind::Square);
    if cli.no_ui {
        minesweeper::run_headless(kind, cli);
    } else {
        macroquad::Window::from_config(conf(), minesweeper::run(kind, cli));
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    macroquad::Window::from_config(
        conf(),
        minesweeper::run(GridKind::Square, minesweeper::parse_cli_args()),
    );
}
