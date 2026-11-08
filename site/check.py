"""Prüft die statische Seite: interne Links und Anker zeigen auf vorhandene Dateien,
jede Seite hat lang, title und viewport, und es werden keine fremden Skripte,
Schriften oder Stylesheets geladen (Datenschutzerklärung verspricht das)."""

from __future__ import annotations

import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit


def is_external(url: str) -> bool:
    """Alles mit Schema (https:, data: …) oder Host (//cdn…) gilt als fremde Ressource."""
    parts = urlsplit(url)
    return bool(parts.scheme or parts.netloc)


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links: list[str] = []
        self.ids: set[str] = set()
        self.lang = None
        self.title = False
        self.viewport = False
        self.external_assets: list[str] = []
        self.scripts = 0

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            self.ids.add(a["id"])
        if tag == "html":
            self.lang = a.get("lang")
        if tag == "title":
            self.title = True
        if tag == "meta" and a.get("name") == "viewport":
            self.viewport = True
        if tag == "script":
            self.scripts += 1
        if tag == "a" and a.get("href"):
            self.links.append(a["href"])
        if tag == "link" and a.get("href"):
            href = a["href"]
            rel = set((a.get("rel") or "").lower().split())
            if rel & {"stylesheet", "icon", "apple-touch-icon", "preload", "modulepreload"}:
                if is_external(href):
                    self.external_assets.append(href)
                else:
                    self.links.append(href)
            elif "alternate" in rel:
                self.links.append(href)
        if tag == "img" and a.get("src"):
            if is_external(a["src"]):
                self.external_assets.append(a["src"])
            else:
                self.links.append(a["src"])


def resolve(root: Path, page: Path, href: str) -> Path | None:
    parts = urlsplit(href)
    if parts.scheme or parts.netloc:
        return None
    if not parts.path:
        return page
    base = root if parts.path.startswith("/") else page.parent
    target = (base / parts.path.lstrip("/")).resolve()
    if target.is_dir() or parts.path.endswith("/"):
        target = target / "index.html"
    return target


def main(root: Path) -> int:
    root = root.resolve()
    pages = {p.resolve(): Page() for p in root.rglob("*.html")}
    for path, parser in pages.items():
        parser.feed(path.read_text(encoding="utf-8"))

    errors: list[str] = []
    for path, parser in pages.items():
        rel = path.relative_to(root)
        if not parser.lang:
            errors.append(f"{rel}: <html lang> fehlt")
        if not parser.title:
            errors.append(f"{rel}: <title> fehlt")
        if not parser.viewport:
            errors.append(f"{rel}: viewport-Meta fehlt")
        if parser.scripts:
            errors.append(f"{rel}: enthält <script> (CSP erlaubt keine Skripte)")
        for asset in parser.external_assets:
            errors.append(f"{rel}: lädt fremde Ressource {asset}")
        for href in parser.links:
            target = resolve(root, path, href)
            if target is None:
                continue
            if not target.exists():
                errors.append(f"{rel}: Link ins Leere: {href}")
                continue
            fragment = urlsplit(href).fragment
            if fragment and target in pages and fragment not in pages[target].ids:
                errors.append(f"{rel}: Anker fehlt: {href}")

    for e in errors:
        print(e)
    print(f"{len(pages)} Seiten geprüft, {len(errors)} Fehler.")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1] if len(sys.argv) > 1 else "public")))
