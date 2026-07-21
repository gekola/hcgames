#!/usr/bin/env python3
# mise description="Generate static/hotel-scene.svg — source of the OG preview image"
#
# Mirrors the pixel-art hotel room scene drawn live on the homepage canvas
# (see the <script> block in generate_index.py). Kept as a separate static
# SVG so it can be rasterized to a real image for social-preview meta tags,
# which crawlers can't render from JS canvas.

W, H = 80, 60

RECTS = []


def px(x, y, w, h, col):
    RECTS.append((x, y, w, h, col))


# wall + floor
px(0, 0, W, 42, "#b89468")
px(0, 0, W, 2, "#a08258")
px(0, 41, W, 1, "#8a7048")
for y in range(42, H):
    px(0, y, W, 1, "#7a5e1e" if y % 4 < 2 else "#6e5418")
for x in range(6, W, 10):
    px(x, 42, 1, H - 42, "#5a4010")

# curtain rod
px(8, 3, 54, 1, "#c0a060")
px(8, 4, 54, 1, "#907040")
px(7, 2, 3, 3, "#c8a858")
px(60, 2, 3, 3, "#c8a858")

# left curtain
lx = 10
px(lx, 5, 12, 37, "#1e3e5e")
px(lx + 1, 5, 1, 37, "#2a5070")
px(lx + 4, 5, 1, 37, "#2a5070")
px(lx + 8, 5, 1, 37, "#2a5070")
px(lx + 2, 5, 1, 37, "#162e48")
px(lx + 6, 5, 1, 37, "#162e48")
px(lx + 10, 5, 1, 37, "#162e48")
px(lx + 9, 36, 3, 6, "#2a5070")
px(lx + 10, 38, 2, 4, "#162e48")

# right curtain
rx = 48
px(rx, 5, 12, 37, "#1e3e5e")
px(rx + 1, 5, 1, 37, "#2a5070")
px(rx + 4, 5, 1, 37, "#2a5070")
px(rx + 8, 5, 1, 37, "#2a5070")
px(rx + 2, 5, 1, 37, "#162e48")
px(rx + 6, 5, 1, 37, "#162e48")
px(rx + 10, 5, 1, 37, "#162e48")
px(rx, 36, 3, 6, "#2a5070")
px(rx + 1, 38, 2, 4, "#162e48")

# window
px(22, 5, 26, 32, "#3a2818")
px(24, 7, 22, 28, "#7ab0d4")
px(25, 8, 9, 10, "#8ec0e0")
px(37, 8, 9, 10, "#8ec0e0")
px(25, 20, 9, 14, "#6898b8")
px(37, 20, 9, 14, "#6898b8")
px(34, 7, 2, 28, "#3a2818")
px(24, 19, 22, 2, "#3a2818")
px(21, 36, 28, 3, "#4a3820")
px(22, 37, 26, 2, "#6a5030")
px(46, 7, 2, 28, "#c8a87a")
px(22, 35, 22, 2, "#c0a070")

# armchair — side profile facing right (toward bed), drawn first
cx = 21
px(cx, 32, 4, 21, "#5c1e0e")
px(cx + 1, 33, 2, 19, "#782a18")
px(cx + 1, 37, 1, 2, "#8a3020")
px(cx + 1, 42, 1, 2, "#8a3020")
px(cx, 44, 14, 2, "#501808")
px(cx + 1, 44, 13, 1, "#6a2010")
px(cx + 3, 46, 11, 8, "#702818")
px(cx + 4, 47, 9, 6, "#8a3020")
px(cx + 4, 47, 9, 1, "#9a3828")
px(cx + 4, 51, 9, 1, "#602010")
px(cx + 3, 54, 11, 2, "#5a2010")
px(cx + 3, 55, 2, 3, "#3a1008")
px(cx + 11, 55, 2, 3, "#3a1008")
px(cx, 57, 12, 3, "rgba(0,0,0,0.15)")

# bed — drawn on top of chair
bx = 44
px(bx, 29, 29, 15, "#3e2010")
px(bx + 1, 30, 27, 13, "#6a3818")
px(bx + 2, 31, 25, 11, "#5a3010")
px(bx + 3, 32, 10, 9, "#7a4020")
px(bx + 15, 32, 10, 9, "#7a4020")
px(bx + 4, 33, 8, 7, "#8a4c28")
px(bx + 16, 33, 8, 7, "#8a4c28")
px(bx + 3, 32, 1, 9, "#4a2810")
px(bx + 15, 32, 1, 9, "#4a2810")
px(bx, 43, 29, 14, "#4a3018")
px(bx + 1, 44, 27, 12, "#5a3c20")
px(bx + 1, 39, 27, 18, "#ddd5c0")
px(bx + 2, 40, 25, 16, "#ede6d4")
px(bx + 2, 43, 25, 1, "#c8c0a8")
px(bx + 2, 47, 25, 1, "#c8c0a8")
px(bx + 2, 51, 25, 1, "#c8c0a8")
px(bx + 1, 40, 1, 17, "#c0b898")
px(bx + 27, 40, 1, 17, "#c0b898")
px(bx + 2, 38, 11, 7, "#f0ece2")
px(bx + 15, 38, 11, 7, "#f0ece2")
px(bx + 3, 39, 9, 5, "#ffffff")
px(bx + 16, 39, 9, 5, "#ffffff")
px(bx + 2, 54, 2, 3, "#2e1808")
px(bx + 26, 54, 2, 3, "#2e1808")
px(bx + 1, 57, 27, 3, "rgba(0,0,0,0.18)")

rects_svg = "\n".join(
    f'  <rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{col}"/>'
    for x, y, w, h, col in RECTS
)

svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" shape-rendering="crispEdges">
  <rect x="0" y="0" width="{W}" height="{H}" fill="#111827"/>
{rects_svg}
</svg>
"""

with open("static/hotel-scene.svg", "w") as f:
    f.write(svg)
print("wrote static/hotel-scene.svg")
