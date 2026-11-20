#!/usr/bin/env python3
"""Draw the INKFORGE application icon.

    python3 scripts/make-icon.py

Writes packaging/windows/inkforge.ico, the icon that build.rs embeds into the
Windows executable, plus packaging/windows/inkforge-256.png (a plain preview of
the same artwork). The drawing is procedural so the icon can be rebuilt and
tweaked without an image editor; every size is rendered from the same 4x master
and downsampled, so small sizes stay crisp.

Layout: a dark rounded tile with a warm ember glow, and an ink drop in the
middle. Those two shapes stay readable even at 16 px in Explorer.
"""

import math
import struct
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "packaging" / "windows"
ICO = OUT_DIR / "inkforge.ico"
PREVIEW = OUT_DIR / "inkforge-256.png"

# Drawn large and sampled down, so curves and the glow stay smooth.
MASTER = 1024
SIZES_DIB = [16, 24, 32, 48, 64]  # uncompressed bitmaps: most compatible
SIZES_PNG = [128, 256]  # PNG-compressed entries, supported since Vista


def vertical_gradient(size, top, bottom):
    """A size x size image interpolating *top* -> *bottom* down the y axis."""
    img = Image.new("RGBA", (size, size))
    px = img.load()
    (tr, tg, tb, ta), (br, bg, bb, ba) = top, bottom
    for y in range(size):
        f = y / max(1, size - 1)
        row = (
            round(tr + (br - tr) * f),
            round(tg + (bg - tg) * f),
            round(tb + (bb - tb) * f),
            round(ta + (ba - ta) * f),
        )
        for x in range(size):
            px[x, y] = row
    return img


def rounded(size, radius):
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, size - 1, size - 1], radius=radius, fill=255
    )
    return mask


def bezier(p0, c1, c2, p3, steps):
    points = []
    for i in range(1, steps + 1):
        t = i / steps
        u = 1 - t
        x = u**3 * p0[0] + 3 * u * u * t * c1[0] + 3 * u * t * t * c2[0] + t**3 * p3[0]
        y = u**3 * p0[1] + 3 * u * u * t * c1[1] + 3 * u * t * t * c2[1] + t**3 * p3[1]
        points.append((x, y))
    return points


def drop_polygon(cx, cy, r, height):
    """An ink drop: a round body with a tapered point on top.

    The sides leave the body along its tangents (computed from the tip, so
    they meet the circle without a seam) and bow very slightly inward, which is
    what separates a drop from a balloon on a stick.
    """
    tip = (cx, cy - height)
    # Where the tangent lines from the tip touch the circle.
    cos_beta = min(1.0, r / height)
    beta = math.acos(cos_beta)
    right = (cx + r * math.sin(beta), cy - r * cos_beta)
    left = (cx - r * math.sin(beta), cy - r * cos_beta)

    steps = 240
    points = []
    # The body, swept from the right tangent point round the bottom to the left.
    start_angle = -beta
    for i in range(steps + 1):
        angle = start_angle + (math.pi + 2 * beta) * i / steps
        points.append((cx + r * math.cos(angle), cy + r * math.sin(angle)))

    # Tangent direction at each side point, pointing up towards the tip, so the
    # bowed sides still meet the body without a crease.
    side_length = math.hypot(right[0] - tip[0], right[1] - tip[1])
    right_tangent = (-math.cos(beta) * 0.40 * side_length, -math.sin(beta) * 0.40 * side_length)
    left_tangent = (math.cos(beta) * 0.40 * side_length, -math.sin(beta) * 0.40 * side_length)

    points += bezier(
        left,
        (left[0] + left_tangent[0], left[1] + left_tangent[1]),
        (tip[0] - 0.10 * r, tip[1] + 0.22 * side_length),
        tip,
        steps,
    )
    points += bezier(
        tip,
        (tip[0] + 0.10 * r, tip[1] + 0.22 * side_length),
        (right[0] + right_tangent[0], right[1] + right_tangent[1]),
        right,
        steps,
    )
    return points


def draw_master():
    s = MASTER
    tile_radius = round(s * 0.22)
    tile_mask = rounded(s, tile_radius)

    # Background: near-black slate, lit from the bottom by the forge.
    image = vertical_gradient(s, (32, 35, 44, 255), (9, 10, 14, 255))

    # A warm halo behind the drop, rather than a wash across the tile: it keeps
    # the corners dark so the tile still reads as a tile at small sizes.
    halo_size = round(s * 0.92)
    radial = Image.radial_gradient("L").resize((halo_size, halo_size), Image.LANCZOS)
    glow = Image.new("RGBA", (halo_size, halo_size), (224, 104, 34, 255))
    glow.putalpha(radial.point(lambda v: round(((255 - v) / 255) ** 2.2 * 230)))
    glow = glow.filter(ImageFilter.GaussianBlur(s * 0.02))
    image = Image.alpha_composite(
        image,
        Image.new("RGBA", (s, s), (0, 0, 0, 0)),
    )
    layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    layer.paste(glow, (round(s * 0.5 - halo_size / 2), round(s * 0.60 - halo_size / 2)))
    image = Image.alpha_composite(image, layer)

    # The drop.
    cx, cy, r = s * 0.5, s * 0.635, s * 0.190
    drop_mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(drop_mask).polygon(drop_polygon(cx, cy, r, 1.95 * r), fill=255)

    ember = vertical_gradient(s, (255, 226, 170, 255), (188, 42, 18, 255))
    halo = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    halo.paste((255, 138, 58, 130), (0, 0), drop_mask)
    halo = halo.filter(ImageFilter.GaussianBlur(s * 0.045))
    image = Image.alpha_composite(image, halo)
    image.paste(ember, (0, 0), drop_mask)

    # A soft specular, so the drop reads as wet ink rather than a flat blob.
    spec = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ImageDraw.Draw(spec).ellipse(
        [cx - 0.079 * s, cy - 0.070 * s, cx - 0.030 * s, cy - 0.010 * s],
        fill=(255, 252, 244, 120),
    )
    spec = spec.filter(ImageFilter.GaussianBlur(s * 0.018))
    image = Image.composite(Image.alpha_composite(image, spec), image, drop_mask)

    # Hairline inner edge: keeps the tile from dissolving into a dark desktop.
    edge = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle(
        [s * 0.012, s * 0.012, s - s * 0.012, s - s * 0.012],
        radius=tile_radius,
        outline=(255, 176, 112, 70),
        width=max(1, round(s * 0.006)),
    )
    image = Image.alpha_composite(image, edge)

    image.putalpha(tile_mask)
    return image


def dib_bytes(img):
    """A 32-bit bottom-up DIB with the AND mask, as stored inside an .ico."""
    width, height = img.size
    pixels = img.convert("RGBA").load()
    header = struct.pack("<IiiHHIIiiII", 40, width, height * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    body = bytearray()
    for y in range(height - 1, -1, -1):
        for x in range(width):
            red, green, blue, alpha = pixels[x, y]
            body += bytes((blue, green, red, alpha))
    stride = ((width + 31) // 32) * 4
    body += b"\x00" * (stride * height)
    return header + bytes(body)


def png_bytes(img):
    from io import BytesIO

    buffer = BytesIO()
    img.save(buffer, format="PNG", optimize=True)
    return buffer.getvalue()


def write_ico(master, path):
    entries = []
    for size in SIZES_DIB:
        entries.append((size, dib_bytes(master.resize((size, size), Image.LANCZOS))))
    for size in SIZES_PNG:
        entries.append((size, png_bytes(master.resize((size, size), Image.LANCZOS))))

    out = bytearray(struct.pack("<HHH", 0, 1, len(entries)))
    offset = 6 + 16 * len(entries)
    for size, data in entries:
        out += struct.pack(
            "<BBBBHHII",
            size & 0xFF,  # 256 is written as 0
            size & 0xFF,
            0,
            0,
            1,
            32,
            len(data),
            offset,
        )
        offset += len(data)
    for _, data in entries:
        out += data
    path.write_bytes(out)
    return len(out)


def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    master = draw_master()
    written = write_ico(master, ICO)
    master.resize((256, 256), Image.LANCZOS).save(PREVIEW)
    print(f"wrote {ICO.relative_to(ROOT)} ({written} bytes)")
    print(f"wrote {PREVIEW.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
