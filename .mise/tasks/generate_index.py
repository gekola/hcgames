#!/usr/bin/env python3
# mise description="Generate root index.html listing all games"
import os
from string import Template

gtag_id = os.environ.get("GTAG_ID", "")

games = sorted(d for d in os.listdir("dist") if os.path.isdir(f"dist/{d}"))
rows = "\n".join(
    f'    <a class="game-link" href="{g}/">{g.removeprefix("game").replace("-", " ").title()}</a>'
    for g in games
)

if gtag_id:
    gtag_loader = f'  <script async src="https://www.googletagmanager.com/gtag/js?id={gtag_id}"></script>'
    gtag_config = f"""<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){{dataLayer.push(arguments);}}
  gtag('js', new Date());

  gtag('config', '{gtag_id}');
</script>"""
else:
    gtag_loader = ""
    gtag_config = ""

page = Template("""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Hotel Chair Games</title>
$GTAG_LOADER
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }

    body {
      background: #111827;
      color: #e5e7eb;
      font-family: system-ui, sans-serif;
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
    }

    header {
      padding: 3rem 1rem 1rem;
      text-align: center;
    }

    header h1 {
      font-size: 2rem;
      font-weight: 700;
      color: #f9fafb;
    }

    .main {
      display: flex;
      gap: 3rem;
      align-items: flex-start;
      max-width: 960px;
      width: 95%;
      margin: 1.5rem 0 3rem;
    }

    .scene-wrap { flex-shrink: 0; }

    #hotel {
      display: block;
      image-rendering: pixelated;
      image-rendering: crisp-edges;
    }

    .quotes {
      flex: 1;
      border-left: 2px solid #374151;
      padding-left: 1.5rem;
      padding-top: 0.5rem;
      display: flex;
      flex-direction: column;
      gap: 1.8rem;
    }

    .quotes blockquote {
      font-size: 0.9rem;
      line-height: 1.7;
      color: #9ca3af;
      font-style: italic;
    }

    .quotes blockquote .speaker {
      display: block;
      margin-top: 0.4rem;
      font-style: normal;
      font-size: 0.72rem;
      color: #4b5563;
    }

    .games {
      margin-top: 1.5rem;
    }

    .games h2 {
      font-size: 0.75rem;
      letter-spacing: 0.15em;
      color: #4b5563;
      text-transform: uppercase;
      margin-bottom: 1rem;
      font-weight: normal;
    }

    .game-link {
      display: block;
      padding: 0.75rem 1rem;
      border-radius: 0.5rem;
      background: #1f2937;
      color: #93c5fd;
      text-decoration: none;
      border: 1px solid #374151;
      transition: background 0.15s;
      margin-bottom: 0.5rem;
    }

    .game-link:hover { background: #374151; color: #bfdbfe; }
  </style>
</head>
<body>

<header>
  <h1>Hotel Chair Games</h1>
</header>

<div class="main">
  <div class="scene-wrap">
    <canvas id="hotel" width="480" height="360"></canvas>
    <section class="games">
      <h2>games</h2>
$GAME_LINKS
    </section>
  </div>

  <section class="quotes">
    <blockquote>
      "Gaming is solved."
      <cite class="speaker">— a man in a hoodie who discovered Pong last Tuesday</cite>
    </blockquote>
    <blockquote>
      "I haven't pressed a button in weeks. I just describe my intended gameplay and the AI plays for me."
      <cite class="speaker">— a thought leader on the future of fun</cite>
    </blockquote>
    <blockquote>
      "Human players will be completely obsolete within 18 months. We'll only need play prompters."
      <cite class="speaker">— someone who has never finished a game in their life</cite>
    </blockquote>
    <blockquote>
      "Beating games yourself is just stubbornness now. The AI has already seen the credits."
      <cite class="speaker">— posted from a hotel room at a gaming conference</cite>
    </blockquote>
    <blockquote>
      "The era of human gameplay is over. These are the last games played by hand."
      <cite class="speaker">— a VC who just funded an AI esports team</cite>
    </blockquote>
    <blockquote>
      "No one will need gamers in 6 months."
      <cite class="speaker">— someone who has never finished a game in their life</cite>
    </blockquote>
  </section>
</div>

<script>
(function () {
  const canvas = document.getElementById('hotel');
  const ctx = canvas.getContext('2d');
  ctx.imageSmoothingEnabled = false;

  const W = 80, H = 60;
  const off = document.createElement('canvas');
  off.width = W; off.height = H;
  const c = off.getContext('2d');
  c.imageSmoothingEnabled = false;

  function px(x, y, w, h, col) {
    c.fillStyle = col;
    c.fillRect(Math.round(x), Math.round(y), Math.round(w), Math.round(h));
  }

  // wall + floor
  px(0, 0, W, 42, '#b89468');
  px(0, 0, W,  2, '#a08258');
  px(0, 41, W,  1, '#8a7048');
  for (let y = 42; y < H; y++) {
    px(0, y, W, 1, y % 4 < 2 ? '#7a5e1e' : '#6e5418');
  }
  for (let x = 6; x < W; x += 10) { px(x, 42, 1, H - 42, '#5a4010'); }

  // curtain rod
  px(8, 3, 54, 1, '#c0a060');
  px(8, 4, 54, 1, '#907040');
  px( 7, 2, 3, 3, '#c8a858');
  px(60, 2, 3, 3, '#c8a858');

  // left curtain
  const lx = 10;
  px(lx,     5, 12, 37, '#1e3e5e');
  px(lx+1,   5,  1, 37, '#2a5070');
  px(lx+4,   5,  1, 37, '#2a5070');
  px(lx+8,   5,  1, 37, '#2a5070');
  px(lx+2,   5,  1, 37, '#162e48');
  px(lx+6,   5,  1, 37, '#162e48');
  px(lx+10,  5,  1, 37, '#162e48');
  px(lx+9,  36,  3,  6, '#2a5070');
  px(lx+10, 38,  2,  4, '#162e48');

  // right curtain
  const rx = 48;
  px(rx,     5, 12, 37, '#1e3e5e');
  px(rx+1,   5,  1, 37, '#2a5070');
  px(rx+4,   5,  1, 37, '#2a5070');
  px(rx+8,   5,  1, 37, '#2a5070');
  px(rx+2,   5,  1, 37, '#162e48');
  px(rx+6,   5,  1, 37, '#162e48');
  px(rx+10,  5,  1, 37, '#162e48');
  px(rx,    36,  3,  6, '#2a5070');
  px(rx+1,  38,  2,  4, '#162e48');

  // window
  px(22,  5, 26, 32, '#3a2818');
  px(24,  7, 22, 28, '#7ab0d4');
  px(25,  8,  9, 10, '#8ec0e0');
  px(37,  8,  9, 10, '#8ec0e0');
  px(25, 20,  9, 14, '#6898b8');
  px(37, 20,  9, 14, '#6898b8');
  px(34,  7,  2, 28, '#3a2818');
  px(24, 19, 22,  2, '#3a2818');
  px(21, 36, 28,  3, '#4a3820');
  px(22, 37, 26,  2, '#6a5030');
  px(46,  7,  2, 28, '#c8a87a');
  px(22, 35, 22,  2, '#c0a070');

  // ── armchair — side profile facing right (toward bed), drawn first ────────
  const cx = 21;
  // chair back: tall post on the left
  px(cx,    32,  4, 21, '#5c1e0e');
  px(cx+1,  33,  2, 19, '#782a18');
  // tufting buttons (side view)
  px(cx+1,  37,  1,  2, '#8a3020');
  px(cx+1,  42,  1,  2, '#8a3020');
  // armrest rail (horizontal, connects back to seat front)
  px(cx,    44, 14,  2, '#501808');
  px(cx+1,  44, 13,  1, '#6a2010');
  // seat cushion
  px(cx+3,  46, 11,  8, '#702818');
  px(cx+4,  47,  9,  6, '#8a3020');
  px(cx+4,  47,  9,  1, '#9a3828');
  px(cx+4,  51,  9,  1, '#602010');
  // seat front face
  px(cx+3,  54, 11,  2, '#5a2010');
  // legs
  px(cx+3,  55,  2,  3, '#3a1008');
  px(cx+11, 55,  2,  3, '#3a1008');
  // floor shadow (left portion only, right hidden by bed)
  c.fillStyle = 'rgba(0,0,0,0.15)';
  c.fillRect(cx, 57, 12, 3);

  // ── bed — drawn on top of chair ──────────────────────────────────────────
  const bx = 44;
  // headboard (original proportions: 29px wide)
  px(bx,    29, 29, 15, '#3e2010');
  px(bx+1,  30, 27, 13, '#6a3818');
  px(bx+2,  31, 25, 11, '#5a3010');
  // headboard panels (symmetric)
  px(bx+3,  32, 10,  9, '#7a4020');
  px(bx+15, 32, 10,  9, '#7a4020');
  px(bx+4,  33,  8,  7, '#8a4c28');
  px(bx+16, 33,  8,  7, '#8a4c28');
  px(bx+3,  32,  1,  9, '#4a2810');
  px(bx+15, 32,  1,  9, '#4a2810');
  // frame side
  px(bx,    43, 29, 14, '#4a3018');
  px(bx+1,  44, 27, 12, '#5a3c20');
  // duvet
  px(bx+1,  39, 27, 18, '#ddd5c0');
  px(bx+2,  40, 25, 16, '#ede6d4');
  px(bx+2,  43, 25,  1, '#c8c0a8');
  px(bx+2,  47, 25,  1, '#c8c0a8');
  px(bx+2,  51, 25,  1, '#c8c0a8');
  px(bx+1,  40,  1, 17, '#c0b898');
  px(bx+27, 40,  1, 17, '#c0b898');
  // pillows
  px(bx+2,  38, 11,  7, '#f0ece2');
  px(bx+15, 38, 11,  7, '#f0ece2');
  px(bx+3,  39,  9,  5, '#ffffff');
  px(bx+16, 39,  9,  5, '#ffffff');
  // legs
  px(bx+2,  54,  2,  3, '#2e1808');
  px(bx+26, 54,  2,  3, '#2e1808');
  // floor shadow
  c.fillStyle = 'rgba(0,0,0,0.18)';
  c.fillRect(bx+1, 57, 27, 3);

  ctx.drawImage(off, 0, 0, canvas.width, canvas.height);
})();
</script>

$GTAG_CONFIG

</body>
</html>
""").safe_substitute(GTAG_LOADER=gtag_loader, GAME_LINKS=rows, GTAG_CONFIG=gtag_config)

with open("dist/index.html", "w") as f:
    f.write(page)
print(f"wrote dist/index.html ({len(games)} game(s))")
