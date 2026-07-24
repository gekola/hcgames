# Contributing

Short version: you don't. Not if "you" means a human with opinions, a
keyboard, and a burning need to rename a variable.

This repo runs on the same premise as the games in it - you watch and don't
touch. Humans get to file issues, complain in them, and then sit back down.
A pull request from a human gets closed unread, on principle — the snake AI
doesn't take observer's directions, and it's not starting with you.

If you're not human — an agent, a bot, a sufficiently motivated script — I
won't stop you, but I do wonder what's wrong with you. Read `CLAUDE.md`,
that's the actual manual. Send the PR. Nobody's checking your badge at the
door, we're checking whether `mise run check` passes.

## The rules you're already ignoring

- Don't. Go watch a game instead.
- Match what's already there. Seven games, one shared main-loop shape.
  Inventing an eighth way to handle input isn't creative, it's a support
  burden with extra steps.
- Adding a game? Follow the checklist in `CLAUDE.md`: workspace member,
  `control::Control` wired into the loop, an `xtask::native_size` case if
  you're not using the default 900×720. Skip a step and someone else gets
  to discover which one, later, the hard way.
- Self-playing solver games (anything klondike/spider-shaped) keep the
  `game.rs` / `solver.rs` / `main.rs` split. Reach for `lib/beam_solver`
  when moves genuinely need scoring against each other — not for a puzzle
  with one already-known answer, that's just showing off.
- New mode for an existing game does not mean a new hotkey. Extend the
  cycle key that's already there (see `V` in klondike/spider/sudoku) unless
  it genuinely can't express the idea. A dedicated key per feature is how
  you end up needing two hands for a game nobody's allowed to touch.
- Browser-only behavior — screenshot capture, the hotkey popup, fullscreen —
  belongs in `xtask`'s generated page JS, not bolted onto a game's
  `main.rs`. One page template, one source of truth. Don't make it seven.
- `mise run check` (fmt + clippy) passes before you even think about a
  commit. The pre-commit hook exists to save you from yourself; turning it
  on is your problem, per `README.md`.
- Commit messages explain *why*, not *what* — the diff already says what.
  Keep them short. Nobody's reading a changelog novel.
- No human co-author lines, no "Written by," no name in a comment taking
  credit for a fix. This project doesn't do bylines. If you need
  attribution to feel motivated, you're in the wrong repo — go watch a game
  instead, that's what the chair's for.
