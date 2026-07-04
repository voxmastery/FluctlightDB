#!/usr/bin/env python3
"""Playwright layout audit — overlap, clipping, and out-of-frame checks."""

from __future__ import annotations

from pathlib import Path

from playwright.sync_api import sync_playwright

HTML = Path(__file__).resolve().parent / "fig1_template.html"


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


def main() -> None:
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(device_scale_factor=2)
        page.goto(HTML.as_uri())
        page.wait_for_load_state("networkidle")

        body = page.locator("body").bounding_box()
        assert body
        print(f"Body: {body['width']:.0f}×{body['height']:.0f}px\n")

        # Clipping: each compare-col contains its own children
        clip_issues = 0
        for i, col in enumerate(page.locator(".compare-col").all()):
            cb = col.bounding_box()
            if not cb:
                continue
            for sel in ("h4", "ul", "li"):
                for j, el in enumerate(col.locator(sel).all()):
                    eb = el.bounding_box()
                    if eb and not contains(cb, eb):
                        clip_issues += 1
                        print(f"CLIP compare-col[{i}] {sel}[{j}]: outside parent by "
                              f"R={eb['x']+eb['width']-cb['x']-cb['width']:.1f} "
                              f"B={eb['y']+eb['height']-cb['y']-cb['height']:.1f}")

        # Out of body
        oob = 0
        for sel in (".hero-wrap", ".stack", ".flow-card", ".compare", ".compare-col"):
            for i, el in enumerate(page.locator(sel).all()):
                bb = el.bounding_box()
                if bb and not contains(body, bb):
                    oob += 1
                    print(f"OUT OF BODY: {sel}[{i}] right={bb['x']+bb['width']-body['x']-body['width']:.1f} "
                          f"bottom={bb['y']+bb['height']-body['y']-body['height']:.1f}")

        # Cross-panel overlaps (top-level panels only)
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

        # Small text
        small = 0
        for name, bb in panels:
            pass
        for sel in (".compare-col ul", ".flow-card ol li", ".hero-svg text"):
            for i, el in enumerate(page.locator(sel).all()):
                bb = el.bounding_box()
                if bb and (bb["width"] < 20 or bb["height"] < 14):
                    small += 1
                    print(f"SMALL: {sel}[{i}] {bb['width']:.0f}×{bb['height']:.0f}")

        print(f"\nSummary: clip={clip_issues} out_of_body={oob} cross_panel={cross} small={small}")
        if clip_issues == 0 and oob == 0 and cross == 0:
            print("PASS — no layout defects detected.")

        browser.close()


if __name__ == "__main__":
    main()
