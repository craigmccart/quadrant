"""Generate WinScan app icon (winscan.ico) — run once, then delete this file."""

from PIL import Image, ImageDraw, ImageFont
import math

SIZES = [256, 128, 64, 48, 32, 16]
ACCENT = (91, 141, 238)       # #5b8dee
BG     = (13, 13, 26)         # #0d0d1a
SURFACE= (20, 20, 42)         # #14142a
WHITE  = (232, 232, 240)      # #e8e8f0
GREEN  = (61, 220, 132)       # #3ddc84

def draw_icon(size: int) -> Image.Image:
    """Draw the WinScan icon at a given size."""
    s = size
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Rounded-rect background
    pad = max(1, s // 32)
    r = s // 5
    d.rounded_rectangle([pad, pad, s - pad, s - pad], radius=r, fill=BG)

    cx, cy = s * 0.42, s * 0.44

    # Hard drive body — rounded rectangle
    dw, dh = s * 0.44, s * 0.28
    dx, dy = cx - dw / 2, cy - dh / 2 + s * 0.04
    drive_r = max(2, s // 20)
    d.rounded_rectangle(
        [dx, dy, dx + dw, dy + dh],
        radius=drive_r, fill=SURFACE, outline=ACCENT, width=max(1, s // 64)
    )

    # Drive slot line
    slot_y = dy + dh * 0.65
    slot_pad = dw * 0.08
    d.line(
        [dx + slot_pad, slot_y, dx + dw - slot_pad, slot_y],
        fill=ACCENT, width=max(1, s // 64)
    )

    # Drive LED dot
    led_r = max(1, s // 48)
    led_x = dx + dw * 0.8
    led_y = slot_y + (dy + dh - slot_y) * 0.5
    d.ellipse(
        [led_x - led_r, led_y - led_r, led_x + led_r, led_y + led_r],
        fill=GREEN
    )

    # Magnifying glass
    mg_cx = s * 0.62
    mg_cy = s * 0.38
    mg_r = s * 0.17
    ring_w = max(2, s // 20)

    # Glass fill (semi-transparent blue)
    d.ellipse(
        [mg_cx - mg_r, mg_cy - mg_r, mg_cx + mg_r, mg_cy + mg_r],
        fill=(*ACCENT, 60), outline=WHITE, width=ring_w
    )

    # Handle
    angle = math.radians(45)
    hx1 = mg_cx + mg_r * math.cos(angle)
    hy1 = mg_cy + mg_r * math.sin(angle)
    handle_len = s * 0.2
    hx2 = hx1 + handle_len * math.cos(angle)
    hy2 = hy1 + handle_len * math.sin(angle)
    d.line([hx1, hy1, hx2, hy2], fill=WHITE, width=max(2, s // 16))

    # Handle grip (rounded end)
    grip_r = max(2, s // 24)
    d.ellipse(
        [hx2 - grip_r, hy2 - grip_r, hx2 + grip_r, hy2 + grip_r],
        fill=ACCENT
    )

    # Small sparkle / scan lines inside glass
    for i, frac in enumerate([0.3, 0.5, 0.7]):
        lx = mg_cx - mg_r * 0.5
        lx2 = mg_cx + mg_r * 0.5
        ly = mg_cy - mg_r * 0.6 + mg_r * 1.2 * frac
        alpha = 120 if i == 1 else 70
        d.line([lx, ly, lx2, ly], fill=(*GREEN, alpha), width=max(1, s // 64))

    return img

# Generate all sizes and save as .ico
images = [draw_icon(s) for s in SIZES]
images[0].save(
    "winscan.ico",
    format="ICO",
    sizes=[(s, s) for s in SIZES],
    append_images=images[1:]
)
print(f"Created winscan.ico ({', '.join(f'{s}x{s}' for s in SIZES)})")
