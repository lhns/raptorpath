#!/bin/bash
# Build the FEC/ARQ model paper from markdown to PDF using Pandoc + Typst.
#
# Usage: bash tools/build_paper.sh [input.md] [output.pdf]
#
# Requires:
#   - Pandoc 3.9+ (markdown to Typst conversion)
#   - Typst (PDF rendering)
#
# All layout/styling is in this script and the Typst template.
# The markdown source is NOT modified.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

INPUT="${1:-$PROJECT_DIR/docs/fec-arq-model.md}"
OUTPUT="${2:-$PROJECT_DIR/docs/fec-arq-model.pdf}"

# Find pandoc
PANDOC="${PANDOC:-}"
if [ -z "$PANDOC" ]; then
    # Check common locations
    for candidate in \
        "pandoc" \
        "C:/Users/pierr/Downloads/pandoc-3.9.0.2-windows-x86_64/pandoc-3.9.0.2/pandoc.exe" \
        "$HOME/.local/bin/pandoc"; do
        if command -v "$candidate" &>/dev/null || [ -f "$candidate" ]; then
            PANDOC="$candidate"
            break
        fi
    done
fi

if [ -z "$PANDOC" ]; then
    echo "Error: pandoc not found. Set PANDOC env var or install pandoc."
    exit 1
fi

# Find typst
TYPST="${TYPST:-}"
if [ -z "$TYPST" ]; then
    for candidate in "typst" "$HOME/.cargo/bin/typst" "$HOME/.cargo/bin/typst.exe"; do
        if command -v "$candidate" &>/dev/null || [ -f "$candidate" ]; then
            TYPST="$candidate"
            break
        fi
    done
fi

if [ -z "$TYPST" ]; then
    echo "Error: typst not found. Install with: cargo install --git https://github.com/typst/typst --locked typst-cli"
    exit 1
fi

echo "Using pandoc: $PANDOC"
echo "Using typst:  $TYPST"
echo "Input:        $INPUT"
echo "Output:       $OUTPUT"

# Strip the manual TOC from the markdown (pandoc --toc generates its own)
TMPFILE=$(mktemp --suffix=.md)
sed '/^## Table of Contents$/,/^---$/d' "$INPUT" > "$TMPFILE"

# Create a Typst header file for table/layout fixes
TYPST_HEADER=$(mktemp --suffix=.typ)
cat > "$TYPST_HEADER" << 'TYPSTEOF'
// Fix table rendering: allow tables to break across pages
#set table(stroke: 0.5pt + luma(180))
#show table: set text(size: 8.5pt)
#show table.cell: set par(leading: 0.45em)
#show table.cell: set text(hyphenate: true)

// Force figures (which pandoc wraps tables in) to be breakable
#show figure.where(kind: table): set block(breakable: true)
#show figure.where(kind: table): set figure(gap: 0.5em)

// Pandoc generates tables with auto column widths which can overflow.
// Wrap all tables in a block that constrains width and enables word wrap.
#show figure.where(kind: table): it => block(width: 100%, breakable: true)[#it]

// Better page breaks: avoid orphans/widows
#set par(leading: 0.65em)
#set block(breakable: true)

// Code blocks: smaller font
// Short blocks (ASCII art, diagrams): keep together on one page
// Long blocks (formula summaries): allow page breaks
#show raw.where(block: true): set text(size: 8pt)
#show raw.where(block: true): it => {
  let lines = it.text.split("\n").len()
  if lines > 25 {
    block(breakable: true, width: 100%, it)
  } else {
    block(breakable: false, width: 100%, it)
  }
}

// New page for each section and subsection
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  it
}
#show heading.where(level: 2): it => {
  pagebreak(weak: true)
  it
}
#show raw.where(block: false): set text(size: 9pt)

// Headings: keep with following content
#show heading: set block(above: 1.5em, below: 0.8em)
TYPSTEOF

# Two-step: markdown -> typst -> fix columns -> PDF
TYPST_FILE=$(mktemp --suffix=.typ)

# Step 1: markdown to typst
"$PANDOC" \
    "$TMPFILE" \
    -o "$TYPST_FILE" \
    --include-in-header="$TYPST_HEADER" \
    -V fontsize=11pt \
    -V papersize=a4 \
    -V margin-top=2.5cm \
    -V margin-bottom=2.5cm \
    -V margin-left=2.5cm \
    -V margin-right=2.5cm \
    --toc \
    --toc-depth=3 \
    --metadata title="FEC/ARQ Unified Correction Symbol Model" \
    --metadata author="Pierre Kisters" \
    --metadata date="April 2026" \
    2>&1

# Step 2: fix table columns — replace percentage widths with equal fractions
# Pandoc generates narrow percentage columns that cause text overflow.
# Replace all percentage-based column specs with equal 1fr columns.
# e.g., "columns: (16.9%, 14.08%, ...)" -> "columns: (1fr, 1fr, ...)"
sed -i '/^    columns: (/s/[0-9.]*%/1fr/g' "$TYPST_FILE"

# Step 3: prevent double page break when first subsection follows section
# Level-1 heading (=) and level-2 (==) both get pagebreaks from show rules.
# When == immediately follows =, that's a double break. Convert the first
# == after each = to a manual heading that bypasses the show rule.
python3 -c "
import re, sys
typst_file = sys.argv[1]
with open(typst_file, encoding='utf-8') as f:
    lines = f.readlines()
out = []
prev_was_h1 = False
for line in lines:
    s = line.rstrip()
    if re.match(r'^= ', s) and not re.match(r'^==', s):
        prev_was_h1 = True
        out.append(line)
    elif re.match(r'^== ', s) and prev_was_h1:
        title = s[3:]
        out.append('#heading(level: 2, outlined: true)[' + title + ']\n')
        prev_was_h1 = False
    else:
        if s:
            prev_was_h1 = False
        out.append(line)
with open(typst_file, 'w', encoding='utf-8') as f:
    f.writelines(out)
" "$TYPST_FILE"

# Step 4: typst to PDF
"$TYPST" compile "$TYPST_FILE" "$OUTPUT" 2>&1

rm -f "$TMPFILE" "$TYPST_HEADER" "$TYPST_FILE"

echo ""
echo "PDF generated: $OUTPUT"
echo "Size: $(du -h "$OUTPUT" | cut -f1)"
