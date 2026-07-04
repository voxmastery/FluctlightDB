#!/usr/bin/env python3
"""Playwright layout audit — overlap, clipping, arrows, out-of-frame."""

from __future__ import annotations

import math
from pathlib import Path

from playwright.sync_api import sync_playwright

HTML = Path(__file__).resolve().parent / "fig1_template.html"

# Expected graph topology (must match fig1_template.html)
NODES = {
    "cue": (260, 62, 30),
    "fts5": (195, 132, 26),
    "hnsw": (325, 132, 26),
    "eL": (115, 205, 24),
    "eM": (260, 205, 24),
    "eR": (405, 205, 24),
    "gsL": (195, 282, 22),
    "gsR": (325, 282, 22),
    "fused": (260, 334, 26),
}
EDGES = [
    ("cue", "fts5"), ("cue", "hnsw"),
    ("fts5", "eL"), ("fts5", "eM"),
    ("hnsw", "eM"), ("hnsw", "eR"),
    ("eL", "gsL"), ("eR", "gsR"),
    ("eM", "fused"),
    ("gsL", "fused"), ("gsR", "fused"),
]


def overlap(a: dict, b: dict, pad: float = 2) -> bool:
    return not (
        a["x"] + a["width"] - pad <= b["x"] + pad
        or b["x"] + b["width"] - pad <= a["x"] + pad
        or a["y"] + a["height"] - pad <= b["y"] + pad
        or b["y"] + b["height"] - pad <= a["y"] + pad
    )


def contains(outer: dict, inner: dict, tol: float = 1.5) -> bool:
    return (
        inner["x"] >= outer["x"] - tol
        and inner["y"] >= outer["y"] - tol
        and inner["x"] + inner["width"] <= outer["x"] + outer["width"] + tol
        and inner["y"] + inner["height"] <= outer["y"] + outer["height"] + tol
    )


def dist(x1: float, y1: float, x2: float, y2: float) -> float:
    return math.hypot(x2 - x1, y2 - y1)


def perimeter_point(cx: float, cy: float, r: float, tx: float, ty: float, pad: float = 3) -> tuple[float, float]:
    dx, dy = tx - cx, ty - cy
    d = math.hypot(dx, dy) or 1.0
    return cx + dx / d * (r + pad), cy + dy / d * (r + pad)


def audit_svg_arrows(page) -> int:
    """Check SVG lines start/end near expected circle perimeters."""
    issues = 0
    lines = page.locator(".hero-svg line").all()
    if len(lines) != len(EDGES):
        print(f"ARROW COUNT: expected {len(EDGES)} lines, found {len(lines)}")
        issues += 1

    svg = page.locator(".hero-svg").bounding_box()
    if not svg:
        return issues + 1

    for i, ((a_name, b_name), line) in enumerate(zip(EDGES, lines)):
        x1 = float(line.get_attribute("x1") or 0)
        y1 = float(line.get_attribute("y1") or 0)
        x2 = float(line.get_attribute("x2") or 0)
        y2 = float(line.get_attribute("y2") or 0)
        ax, ay, ar = NODES[a_name]
        bx, by, br = NODES[b_name]
        exp_x1, exp_y1 = perimeter_point(ax, ay, ar, bx, by)
        exp_x2, exp_y2 = perimeter_point(bx, by, br, ax, ay)
        tol = 4.0
        if dist(x1, y1, exp_x1, exp_y1) > tol or dist(x2, y2, exp_x2, exp_y2) > tol:
            issues += 1
            print(f"ARROW GEOM[{i}] {a_name}->{b_name}: "
                  f"({x1},{y1})-({x2},{y2}) expected ~({exp_x1:.1f},{exp_y1:.1f})-({exp_x2:.1f},{exp_y2:.1f})")

        # endpoints must not sit inside target circle center (dangling stub check)
        if dist(x2, y2, bx, by) < br - 6:
            issues += 1
            print(f"ARROW STUB[{i}] {a_name}->{b_name}: end too close to center of {b_name}")

    return issues


def audit_ellipse_containment(page) -> int:
    """All hero node circles must fit inside the brain casing ellipse."""
    issues = 0
    ellipse = {"cx": 260, "cy": 200, "rx": 220, "ry": 172, "pad": 2}
    cx, cy, rx, ry, pad = ellipse["cx"], ellipse["cy"], ellipse["rx"], ellipse["ry"], ellipse["pad"]

    for name, (nx, ny, nr) in NODES.items():
        for label, px, py in (
            ("center", nx, ny),
            ("top", nx, ny - nr),
            ("bottom", nx, ny + nr),
            ("left", nx - nr, ny),
            ("right", nx + nr, ny),
        ):
            val = ((px - cx) / rx) ** 2 + ((py - cy) / ry) ** 2
            if val > 1.0 + pad / min(rx, ry):
                issues += 1
                print(f"ELLIPSE OUT: {name} {label} ({px},{py}) val={val:.3f}")
    return issues


def main() -> None:
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(device_scale_factor=2)
        page.goto(HTML.as_uri())
        page.wait_for_load_state("networkidle")

        body = page.locator("body").bounding_box()
        assert body
        print(f"Body: {body['width']:.0f}×{body['height']:.0f}px\n")

        clip_issues = 0
        for i, col in enumerate(page.locator(".compare-col").all()):
            cb = col.bounding_box()
            if not cb:
                continue
            for sel in ("h4", "ul", "li"):
                for el in col.locator(sel).all():
                    eb = el.bounding_box()
                    if eb and not contains(cb, eb):
                        clip_issues += 1
                        print(f"CLIP compare-col[{i}] {sel}: outside parent")

        oob = 0
        for sel in (".hero-wrap", ".stack", ".flow-card", ".compare", ".compare-col"):
            for i, el in enumerate(page.locator(sel).all()):
                bb = el.bounding_box()
                if bb and not contains(body, bb):
                    oob += 1
                    print(f"OUT OF BODY: {sel}[{i}]")

        panels: list[tuple[str, dict]] = []
        for sel, name in (
            (".hero-wrap", "hero"),
            (".stack", "stack"),
            (".bottom .flow-card.write", "write"),
            (".bottom .flow-card.read", "read"),
            (".compare", "compare"),
        ):
            bb = page.locator(sel).first.bounding_box()
            if bb:
                panels.append((name, bb))

        cross = 0
        for i, (na, a) in enumerate(panels):
            for nb, b in panels[i + 1 :]:
                if overlap(a, b):
                    cross += 1
                    print(f"CROSS-PANEL OVERLAP: {na} ↔ {nb}")

        arrow_issues = audit_svg_arrows(page)
        ellipse_issues = audit_ellipse_containment(page)

        print(f"\nSummary: clip={clip_issues} out_of_body={oob} cross_panel={cross} arrows={arrow_issues} ellipse={ellipse_issues}")
        if clip_issues == 0 and oob == 0 and cross == 0 and arrow_issues == 0 and ellipse_issues == 0:
            print("PASS — figure ready for paper.")

        browser.close()


if __name__ == "__main__":
    main()
