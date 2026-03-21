#!/usr/bin/env python3
"""Render ASCII art from markdown code blocks to PNG images for alignment verification.

Extracts all fenced code blocks from a markdown file and renders each one
to a numbered PNG using a monospace font. This lets you visually inspect
whether box-drawing characters, arrows, and indentation are properly aligned.

Usage:
    python tools/render_ascii_art.py docs/fec-arq-model.md [output_dir]

Output:
    output_dir/block_01.png, block_02.png, ...

Requires: PIL/Pillow (pip install Pillow)
"""

import os
import re
import sys

def extract_code_blocks(markdown_path):
    """Extract fenced code blocks from a markdown file."""
    with open(markdown_path, "r", encoding="utf-8") as f:
        content = f.read()

    blocks = []
    in_block = False
    current_block = []
    line_start = 0

    for i, line in enumerate(content.split("\n"), 1):
        if line.strip().startswith("```"):
            if in_block:
                blocks.append({
                    "text": "\n".join(current_block),
                    "start_line": line_start,
                    "end_line": i,
                })
                current_block = []
                in_block = False
            else:
                in_block = True
                line_start = i
                current_block = []
        elif in_block:
            current_block.append(line)

    return blocks


def render_block_to_png(block_text, output_path, font_size=14, padding=20):
    """Render a text block to a PNG image using a monospace font."""
    from PIL import Image, ImageDraw, ImageFont

    # Try to find a monospace font
    font = None
    mono_fonts = [
        "C:/Windows/Fonts/consola.ttf",       # Consolas (Windows)
        "C:/Windows/Fonts/cour.ttf",           # Courier New (Windows)
        "C:/Windows/Fonts/lucon.ttf",          # Lucida Console (Windows)
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",  # Linux
        "/System/Library/Fonts/Menlo.ttc",     # macOS
    ]
    for font_path in mono_fonts:
        if os.path.exists(font_path):
            try:
                font = ImageFont.truetype(font_path, font_size)
                break
            except Exception:
                continue

    if font is None:
        font = ImageFont.load_default()
        print("Warning: No monospace font found, using default (may not be monospace)")

    lines = block_text.split("\n")

    # Measure character dimensions using a reference character
    dummy_img = Image.new("RGB", (1, 1))
    dummy_draw = ImageDraw.Draw(dummy_img)
    bbox = dummy_draw.textbbox((0, 0), "M", font=font)
    char_width = bbox[2] - bbox[0]
    char_height = bbox[3] - bbox[1]
    line_height = int(char_height * 1.4)

    # Calculate image dimensions
    max_line_len = max((len(line) for line in lines), default=0)
    img_width = max_line_len * char_width + 2 * padding
    img_height = len(lines) * line_height + 2 * padding

    # Ensure minimum size
    img_width = max(img_width, 200)
    img_height = max(img_height, 50)

    # Create image with dark background
    img = Image.new("RGB", (img_width, img_height), color=(26, 26, 46))
    draw = ImageDraw.Draw(img)

    # Draw each line character by character for precise monospace alignment
    y = padding
    for line in lines:
        x = padding
        for ch in line:
            draw.text((x, y), ch, fill=(224, 224, 224), font=font)
            x += char_width
        y += line_height

    img.save(output_path)
    return img_width, img_height


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <markdown_file> [output_dir]")
        sys.exit(1)

    md_path = sys.argv[1]
    output_dir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(
        os.path.dirname(md_path), "ascii_art_renders"
    )

    if not os.path.exists(md_path):
        print(f"Error: {md_path} not found")
        sys.exit(1)

    os.makedirs(output_dir, exist_ok=True)

    blocks = extract_code_blocks(md_path)
    print(f"Found {len(blocks)} code blocks in {md_path}")

    for i, block in enumerate(blocks, 1):
        # Skip very short blocks (likely just formulas, not diagrams)
        lines = block["text"].strip().split("\n")
        has_box_chars = any(c in block["text"] for c in "┌┐└┘├┤┬┴│─╲╱►◄▼▲")

        output_path = os.path.join(output_dir, f"block_{i:02d}.png")
        w, h = render_block_to_png(block["text"], output_path)
        tag = " [DIAGRAM]" if has_box_chars else ""
        print(f"  Block {i:2d} (lines {block['start_line']}-{block['end_line']}): "
              f"{len(lines)} lines, {w}x{h}px{tag} -> {output_path}")

    print(f"\nRendered {len(blocks)} blocks to {output_dir}/")
    print("Open the PNG files to visually inspect alignment.")


if __name__ == "__main__":
    main()
