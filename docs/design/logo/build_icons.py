"""Build the Tauri app icons from the logo sources in this folder.

Sizes below 48 px use the flat pixel-grid version (icon-small.svg, rendered
with headless Edge); 48 px and up use the glass version (icon-transparent.png,
exported from emblem-glass.figcraft).

    python docs/design/logo/build_icons.py
"""

import subprocess
import tempfile
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
OUT = HERE.parents[2] / "pc-app" / "app" / "src-tauri" / "icons"
EDGE = Path(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")

SMALL_MAX = 47  # largest size that uses the flat version
GLASS_FILL = 0.96  # share of the canvas width the glass symbol fills


def render_small(size: int, tmp: Path) -> Image.Image:
    html = tmp / f"small{size}.html"
    png = tmp / f"small{size}.png"
    html.write_text(
        '<!doctype html><body style="margin:0;background:transparent">'
        f'<img src="{(HERE / "icon-small.svg").as_uri()}" width="{size}" height="{size}" '
        'style="display:block"></body>'
    )
    subprocess.run(
        [str(EDGE), "--headless=new", "--disable-gpu", "--hide-scrollbars",
         "--force-device-scale-factor=1", "--default-background-color=00000000",
         f"--window-size={size},{size}", f"--screenshot={png}", html.as_uri()],
        check=True, capture_output=True,
    )
    return Image.open(png).convert("RGBA")


def glass_square(size: int, master: Image.Image) -> Image.Image:
    w = round(size * GLASS_FILL)
    h = round(w * master.height / master.width)
    symbol = master.resize((w, h), Image.LANCZOS)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.alpha_composite(symbol, ((size - w) // 2, (size - h) // 2))
    return canvas


def main() -> None:
    master = Image.open(HERE / "icon-transparent.png").convert("RGBA")
    cache: dict[int, Image.Image] = {}

    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)

        def icon(size: int) -> Image.Image:
            if size not in cache:
                cache[size] = (render_small(size, tmp) if size <= SMALL_MAX
                               else glass_square(size, master))
            return cache[size]

        pngs = {
            "32x32.png": 32,
            "128x128.png": 128,
            "128x128@2x.png": 256,
            "icon.png": 1024,
            "StoreLogo.png": 50,
            **{f"Square{s}x{s}Logo.png": s
               for s in (30, 44, 71, 89, 107, 142, 150, 284, 310)},
        }
        for name, size in pngs.items():
            icon(size).save(OUT / name)

        ico_sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
        frames = [icon(s) for s in ico_sizes]
        frames[-1].save(OUT / "icon.ico", format="ICO",
                        sizes=[(s, s) for s in ico_sizes], append_images=frames[:-1])

        icon(1024).save(OUT / "icon.icns", format="ICNS")

    print(f"wrote icons to {OUT}")


if __name__ == "__main__":
    main()
