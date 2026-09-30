"""Mock grocery catalog. Prices change on a fixed calendar. All names are fictional."""

from __future__ import annotations

from typing import Any

# (day_offset, price_cents) from 2026-03-01. Later rows replace earlier ones.
PRODUCTS: list[dict[str, Any]] = [
    {
        "sku": "WM-MD-1L",
        "item": "whole_milk",
        "brand": "meadow_dairy",
        "name": "Meadow Dairy whole milk 1L",
        "prices": [(0, 349), (14, 379), (28, 399)],
    },
    {
        "sku": "WM-NF-1L",
        "item": "whole_milk",
        "brand": "north_farm",
        "name": "North Farm whole milk 1L",
        "prices": [(0, 339), (14, 359), (28, 389)],
    },
    {
        "sku": "WM-CF-1L",
        "item": "whole_milk",
        "brand": "clear_filter",
        "name": "Clear Filter whole milk 1L",
        "prices": [(0, 429), (14, 449), (28, 469)],
    },
    {
        "sku": "WM-HB-1L",
        "item": "whole_milk",
        "brand": "house_brand",
        "name": "House brand whole milk 1L",
        "prices": [(0, 199), (14, 219), (28, 229)],
    },
    {
        "sku": "OM-GC-1L",
        "item": "oat_milk",
        "brand": "grain_cup",
        "name": "Grain Cup oat drink 1L",
        "prices": [(0, 429), (18, 459)],
    },
    {
        "sku": "OM-SB-1L",
        "item": "oat_milk",
        "brand": "small_batch",
        "name": "Small Batch oat milk 1L",
        "prices": [(0, 449), (18, 479)],
    },
    {
        "sku": "EG-PC-12",
        "item": "eggs",
        "brand": "pasture_coop",
        "name": "Pasture Coop eggs 12ct",
        "prices": [(0, 549), (21, 599)],
    },
    {
        "sku": "BR-HL-21",
        "item": "bread",
        "brand": "hearth_loaf",
        "name": "Hearth Loaf bread",
        "prices": [(0, 399), (10, 429)],
    },
    {
        "sku": "CF-HR-12",
        "item": "coffee",
        "brand": "harbor_roast",
        "name": "Harbor Roast ground coffee 12oz",
        "prices": [(0, 899), (7, 949), (30, 999)],
    },
    {
        "sku": "YG-CC-5",
        "item": "yogurt",
        "brand": "cup_culture",
        "name": "Cup Culture yogurt 5oz",
        "prices": [(0, 179), (12, 189)],
    },
]


def price_on_day(product: dict[str, Any], day: int) -> int:
    current = int(product["prices"][0][1])
    for offset, cents in product["prices"]:
        if int(offset) <= day:
            current = int(cents)
    return current


def find_sku(sku: str) -> dict[str, Any]:
    for product in PRODUCTS:
        if product["sku"] == sku:
            return product
    raise KeyError(sku)


def describe_catalog() -> str:
    lines = ["Mock catalog (fictional brands, cents, price changes from day 0 = 2026-03-01):"]
    for product in PRODUCTS:
        steps = ", ".join(f"day {d}:${cents / 100:.2f}" for d, cents in product["prices"])
        lines.append(f"  {product['sku']} {product['name']} [{product['brand']}] {steps}")
    return "\n".join(lines)
