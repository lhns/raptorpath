#!/usr/bin/env python3
"""Extract model discussion messages from a Claude Code conversation transcript.

Reads a JSONL transcript file and extracts human/assistant text messages,
filtering out tool calls, system messages, and short operational exchanges.
Outputs a markdown document with the conversation in P:/C: format.

Usage:
    python tools/extract_conversation.py <transcript.jsonl> [output.md] [--min-length N]

The --min-length flag filters out messages shorter than N characters (default: 100).
This removes one-line operational messages ("push", "retry", etc.) and keeps
substantive discussion.
"""

import json
import os
import re
import sys


def extract_text_from_content(content):
    """Extract plain text from message content (string or content blocks)."""
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        texts = []
        for block in content:
            if isinstance(block, dict) and block.get("type") == "text":
                texts.append(block.get("text", ""))
        return "\n".join(texts).strip()
    return ""


def strip_system_reminders(text):
    """Remove <system-reminder> tags and their content."""
    text = re.sub(r"<system-reminder>.*?</system-reminder>", "", text, flags=re.DOTALL)
    text = re.sub(r"<available-deferred-tools>.*?</available-deferred-tools>", "", text, flags=re.DOTALL)
    return text.strip()


def is_model_discussion(text):
    """Heuristic: is this message about the model/theory (not code/ops)?"""
    # Skip pure operational messages
    ops_patterns = [
        r"^push$", r"^retry$", r"^commit", r"^show me",
        r"^run generate", r"^what other",
    ]
    for pat in ops_patterns:
        if re.match(pat, text.strip(), re.IGNORECASE):
            return False

    # Include messages that discuss theory/model
    theory_keywords = [
        "taper", "fec", "arq", "nack", "repair", "retransmit", "correction",
        "loss", "latency", "bandwidth", "reliability", "burst", "gilbert",
        "elliott", "bocd", "copa", "bbr", "multipath", "scheduling",
        "formula", "optimize", "probability", "deficit", "triangle",
        "protocol hint", "tail", "p_lost", "window", "estimation",
        "channel", "congestion", "overhead", "codec", "decoder",
        "source symbol", "repair symbol", "interleav",
        "well", "think", "actually", "right", "wrong", "model",
        "paper", "document", "section",
    ]
    text_lower = text.lower()
    return any(kw in text_lower for kw in theory_keywords)


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <transcript.jsonl> [output.md] [--min-length N]")
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = sys.argv[2] if len(sys.argv) > 2 and not sys.argv[2].startswith("--") else None
    min_length = 100

    for i, arg in enumerate(sys.argv):
        if arg == "--min-length" and i + 1 < len(sys.argv):
            min_length = int(sys.argv[i + 1])

    # Parse transcript
    messages = []
    with open(input_path, encoding="utf-8") as f:
        for line in f:
            d = json.loads(line)
            if d["type"] not in ("user", "assistant"):
                continue
            msg = d.get("message", {})
            role = msg.get("role", "")
            content = msg.get("content", "")
            text = extract_text_from_content(content)
            text = strip_system_reminders(text)

            if not text or len(text) < min_length:
                continue

            # Skip messages that are just tool results or code
            if text.startswith("Tool loaded") or text.startswith("User has answered"):
                continue

            if not is_model_discussion(text):
                continue

            messages.append({
                "role": role,
                "text": text,
                "timestamp": d.get("timestamp", ""),
            })

    print(f"Extracted {len(messages)} substantive messages from {input_path}")

    # Format as markdown
    lines = [
        "# FEC/ARQ Unified Model: Design Rationale",
        "",
        "*Companion to [fec-arq-model.md](fec-arq-model.md). This document",
        "preserves the conversation that developed the model, with operational",
        "messages (code changes, git commits, tool calls) filtered out. The",
        "thought process, wrong turns, and corrections are preserved because",
        "they illuminate the reasoning behind design decisions.*",
        "",
        "---",
        "",
    ]

    prev_role = None
    for msg in messages:
        role_label = "**P:**" if msg["role"] == "user" else "**C:**"

        # Add separator between role changes
        if prev_role and prev_role != msg["role"]:
            lines.append("")

        lines.append(f"{role_label} {msg['text']}")
        lines.append("")
        prev_role = msg["role"]

    output = "\n".join(lines)

    if output_path:
        with open(output_path, "w", encoding="utf-8") as f:
            f.write(output)
        print(f"Written to {output_path} ({len(lines)} lines)")
    else:
        print(output)


if __name__ == "__main__":
    main()
