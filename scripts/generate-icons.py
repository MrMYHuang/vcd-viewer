#!/usr/bin/env python3
"""
Generate VCD Viewer app icon assets:
- assets/icon.svg (Vector master)
- assets/icon.png (512x512)
- assets/icon-256.png (256x256)
- assets/favicon.png (32x32)
- assets/icon.ico (Windows multi-resolution ICO)
- assets/favicon.ico (Web favicon ICO)
"""

import os
from PIL import Image, ImageDraw, ImageFilter

ASSETS_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets")
os.makedirs(ASSETS_DIR, exist_ok=True)

# 1. Generate SVG master
SVG_CONTENT = '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#1e222b"/>
      <stop offset="100%" stop-color="#12141a"/>
    </linearGradient>
    <filter id="cyanGlow" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur stdDeviation="8" result="blur"/>
      <feMerge>
        <feMergeNode in="blur"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
    <filter id="amberGlow" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur stdDeviation="7" result="blur"/>
      <feMerge>
        <feMergeNode in="blur"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </defs>

  <!-- Squircle Base -->
  <rect x="24" y="24" width="464" height="464" rx="104" ry="104" fill="url(#bgGrad)" stroke="#2f3746" stroke-width="6"/>

  <!-- Timing Grid Lines -->
  <line x1="96" y1="64" x2="96" y2="448" stroke="#252b37" stroke-width="2" stroke-dasharray="4,6"/>
  <line x1="176" y1="64" x2="176" y2="448" stroke="#252b37" stroke-width="2" stroke-dasharray="4,6"/>
  <line x1="256" y1="64" x2="256" y2="448" stroke="#2a3240" stroke-width="3" stroke-dasharray="6,6"/>
  <line x1="336" y1="64" x2="336" y2="448" stroke="#252b37" stroke-width="2" stroke-dasharray="4,6"/>
  <line x1="416" y1="64" x2="416" y2="448" stroke="#252b37" stroke-width="2" stroke-dasharray="4,6"/>

  <!-- Horizontal Guide Rails -->
  <line x1="64" y1="168" x2="448" y2="168" stroke="#222834" stroke-width="2"/>
  <line x1="64" y1="240" x2="448" y2="240" stroke="#222834" stroke-width="2"/>
  <line x1="64" y1="312" x2="448" y2="312" stroke="#222834" stroke-width="2"/>
  <line x1="64" y1="384" x2="448" y2="384" stroke="#222834" stroke-width="2"/>

  <!-- Trace 1: Neon Cyan Clock / Square Wave -->
  <path d="M 64 240 L 136 240 L 136 168 L 216 168 L 216 240 L 296 240 L 296 168 L 376 168 L 376 240 L 448 240"
        fill="none" stroke="#00e5ff" stroke-width="12" stroke-linecap="round" stroke-linejoin="miter" filter="url(#cyanGlow)"/>

  <!-- Trace 2: Amber Bus / Data Waveform with Hex Transitions -->
  <!-- Low level -->
  <path d="M 64 384 L 176 384 L 192 312 L 320 312 L 336 384 L 448 384"
        fill="none" stroke="#ffb300" stroke-width="10" stroke-linecap="round" stroke-linejoin="round" filter="url(#amberGlow)"/>
  <!-- Bus cross-over lines -->
  <path d="M 176 312 L 192 384 M 320 384 L 336 312"
        fill="none" stroke="#ffb300" stroke-width="8" stroke-linecap="round" stroke-linejoin="round" filter="url(#amberGlow)"/>

  <!-- Primary Cursor Marker (Red-Orange Line) -->
  <line x1="256" y1="112" x2="256" y2="400" stroke="#ff5252" stroke-width="4" stroke-linecap="round"/>
  <polygon points="250,112 262,112 256,124" fill="#ff5252"/>
</svg>
'''

with open(os.path.join(ASSETS_DIR, "icon.svg"), "w") as f:
    f.write(SVG_CONTENT)

# 2. Render pixel-perfect high-res raster image using Pillow
SIZE = 1024  # Render at 1024x1024 supersampled, then downsample for pristine crispness
img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# Rounded squircle background
padding = 48
radius = 210
# Outer subtle border
draw.rounded_rectangle(
    [padding, padding, SIZE - padding, SIZE - padding],
    radius=radius,
    fill=(25, 29, 37, 255),
    outline=(54, 63, 80, 255),
    width=12
)

# Dark gradient fill overlay
inner_pad = padding + 12
inner_rad = radius - 8
draw.rounded_rectangle(
    [inner_pad, inner_pad, SIZE - inner_pad, SIZE - inner_pad],
    radius=inner_rad,
    fill=(20, 23, 30, 255)
)

# Grid lines
grid_x_coords = [192, 352, 512, 672, 832]
for x in grid_x_coords:
    is_center = (x == 512)
    col = (48, 58, 74, 255) if is_center else (36, 42, 54, 200)
    w = 6 if is_center else 4
    # Dashed vertical line
    y = 140
    while y < 884:
        draw.line([(x, y), (x, min(y + 20, 884))], fill=col, width=w)
        y += 36

# Guide rails
rail_y = [336, 480, 624, 768]
for y in rail_y:
    draw.line([(128, y), (SIZE - 128, y)], fill=(32, 38, 48, 180), width=4)

# Create a separate glow layer for traces
glow_layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
glow_draw = ImageDraw.Draw(glow_layer)

# Clock square wave coordinates (Cyan)
# High: y=336, Low: y=480
cyan_pts = [
    (128, 480), (272, 480),
    (272, 336), (432, 336),
    (432, 480), (592, 480),
    (592, 336), (752, 336),
    (752, 480), (896, 480)
]

# Amber bus trace coordinates
# High: y=624, Low: y=768
amber_top = [(128, 768), (352, 768), (384, 624), (640, 624), (672, 768), (896, 768)]
amber_cross1 = [(352, 624), (384, 768)]
amber_cross2 = [(640, 768), (672, 624)]
amber_bot = [(384, 768), (640, 768)]

# Draw glows
glow_draw.line(cyan_pts, fill=(0, 229, 255, 180), width=36, joint="miter")
glow_draw.line(amber_top, fill=(255, 179, 0, 160), width=32, joint="round")
glow_draw.line(amber_cross1, fill=(255, 179, 0, 160), width=28)
glow_draw.line(amber_cross2, fill=(255, 179, 0, 160), width=28)
glow_draw.line(amber_bot, fill=(255, 179, 0, 160), width=28)

blurred_glow = glow_layer.filter(ImageFilter.GaussianBlur(radius=24))
img.alpha_composite(blurred_glow)

# Draw crisp solid traces on main image
draw.line(cyan_pts, fill=(0, 229, 255, 255), width=22, joint="miter")
# Bright inner core highlight for cyan
draw.line(cyan_pts, fill=(200, 250, 255, 230), width=8, joint="miter")

draw.line(amber_top, fill=(255, 179, 0, 255), width=20, joint="round")
draw.line(amber_cross1, fill=(255, 179, 0, 255), width=18)
draw.line(amber_cross2, fill=(255, 179, 0, 255), width=18)
draw.line(amber_bot, fill=(255, 179, 0, 255), width=18)
# Highlight for amber
draw.line(amber_top, fill=(255, 236, 179, 220), width=6, joint="round")

# Cursor marker (Red/Coral) at x=512
draw.line([(512, 220), (512, 800)], fill=(255, 82, 82, 255), width=8)
# Cursor arrowhead at top
draw.polygon([(498, 220), (526, 220), (512, 248)], fill=(255, 82, 82, 255))

# Save 512x512 master PNG
img_512 = img.resize((512, 512), Image.Resampling.LANCZOS)
img_512.save(os.path.join(ASSETS_DIR, "icon.png"), "PNG")

# Save 256x256 native desktop icon
img_256 = img.resize((256, 256), Image.Resampling.LANCZOS)
img_256.save(os.path.join(ASSETS_DIR, "icon-256.png"), "PNG")

# Save 32x32 web favicon PNG
img_32 = img.resize((32, 32), Image.Resampling.LANCZOS)
img_32.save(os.path.join(ASSETS_DIR, "favicon.png"), "PNG")

# Multi-resolution Windows ICO
ico_sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
img.save(
    os.path.join(ASSETS_DIR, "icon.ico"),
    format="ICO",
    sizes=ico_sizes
)

# Web favicon ICO
img_32.save(
    os.path.join(ASSETS_DIR, "favicon.ico"),
    format="ICO",
    sizes=[(16, 16), (32, 32)]
)

print(f"Generated icon assets in {ASSETS_DIR}:")
for f in sorted(os.listdir(ASSETS_DIR)):
    print(f" - {f} ({os.path.getsize(os.path.join(ASSETS_DIR, f))} bytes)")
