/// The three CLI flags every game in this workspace accepts (see CLAUDE.md's "Native
/// CLI flags" table) — `--debug`, `--once`, and `--no-ui`. A game with its own extra
/// flag (klondike/spider/sudoku/minesweeper's `--variant`) parses `parse_base_args`'s
/// leftover tokens itself; see any of those games' `parse_cli_args` for the pattern.
pub struct BaseCliArgs {
    pub debug: bool,
    pub once: bool,
    /// Meaningless in a browser tab — only ever `true` via a native CLI flag.
    #[cfg(not(target_arch = "wasm32"))]
    pub no_ui: bool,
}

/// Parses `--debug`/`--once`/`--no-ui` out of `args` (already stripped of argv\[0\], i.e.
/// `std::env::args().skip(1)`), returning them plus every argument it didn't recognize
/// — in original order — for the caller to parse itself (a game-specific `--variant
/// <value>` pair, for example).
#[cfg(not(target_arch = "wasm32"))]
pub fn parse_base_args(args: &[String]) -> (BaseCliArgs, Vec<String>) {
    let mut debug = false;
    let mut once = false;
    let mut no_ui = false;
    let mut rest = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--debug" => debug = true,
            "--once" => once = true,
            "--no-ui" => no_ui = true,
            _ => rest.push(arg.clone()),
        }
    }
    (BaseCliArgs { debug, once, no_ui }, rest)
}

/// WASM has no real argv (a browser tab never passes CLI flags) — always defaults,
/// matching every game's existing `#[cfg(target_arch = "wasm32")] fn parse_cli_args`.
#[cfg(target_arch = "wasm32")]
pub fn parse_base_args(_args: &[String]) -> (BaseCliArgs, Vec<String>) {
    (
        BaseCliArgs {
            debug: false,
            once: false,
        },
        Vec::new(),
    )
}
