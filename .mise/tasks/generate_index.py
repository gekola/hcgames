#!/usr/bin/env python3
# mise description="Generate root index.html listing all games"
import os

games = sorted(d for d in os.listdir("dist") if os.path.isdir(f"dist/{d}"))
rows = "\n".join(
    f'    <li><a href="{g}/">{g.replace("-", " ").title()}</a></li>'
    for g in games
)

page = (
    '<!DOCTYPE html>\n'
    '<html lang="en">\n'
    '<head>\n'
    '  <meta charset="utf-8">\n'
    '  <title>Hotel Chair Games</title>\n'
    '  <style>\n'
    '    body { font-family: system-ui, sans-serif; background: #111827; color: #e5e7eb;\n'
    '           max-width: 480px; margin: 8rem auto; padding: 0 1.5rem; }\n'
    '    h1   { font-size: 2rem; font-weight: 700; margin-bottom: 2rem; color: #f9fafb; }\n'
    '    ul   { list-style: none; padding: 0; display: flex; flex-direction: column; gap: .75rem; }\n'
    '    a    { display: block; padding: .75rem 1rem; border-radius: .5rem;\n'
    '           background: #1f2937; color: #93c5fd; text-decoration: none;\n'
    '           border: 1px solid #374151; transition: background .15s; }\n'
    '    a:hover { background: #374151; color: #bfdbfe; }\n'
    '  </style>\n'
    '</head>\n'
    '<body>\n'
    '  <h1>Hotel Chair Games</h1>\n'
    '  <ul>\n'
    + rows + '\n'
    '  </ul>\n'
    '</body>\n'
    '</html>\n'
)

with open("dist/index.html", "w") as f:
    f.write(page)
print(f"wrote dist/index.html ({len(games)} game(s))")
