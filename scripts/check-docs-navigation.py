#!/usr/bin/env python3
"""Check the built docs-to-storefront links, including the Keel-only routes."""

from html.parser import HTMLParser
from pathlib import Path


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__()
        self.elements = []
        self.feed(path.read_text())

    def handle_starttag(self, tag, attrs):
        self.elements.append((tag, dict(attrs)))

    def links(self, class_name=None):
        return [
            attrs for tag, attrs in self.elements
            if tag == "a" and (
                class_name is None or class_name in attrs.get("class", "").split()
            )
        ]


def check(dist):
    pages = list(dist.rglob("*.html"))
    docs = [path for path in pages if 'class="VPNavBar' in path.read_text()]
    assert docs, "No built VitePress pages found"
    for path in docs:
        page = Page(path)
        for class_name in ("VPNavBarMenuLink", "pro-nav-link"):
            links = [a for a in page.links(class_name) if a.get("href") == "/lockdocs/pricing"]
            assert links, f"{path}: missing {class_name} pricing link"
            assert all(a.get("target") == "_self" for a in links), (
                f"{path}: pricing must leave the VitePress router for Keel"
            )
        for route in ("guide/quickstart", "reference/tools", "benchmarks"):
            assert any(a.get("href") == f"/lockdocs/{route}" for a in page.links()), (
                f"{path}: free {route} link missing"
            )
            assert (dist / f"{route}.html").is_file(), f"Missing free page: {route}"

    hero = Page(dist / "index.html").links("VPButton")
    assert any(a.get("href") == "/lockdocs/pricing" and a.get("target") == "_self" for a in hero), (
        "Home hero must link directly to pricing outside the docs router"
    )
    assert any(a.get("href") == "/lockdocs/guide/quickstart" and "brand" in a.get("class", "").split() for a in hero), (
        "Free quickstart must remain the primary home action"
    )
    pricing = Page(dist / "pricing.html")
    assert any(a.get("href") == "/lockdocs/recover" for a in pricing.links()), (
        "Pricing must link to licence recovery under the deployed base path"
    )
    packs = {attrs.get("value") for tag, attrs in pricing.elements if tag == "option"}
    assert {"1", "5", "10", "25", "50"} <= packs, "Missing existing seat packs"
    recovery = Page(dist / "recover.html")
    assert any(attrs.get("data-keel-money-form") == "recover" for _, attrs in recovery.elements), (
        "Missing existing recovery page"
    )
    print(f"Docs navigation checked: {len(docs)} pages, free setup, pricing and recovery")


if __name__ == "__main__":
    check(Path(__file__).resolve().parents[1] / "docs/.vitepress/dist")
