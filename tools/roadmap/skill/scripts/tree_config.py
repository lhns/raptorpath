#!/usr/bin/env python3
"""Load and validate tech-tree.yaml, the one config every tech-tree script reads.

Paths in the file are relative to the file itself. A bad config is a clear error
message and exit code 2 (see load_for_cli).
"""
import os
import re
import sys
from pathlib import Path

import yaml

DEFAULT_CONFIG = "tech-tree.yaml"
DEFAULT_NODES = "docs/roadmap/nodes.yaml"
DEFAULT_PAGE = "docs/roadmap.md"
SHAPES = ("box", "slant", "hexagon", "flag", "round")
KIND_KEYS = ("shape", "rated", "owner_gate", "blocks")
TOP_KEYS = ("nodes", "page", "tracks", "kinds", "lanes", "queue_lane", "overview", "layout", "history")
LAYOUT_DEFAULTS = {"overview_width": 3, "track_width": 3, "band_min": 1}
HISTORY_KEYS = ("main", "weak_refs", "merge_subject")
RESERVED_REFS = ("sha", "branch")  # token kinds history_gap.py uses for its strong matches
CODE_RE = re.compile(r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$")


class ConfigError(Exception):
    pass


class Config:
    """A validated config. `kinds` maps a name to {shape, rated, owner_gate, blocks}, all filled in."""

    def __init__(self, path, raw):
        self.path = Path(path)
        self.root = self.path.parent
        self.tracks = {c: (v["name"], v.get("scope", "")) for c, v in raw["tracks"].items()}
        self.kinds = {k: {"shape": (v or {}).get("shape", "box"), "rated": (v or {}).get("rated", True),
                          "owner_gate": (v or {}).get("owner_gate", False), "blocks": (v or {}).get("blocks", True)}
                      for k, v in raw["kinds"].items()}
        self.lanes = list(raw["lanes"])
        self.queue_lane = raw.get("queue_lane") or None
        self.overview = raw.get("overview") or None  # {min, max} or None
        self.history = raw.get("history") or {}
        self.layout = {**LAYOUT_DEFAULTS, **(raw.get("layout") or {})}
        self.nodes_path = _join(self.root, raw.get("nodes", DEFAULT_NODES))
        self.page_path = _join(self.root, raw.get("page", DEFAULT_PAGE))

    # generated pages: <page dir>/<page stem>/...
    @property
    def page_dir(self):
        return self.page_path.parent

    @property
    def page_stem(self):
        return self.page_path.stem

    @property
    def generated_dir(self):
        return self.page_dir / self.page_stem

    @property
    def tracks_dir(self):
        return self.generated_dir / "tracks"

    @property
    def timeline_path(self):
        return self.generated_dir / "timeline.md"

    def track_file(self, code):
        return self.tracks_dir / f"{code}.md"

    def nodes_rel(self):
        """The nodes path as seen from the page, with forward slashes."""
        return os.path.relpath(self.nodes_path, self.page_dir).replace("\\", "/")


def _join(root, p):
    return Path(os.path.abspath(Path(root) / str(p)))


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def validate(raw):
    """List of problems with a parsed config (empty when fine)."""
    if not isinstance(raw, dict):
        return ["the config must be a mapping"]
    errs = [f"unknown key '{k}' (allowed: {', '.join(TOP_KEYS)})" for k in raw if k not in TOP_KEYS]
    for key in ("nodes", "page"):
        if key in raw and (not isinstance(raw[key], str) or not raw[key].strip()):
            errs.append(f"{key} must be a non-empty path")
    tracks = raw.get("tracks")
    if not isinstance(tracks, dict) or not tracks:
        errs.append("tracks must be a non-empty mapping {code: {name, scope}}")
    else:
        for code, v in tracks.items():
            if not isinstance(code, str) or not CODE_RE.match(code):
                errs.append(f"tracks: code {code!r} must be lowercase kebab-case")
            if not isinstance(v, dict) or not isinstance(v.get("name"), str) or not v["name"].strip():
                errs.append(f"tracks.{code}: needs a 'name'")
                continue
            if not isinstance(v.get("scope", ""), str):
                errs.append(f"tracks.{code}: scope must be a string")
            errs += [f"tracks.{code}: unknown key '{k}'" for k in v if k not in ("name", "scope")]
    kinds = raw.get("kinds")
    if not isinstance(kinds, dict) or not kinds:
        errs.append("kinds must be a non-empty mapping {name: {shape, rated, owner_gate, blocks}}")
    else:
        for name, v in kinds.items():
            if not isinstance(name, str) or not name.strip():
                errs.append(f"kinds: name {name!r} must be a non-empty string")
            if v is None:
                continue
            if not isinstance(v, dict):
                errs.append(f"kinds.{name}: must be a mapping (or empty)")
                continue
            errs += [f"kinds.{name}: unknown attribute '{k}' (allowed: {', '.join(KIND_KEYS)})" for k in v if k not in KIND_KEYS]
            if "shape" in v and v["shape"] not in SHAPES:
                errs.append(f"kinds.{name}: shape '{v['shape']}' is not one of {list(SHAPES)}")
            for b in ("rated", "owner_gate", "blocks"):
                if b in v and not isinstance(v[b], bool):
                    errs.append(f"kinds.{name}: {b} must be true or false")
    lanes = raw.get("lanes")
    if not isinstance(lanes, list) or not lanes or not all(isinstance(x, str) and x.strip() for x in lanes):
        errs.append("lanes must be a non-empty list of names")
        lanes = None
    elif len(set(lanes)) != len(lanes):
        errs.append("lanes must be unique")
    ql = raw.get("queue_lane")
    if ql is not None and (not isinstance(ql, str) or (lanes is not None and ql not in lanes)):
        errs.append(f"queue_lane '{ql}' must be one of the lanes")
    ov = raw.get("overview")
    if ov is not None:
        if (not isinstance(ov, dict) or set(ov) - {"min", "max"} or not _is_int(ov.get("min")) or not _is_int(ov.get("max"))
                or ov["min"] > ov["max"]):
            errs.append("overview must be {min: N, max: M} with integers, min <= max")
    lay = raw.get("layout")
    if lay is not None:
        if not isinstance(lay, dict):
            errs.append("layout must be a mapping")
        else:
            errs += [f"layout: unknown key '{k}' (allowed: {', '.join(LAYOUT_DEFAULTS)})" for k in lay if k not in LAYOUT_DEFAULTS]
            errs += [f"layout.{k} must be a positive integer" for k, v in lay.items()
                     if k in LAYOUT_DEFAULTS and not (_is_int(v) and v > 0)]
    hist = raw.get("history")
    if hist is not None:
        if not isinstance(hist, dict):
            errs.append("history must be a mapping")
        else:
            errs += [f"history: unknown key '{k}' (allowed: {', '.join(HISTORY_KEYS)})" for k in hist if k not in HISTORY_KEYS]
            if "main" in hist and not isinstance(hist["main"], str):
                errs.append("history.main must be a ref name")
            refs = hist.get("weak_refs", {})
            if not isinstance(refs, dict):
                errs.append("history.weak_refs must be a mapping {name: regex}")
            else:
                for name, rx in refs.items():
                    if name in RESERVED_REFS:
                        errs.append(f"history.weak_refs: the name '{name}' is reserved")
                    errs += _regex_problem(f"history.weak_refs.{name}", rx, exact_groups=1)
            if "merge_subject" in hist:
                errs += _regex_problem("history.merge_subject", hist["merge_subject"], min_groups=1)
    return errs


def _regex_problem(label, rx, exact_groups=None, min_groups=None):
    try:
        c = re.compile(rx)
    except (re.error, TypeError) as e:
        return [f"{label}: not a valid regex ({e})"]
    if exact_groups is not None and c.groups != exact_groups:
        return [f"{label}: needs exactly {exact_groups} capture group, has {c.groups}"]
    if min_groups is not None and c.groups < min_groups:
        return [f"{label}: needs a capture group (group 1 is the branch name)"]
    return []


def from_dict(raw, path):
    """Build a Config from an already parsed dict; `path` is the (possibly not yet existing) config file path."""
    errs = validate(raw)
    if errs:
        raise ConfigError(f"{path}: invalid config:\n  " + "\n  ".join(errs))
    return Config(Path(os.path.abspath(path)), raw)


def load(path):
    path = Path(path)
    try:
        raw = yaml.safe_load(path.read_text(encoding="utf-8"))
    except OSError as e:
        raise ConfigError(f"cannot read config {path}: {e.strerror or e}") from None
    except yaml.YAMLError as e:
        raise ConfigError(f"{path}: not valid YAML: {e}") from None
    return from_dict(raw, path)


def utf8_stdio():
    """Messages carry node titles; keep them printable on a console that is not UTF-8."""
    for s in (sys.stdout, sys.stderr):
        try:
            s.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError):
            pass


def add_config_arg(ap):
    ap.add_argument("--config", type=Path, default=Path(DEFAULT_CONFIG),
                    help=f"path of the config (default ./{DEFAULT_CONFIG})")


def load_for_cli(path, prog):
    """The Config, or None after printing the error (the caller then exits 2)."""
    try:
        return load(path)
    except ConfigError as e:
        print(f"{prog}: {e}", file=sys.stderr)
        return None
