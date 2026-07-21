#!/usr/bin/env python3
# mise description="Generate dist/<name>/index.html for a game"
import os, sys
from string import Template

name = sys.argv[1]
gtag_id = os.environ.get("GTAG_ID", "")
title = name.removeprefix("game").replace("-", " ").title()

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
  <link rel="icon" href="../favicon.svg" type="image/svg+xml">
  <title>$TITLE</title>
$GTAG_LOADER
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { background: #000; overflow: hidden; }
    canvas { display: block; width: 100vw; height: 100vh; }
  </style>
</head>
<body>
  <canvas id="glcanvas" tabindex="1"></canvas>
  <script src="mq_js_bundle.js"></script>
  <script>load("$NAME.wasm");</script>
$GTAG_CONFIG
</body>
</html>
""").safe_substitute(TITLE=title, NAME=name, GTAG_LOADER=gtag_loader, GTAG_CONFIG=gtag_config)

with open(f"dist/{name}/index.html", "w") as f:
    f.write(page)
