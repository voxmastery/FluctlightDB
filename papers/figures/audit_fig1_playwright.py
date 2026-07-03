#!/usr/bin/env python3
"""Playwright layout audit — logs element bounding boxes and overlap pairs."""

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


def main() -> None:
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page()
        page.goto(HTML.as_uri())
        page.wait_for_load_state("networkidle")

        selectors = [
            ".hero-wrap",
            ".stack .card",
            ".bottom .flow-card",
            ".compare",
            ".hero-svg circle",
            ".hero-svg text",
        ]
        boxes: list[tuple[str, dict]] = []
        for sel in selectors:
            for i, el in enumerate(page.locator(sel).all()):
                bb = el.bounding_box()
                if bb:
                    boxes.append((f"{sel}[{i}]", bb))

        print(f"Audited {len(boxes)} elements\n")
        overlaps = []
        for i, (na, a) in enumerate(boxes):
            for nb, b in boxes[i + 1 :]:
                if overlap(a, b):
                    overlaps.append((na, nb, a, b))

        if overlaps:
            print(f"WARNING: {len(overlaps)} overlap pairs (may be intentional nesting):\n")
            for na, nb, a, b in overlaps[:25]:
                print(f"  {na} ↔ {nb}")
        else:
            print("No unexpected overlaps detected.")

        # Size warnings
        print("\nSmall elements (width or height < 24px):")
        for name, bb in boxes:
            if bb["width"] < 24 or bb["height"] < 24:
                print(f"  {name}: {bb['width']:.0f}×{bb['height']:.0f}")

        browser.close()


if __name__ == "__main__":
    main()
