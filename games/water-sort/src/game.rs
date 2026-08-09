use std::collections::{HashSet, VecDeque};

/// Units per bottle. Fixed across every level (unlike colors/bottle-count, which scale
/// with `level`) — a taller bottle would need its own render-layout tuning for no real
/// difficulty payoff, since color count and bottle count already carry the scaling.
pub const CAPACITY: usize = 4;

pub type Color = u8;

#[derive(Clone, Debug)]
pub struct Bottle {
    /// Bottom-to-top stack, length 0..=CAPACITY.
    pub liquid: Vec<Color>,
    /// `Some(color)` if this bottle starts locked, unlockable once some *other* bottle
    /// is fully sorted (`is_solved_bottle`) to `color`. Never `Some` of a color present
    /// in this bottle's own initial `liquid` — see `game.rs`'s generation code for why
    /// that invariant matters (it's what makes the unlock reachable without touching
    /// this bottle at all, avoiding an unlock deadlock between two locked bottles).
    pub lock_target: Option<Color>,
    /// Sticky: flips false->true the moment `lock_target` is satisfied and never flips
    /// back, even if the qualifying bottle is later disturbed (it can't be — see
    /// `is_solved_bottle`'s callers, a solved bottle is never a legal pour source).
    pub unlocked: bool,
    /// Bottom `fog` slots are visually obscured (see `main.rs`'s bottle rendering)
    /// until this bottle's liquid drains down to `fog` units or fewer, at which point
    /// every remaining unit *is* one of the originally-hidden ones and `apply_pour`
    /// zeroes this field for good — a one-way reveal, not a live comparison, so a
    /// later pour that fills this same bottle back past the old threshold can't
    /// re-cover units the viewer already saw. Purely cosmetic otherwise — never
    /// consulted by any legality/win rule in this file.
    pub fog: usize,
}

impl Bottle {
    fn empty() -> Self {
        Bottle {
            liquid: Vec::new(),
            lock_target: None,
            unlocked: false,
            fog: 0,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.lock_target.is_some() && !self.unlocked
    }

    /// Full to capacity and every unit the same color. A solved bottle is sealed: it
    /// can never again be a legal pour source (see `Game::legal_moves`), so once this
    /// is true for a bottle it stays true for the rest of the episode.
    pub fn is_solved(&self) -> bool {
        self.liquid.len() == CAPACITY && self.liquid.iter().all(|&c| c == self.liquid[0])
    }

    /// How many units, counting down from the top, share the top color. 0 if empty.
    pub fn top_run(&self) -> usize {
        let Some(&top) = self.liquid.last() else {
            return 0;
        };
        self.liquid.iter().rev().take_while(|&&c| c == top).count()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    Won,
    Stuck,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Move {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone)]
pub struct Game {
    pub bottles: Vec<Bottle>,
    pub colors: usize,
    /// 1-indexed, unbounded — see `LevelParams` for how difficulty scales with it.
    pub level: u32,
    pub generation: u32,
    pub moves: u32,
    pub phase: Phase,
}

/// Difficulty knobs derived purely from `level`. Colors cap at `MAX_COLORS` (the render
/// palette's limit), but bottle count, lock count, and fog depth keep climbing past that
/// cap, so difficulty never plateaus even once the color palette does — the puzzle stays
/// "never-ending" in spirit, not just in level-counter number.
pub struct LevelParams {
    pub colors: usize,
    pub empty: usize,
    pub locked: usize,
    pub fog_depth: usize,
}

const MAX_COLORS: usize = 12;
const MAX_EMPTY: usize = 4;
const MAX_LOCKED: usize = 3;
/// Locking needs at least one color guaranteed absent from every locked bottle's own
/// contents (see `lock_target`'s doc comment) — only safe once a bottle (capacity
/// `CAPACITY` slots) can't possibly span the whole color palette.
const LOCK_MIN_COLORS: usize = CAPACITY + 2;

impl LevelParams {
    pub fn for_level(level: u32) -> Self {
        let l = level.saturating_sub(1);
        let colors = (3 + l / 3).min(MAX_COLORS as u32) as usize;
        let empty = 2 + (l / 6).min(MAX_EMPTY as u32 - 2) as usize;
        let locked = if colors >= LOCK_MIN_COLORS {
            (1 + l.saturating_sub(27) / 10).min(MAX_LOCKED as u32) as usize
        } else {
            0
        };
        let fog_depth = if level < 16 {
            0
        } else if level < 40 {
            1
        } else {
            2
        };
        LevelParams {
            colors,
            empty,
            locked,
            fog_depth,
        }
    }
}

/// Bounded BFS over real gameplay (locks/fog included, exactly the rules the solver
/// plays under) used only at generation time to bias toward a winnable deal — not a
/// gameplay feature. Same spirit as sudoku's `solve_count`-gated `dig`: a cheap
/// probabilistic filter, not a proof. `false` means "didn't find a win within budget",
/// which includes both "actually unsolvable" and "solvable but deeper than we looked" —
/// generation retries a bounded number of times and accepts the last attempt regardless
/// (see `Game::new`), so an unlucky false negative just costs a wasted retry, never a
/// hang, matching every other solver-family game's acceptance that a deal *can* end up
/// genuinely unsolvable (klondike/spider's `Phase::Stuck` is the same acceptance).
fn bfs_solvable(start: &Game, node_budget: usize) -> bool {
    let mut visited: HashSet<u64> = HashSet::new();
    let mut queue: VecDeque<Game> = VecDeque::new();
    visited.insert(start.state_hash());
    queue.push_back(start.clone());
    let mut expanded = 0usize;

    while let Some(g) = queue.pop_front() {
        if g.is_won() {
            return true;
        }
        if expanded >= node_budget {
            return false;
        }
        for m in g.legal_moves() {
            let mut next = g.clone();
            next.apply_pour(m);
            let h = next.state_hash();
            if visited.insert(h) {
                expanded += 1;
                if next.is_won() {
                    return true;
                }
                if expanded < node_budget {
                    queue.push_back(next);
                }
            }
        }
    }
    false
}

const GENERATION_ATTEMPTS: usize = 10;
const SOLVABILITY_BUDGET: usize = 12_000;

impl Game {
    pub fn new(level: u32, generation: u32) -> Self {
        let mut best = Self::deal(level, generation);
        for attempt in 1..GENERATION_ATTEMPTS {
            // Solvability is checked against the deal *before* locks are applied
            // (`deal_unlocked`) — reasoning through lock-unlock ordering inside the BFS
            // itself would make this check far more expensive for only a marginal
            // accuracy gain, since `lock_target` is already chosen (see `apply_locks`)
            // so that every lock is satisfiable using only never-locked bottles.
            let probe = Self::deal_unlocked(level, generation.wrapping_add(attempt as u32));
            if bfs_solvable(&probe, SOLVABILITY_BUDGET) {
                best = Self::apply_locks(probe, level, generation.wrapping_add(attempt as u32));
                break;
            }
        }
        best
    }

    fn deal(level: u32, generation: u32) -> Self {
        let unlocked = Self::deal_unlocked(level, generation);
        Self::apply_locks(unlocked, level, generation)
    }

    /// The scramble itself: shuffle a multiset of `colors` colors x `CAPACITY` units
    /// each, deal `CAPACITY`-sized consecutive chunks into `colors` filled bottles,
    /// append `empty` empty ones. No lock/fog applied yet — see `apply_locks` and the
    /// fog pass below, both layered on afterward so this function is the single
    /// solvability-check subject (see `bfs_solvable`'s doc comment).
    fn deal_unlocked(level: u32, generation: u32) -> Self {
        let params = LevelParams::for_level(level);
        let mut pool: Vec<Color> = (0..params.colors)
            .flat_map(|c| std::iter::repeat_n(c as Color, CAPACITY))
            .collect();
        shuffle(&mut pool);

        let mut bottles: Vec<Bottle> = pool
            .chunks(CAPACITY)
            .map(|chunk| Bottle {
                liquid: chunk.to_vec(),
                lock_target: None,
                unlocked: false,
                fog: 0,
            })
            .collect();
        for _ in 0..params.empty {
            bottles.push(Bottle::empty());
        }

        // Fog: applied to a random half of the filled, would-be-unlocked bottles (locks
        // get decided afterward in `apply_locks`, but fog doesn't interact with locking
        // at all, so it's fine to roll it here before that exists).
        if params.fog_depth > 0 {
            for b in bottles.iter_mut() {
                if b.liquid.len() > params.fog_depth && gen_bool(0.5) {
                    b.fog = params.fog_depth;
                }
            }
        }

        Game {
            bottles,
            colors: params.colors,
            level,
            generation,
            moves: 0,
            phase: Phase::Playing,
        }
    }

    fn apply_locks(mut game: Game, level: u32, generation: u32) -> Self {
        let params = LevelParams::for_level(level);
        if params.locked == 0 {
            return game;
        }
        let filled_idx: Vec<usize> = (0..params.colors).collect();
        let mut candidates = filled_idx.clone();
        shuffle_indices(&mut candidates, generation);
        let locked_indices: Vec<usize> = candidates.into_iter().take(params.locked).collect();

        // A color is a safe lock target only if it appears in *no* locked bottle's own
        // contents (own or any other locked bottle's) — otherwise completing it could
        // require pouring out of a still-locked bottle, or two locks could form an
        // unlock cycle. See `Bottle::lock_target`'s doc comment.
        let mut trapped: HashSet<Color> = HashSet::new();
        for &i in &locked_indices {
            trapped.extend(game.bottles[i].liquid.iter().copied());
        }
        let free_colors: Vec<Color> = (0..params.colors as Color)
            .filter(|c| !trapped.contains(c))
            .collect();
        if free_colors.is_empty() {
            return game;
        }

        for (n, &i) in locked_indices.iter().enumerate() {
            let target = free_colors[n % free_colors.len()];
            game.bottles[i].lock_target = Some(target);
        }
        game
    }

    pub fn is_locked(&self, i: usize) -> bool {
        self.bottles[i].is_locked()
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let n = self.bottles.len();
        let mut moves = Vec::new();
        for from in 0..n {
            let src = &self.bottles[from];
            if src.liquid.is_empty() || src.is_solved() || self.is_locked(from) {
                continue;
            }
            let top = *src.liquid.last().unwrap();
            for to in 0..n {
                if to == from || self.is_locked(to) {
                    continue;
                }
                let dst = &self.bottles[to];
                if dst.liquid.len() >= CAPACITY {
                    continue;
                }
                if !dst.liquid.is_empty() && *dst.liquid.last().unwrap() != top {
                    continue;
                }
                moves.push(Move { from, to });
            }
        }
        moves
    }

    pub fn pour_amount(&self, m: Move) -> usize {
        let run = self.bottles[m.from].top_run();
        let room = CAPACITY - self.bottles[m.to].liquid.len();
        run.min(room)
    }

    /// Pure state transition — no phase/win bookkeeping (used by `bfs_solvable`'s
    /// hypothetical search, which reads `is_won` itself). `apply` (below) is the real
    /// gameplay entry point and layers phase transitions and `moves` on top of this.
    fn apply_pour(&mut self, m: Move) {
        let amount = self.pour_amount(m);
        let top = *self.bottles[m.from].liquid.last().unwrap();
        let new_len = self.bottles[m.from].liquid.len() - amount;
        self.bottles[m.from].liquid.truncate(new_len);
        for _ in 0..amount {
            self.bottles[m.to].liquid.push(top);
        }
        // Fog reveal is sticky: once a pour drains `from` down to (or below) its own
        // `fog` count, those bottom units are revealed for good. Without clearing
        // `fog` here (not just comparing it against the live length at render time),
        // a later pour that fills this same bottle back up past the threshold would
        // re-cover units the viewer already saw — see `Bottle::fog`'s doc comment,
        // "already-revealed" is a one-way state, not a live length comparison.
        let from = &mut self.bottles[m.from];
        if from.fog > 0 && from.liquid.len() <= from.fog {
            from.fog = 0;
        }
        self.refresh_locks();
    }

    /// Sticky unlock scan: any locked bottle whose target color is now fully sorted
    /// into some other bottle unlocks permanently. Cheap (bottle count is small) and
    /// safe to call after every pour, not just at generation time.
    fn refresh_locks(&mut self) {
        let solved_colors: HashSet<Color> = self
            .bottles
            .iter()
            .filter(|b| b.is_solved())
            .map(|b| b.liquid[0])
            .collect();
        for b in self.bottles.iter_mut() {
            if let Some(target) = b.lock_target
                && !b.unlocked
                && solved_colors.contains(&target)
            {
                b.unlocked = true;
            }
        }
    }

    pub fn is_won(&self) -> bool {
        self.bottles
            .iter()
            .all(|b| b.liquid.is_empty() || b.is_solved())
    }

    pub fn apply(&mut self, m: Move) {
        self.apply_pour(m);
        self.moves += 1;
        if self.is_won() {
            self.phase = Phase::Won;
        } else if self.legal_moves().is_empty() {
            self.phase = Phase::Stuck;
        }
    }

    pub fn state_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for b in &self.bottles {
            b.liquid.hash(&mut h);
            b.lock_target.hash(&mut h);
            b.unlocked.hash(&mut h);
            0xFFu8.hash(&mut h); // separator between bottles
        }
        h.finish()
    }
}

fn gen_bool(p: f64) -> bool {
    (macroquad::rand::gen_range(0.0f64, 1.0f64)) < p
}

fn shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = macroquad::rand::gen_range(0, i as u32 + 1) as usize;
        v.swap(i, j);
    }
}

/// A local, seed-derived shuffle for lock-candidate selection — deliberately not the
/// ambient `macroquad::rand` stream (used by `deal_unlocked`'s color shuffle and fog
/// coin-flips): `Game::new` calls `deal_unlocked` a variable number of times (one per
/// retry) depending on solvability, so if lock selection also drew from the shared
/// stream, restarting the *same* level generation with a different number of retries
/// would perturb every subsequent level's colors/fog draws too. Deriving purely from
/// `generation` keeps lock selection reproducible independent of retry count.
fn shuffle_indices(v: &mut [usize], seed: u32) {
    let mut state = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
    for i in (1..v.len()).rev() {
        state = state.wrapping_mul(0x2545_F491_4F6C_DD1D).wrapping_add(1);
        let j = (state >> 33) as usize % (i + 1);
        v.swap(i, j);
    }
}
