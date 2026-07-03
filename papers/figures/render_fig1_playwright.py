#!/usr/bin/env python3
"""Render Figure 1 via Playwright (HTML/SVG) for pixel-perfect layout."""

from __future__ import annotations

from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
HTML = ROOT / "fig1_template.html"


def render(stem: str = "01-brain-architecture", hero_only: bool = False) -> None:
    if not HTML.exists():
        raise FileNotFoundError(HTML)

    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(device_scale_factor=2)
        page.goto(HTML.as_uri())
        page.wait_for_load_state("networkidle")

        if hero_only:
            el = page.locator(".hero-wrap")
            el.screenshot(path=str(ROOT / f"{stem}.png"))
            # PDF from clipped hero
            box = el.bounding_box()
            if box:
                page.pdf(
                    path=str(ROOT / f"{stem}.pdf"),
                    width=f"{box['width'] + 40}px",
                    height=f"{box['height'] + 40}px",
                    print_background=True,
                    margin={"top": "20px", "bottom": "20px", "left": "20px", "right": "20px"},
                )
        else:
            body = page.locator("body")
            body.screenshot(path=str(ROOT / f"{stem}.png"))
            page.pdf(
                path=str(ROOT / f"{stem}.pdf"),
                width="1440px",
                height=f"{page.evaluate('document.body.scrollHeight')}px",
                print_background=True,
                margin={"top": "0", "bottom": "0", "left": "0", "right": "0"},
            )

        browser.close()

    print(f"wrote {stem}.pdf / .png (playwright)")


if __name__ == "__main__":
    render("01-brain-architecture")
    render("01-brain-hero", hero_only=True)
