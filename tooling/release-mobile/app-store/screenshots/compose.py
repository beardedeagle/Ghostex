#!/usr/bin/env python3
"""Place unedited simulator captures in a consistent App Store caption frame."""

from __future__ import annotations

import html
import json
from pathlib import Path

from PIL import Image, ImageCms, ImageDraw, ImageFilter, ImageFont, ImageOps


ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
REVIEW = ROOT / "docs/2026-10-19/app-store/screenshots"
RAW = REVIEW / "raw"
FONT = "/System/Library/Fonts/SFNS.ttf"
SIZES = {"iphone-6.9": (1320, 2868), "ipad-13": (2064, 2752)}
SRGB_PROFILE = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()


def background(width: int, height: int) -> Image.Image:
    image = Image.new("RGB", (width, height))
    draw = ImageDraw.Draw(image)
    for y in range(height):
        t = y / height
        color = (
            int(18 + 5 * t),
            int(24 + 4 * t),
            int(25 + 3 * t),
        )
        draw.line((0, y, width, y), fill=color)
    return image


def render(set_name: str, position: int, total: int, row: dict[str, str]) -> Path:
    width, height = SIZES[set_name]
    source = RAW / row["raw"]
    with Image.open(source) as opened:
        capture = opened.convert("RGB")
    assert capture.size == (width, height), f"Unexpected simulator size: {source}: {capture.size}"

    image = background(width, height)
    draw = ImageDraw.Draw(image)
    left = 82 if set_name == "iphone-6.9" else 120
    headline_size = 98 if set_name == "iphone-6.9" else 82
    subline_size = 39 if set_name == "iphone-6.9" else 38
    label_font = ImageFont.truetype(FONT, 25 if set_name == "iphone-6.9" else 31)
    headline_font = ImageFont.truetype(FONT, headline_size)
    subline_font = ImageFont.truetype(FONT, subline_size)
    label_font.set_variation_by_name("Bold")
    headline_font.set_variation_by_name("Semibold")

    draw.text((left, 55), "GHOSTEX", font=label_font, fill="#71ddc6", stroke_width=0)
    headline = row["headline"]
    lines = [headline]
    if set_name == "iphone-6.9" and draw.textbbox((0, 0), headline, font=headline_font)[2] > width - 2 * left:
        words = headline.split()
        candidates = [(" ".join(words[:split]), " ".join(words[split:])) for split in range(1, len(words))]
        lines = list(min(candidates, key=lambda pair: max(draw.textbbox((0, 0), line, font=headline_font)[2] for line in pair)))
    headline_y = 105 if set_name == "iphone-6.9" else 108
    line_height = 107 if set_name == "iphone-6.9" else 90
    for line in lines:
        draw.text((left, headline_y), line, font=headline_font, fill="#f5f8f6")
        headline_y += line_height
    draw.text((left, headline_y + 11), row["subline"], font=subline_font, fill="#acb9b6")

    top = 425 if set_name == "iphone-6.9" else 320
    max_width = width - (2 * (76 if set_name == "iphone-6.9" else 110))
    max_height = height - top - 36
    scale = min(max_width / capture.width, max_height / capture.height)
    screenshot = capture.resize((round(capture.width * scale), round(capture.height * scale)),
                                Image.Resampling.LANCZOS)
    x = (width - screenshot.width) // 2
    y = top

    shadow_layer = Image.new("RGBA", image.size)
    shadow_draw = ImageDraw.Draw(shadow_layer)
    shadow_draw.rounded_rectangle((x - 5, y - 5, x + screenshot.width + 5,
                                   y + screenshot.height + 5), radius=22,
                                  fill=(0, 0, 0, 90))
    shadow_layer = shadow_layer.filter(ImageFilter.GaussianBlur(18))
    image = Image.alpha_composite(image.convert("RGBA"), shadow_layer).convert("RGB")
    mask = Image.new("L", screenshot.size)
    corner_radius = 25 if set_name == "iphone-6.9" else 22
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, screenshot.width - 1, screenshot.height - 1),
                                            radius=corner_radius, fill=255)
    image.paste(screenshot, (x, y), mask)
    border = ImageDraw.Draw(image)
    border.rounded_rectangle((x - 1, y - 1, x + screenshot.width, y + screenshot.height),
                             radius=corner_radius + 1, outline="#495450", width=2)

    output = HERE / set_name / row["file"]
    output.parent.mkdir(parents=True, exist_ok=True)
    if set_name == "ipad-13" and row["file"] == "05-web-preview.png":
        image = ImageOps.posterize(image, 6)
    image.save(output, "PNG", optimize=True, compress_level=9, icc_profile=SRGB_PROFILE)
    return output


def write_review(captions: dict[str, list[dict[str, str]]]) -> None:
    blocks = []
    for set_name, rows in captions.items():
        title = "iPhone 6.9″" if set_name == "iphone-6.9" else "iPad 13″"
        blocks.append(f"<section><h2>{title}</h2>")
        for index, row in enumerate(rows, 1):
            image_path = f"../../../../tooling/release-mobile/app-store/screenshots/{set_name}/{row['file']}"
            raw_path = f"raw/{row['raw']}"
            blocks.append(
                '<article class="shot">'
                f'<div class="meta"><h3>{html.escape(row["headline"])}</h3>'
                f'<p>{html.escape(row["subline"])}</p>'
                f'<div class="files"><a href="{html.escape(image_path)}">{html.escape(row["file"])}</a>'
                f'<a href="{html.escape(raw_path)}">Raw simulator capture</a></div></div>'
                f'<a href="{html.escape(image_path)}"><img loading="lazy" src="{html.escape(image_path)}"'
                f' alt="{html.escape(row["headline"])} on {title}"></a></article>'
            )
        blocks.append("</section>")
    content = "\n".join(blocks)
    page = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Ghostex App Store screenshot review</title>
<style>
*{{box-sizing:border-box}}html{{background:#111819;color:#eaf0ed;font-family:-apple-system,BlinkMacSystemFont,Arial,sans-serif}}
body{{margin:0}}header,main{{max-width:1080px;margin:auto;padding:28px}}header{{padding-top:58px;padding-bottom:10px}}
.eyebrow{{color:#71ddc6;font-size:13px;letter-spacing:.18em;font-weight:700}}h1{{font-size:clamp(32px,6vw,58px);line-height:1.05;margin:15px 0}}
header p{{color:#a9b8b4;line-height:1.5;max-width:680px}}h2{{font-size:29px;margin:34px 0 20px}}h3{{font-size:24px;margin:7px 0}}
.shot{{background:#1a2222;border:1px solid #2c3936;border-radius:20px;margin:0 0 34px;padding:22px}}
.meta{{margin:0 0 18px}}.meta span{{color:#71ddc6;font-size:13px;font-weight:700}}.meta p{{color:#a9b8b4;margin:6px 0 12px}}
.files{{display:flex;flex-wrap:wrap;gap:12px;font-size:13px}}a{{color:#8be6d2}}.shot>a{{display:block;text-align:center}}
img{{max-width:100%;height:auto;max-height:800px;object-fit:contain;box-shadow:0 14px 34px #0007}}
@media(max-width:650px){{header,main{{padding-left:16px;padding-right:16px}}.shot{{padding:12px}}}}
</style></head><body><header><div class="eyebrow">GHOSTEX · APP STORE</div>
<h1>Screenshot review</h1><p>Final iPhone and iPad images, in App Store order. Each screen contains an unedited capture from the release app running against isolated computers.</p></header>
<main>{content}</main></body></html>"""
    REVIEW.mkdir(parents=True, exist_ok=True)
    (REVIEW / "index.html").write_text(page)


def main() -> None:
    captions = json.loads((HERE / "captions.json").read_text())
    for set_name, rows in captions.items():
        assert set_name in SIZES
        for position, row in enumerate(rows, 1):
            render(set_name, position, len(rows), row)
    write_review(captions)


if __name__ == "__main__":
    main()
