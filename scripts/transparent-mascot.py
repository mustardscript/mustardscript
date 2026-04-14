#!/usr/bin/env python3
"""Remove the cream background from the mascot logo via corner flood-fill.

Flood-fill is used instead of a global color threshold because the mascot
body is yellow/cream itself — we only want to zero out the outer background,
not any cream-colored pixels inside the character.
"""
from PIL import Image
from collections import deque
import sys
from pathlib import Path

DEFAULT_SRC = Path("ux-artifacts/logos/09-mascot-character.png")
OUT_DIR = Path("ux-artifacts/logos")

# Colour-match tolerance for the background (per channel, 0..255).
TOLERANCE = 28


def close_enough(a, b, tol=TOLERANCE):
    return all(abs(int(x) - int(y)) <= tol for x, y in zip(a, b))


def flood_transparent(img: Image.Image) -> Image.Image:
    img = img.convert("RGBA")
    w, h = img.size
    px = img.load()

    # Seed from every edge pixel — anything connected to the border that matches
    # the edge colour within TOLERANCE becomes transparent.
    seeds = []
    for x in range(w):
        seeds.append((x, 0))
        seeds.append((x, h - 1))
    for y in range(h):
        seeds.append((0, y))
        seeds.append((w - 1, y))

    # Sample the average edge colour as the target background.
    edge_samples = [px[s[0], s[1]][:3] for s in seeds]
    avg = tuple(sum(c) // len(edge_samples) for c in zip(*edge_samples))
    print(f"edge avg colour: {avg}")

    visited = [[False] * h for _ in range(w)]
    q = deque()
    for s in seeds:
        sx, sy = s
        if not visited[sx][sy] and close_enough(px[sx, sy][:3], avg):
            visited[sx][sy] = True
            q.append(s)

    while q:
        x, y = q.popleft()
        r, g, b, _a = px[x, y]
        px[x, y] = (r, g, b, 0)
        for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            nx, ny = x + dx, y + dy
            if 0 <= nx < w and 0 <= ny < h and not visited[nx][ny]:
                if close_enough(px[nx, ny][:3], avg):
                    visited[nx][ny] = True
                    q.append((nx, ny))

    return img


def crop_to_square(img: Image.Image) -> Image.Image:
    """Crop to the opaque content, then pad to a centred square."""
    bbox = img.getbbox()  # bbox of non-zero (non-transparent) pixels for RGBA
    if not bbox:
        return img
    cropped = img.crop(bbox)
    cw, ch = cropped.size
    side = max(cw, ch)
    square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    square.paste(cropped, ((side - cw) // 2, (side - ch) // 2), cropped)
    return square


def main():
    src = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SRC
    if not src.exists():
        print(f"Source not found: {src}", file=sys.stderr)
        sys.exit(1)
    out = OUT_DIR / f"{src.stem}-transparent.png"
    img = Image.open(src)
    print(f"loaded {src} ({img.size})")

    transparent = flood_transparent(img)
    squared = crop_to_square(transparent)
    squared.save(out, optimize=True)
    print(f"wrote {out} ({squared.size})")


if __name__ == "__main__":
    main()
