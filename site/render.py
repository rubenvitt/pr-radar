"""Erzeugt site/dist aus site/public und setzt Werte aus Umgebungsvariablen ein.

Persönliche Angaben (die Anschrift für Impressum und Datenschutz) stehen nicht im
Repo. Die Seiten enthalten Platzhalter wie {{IMPRESSUM_STRASSE}}; die Werte kommen
beim Build aus gleichnamigen Umgebungsvariablen (bei Cloudflare: Build-Variablen).

Aufruf: python3 render.py <quelle> <ziel> <produktion: 0|1>
Fehlt in der Produktion ein Wert, bricht der Build ab, damit kein Impressum ohne
Anschrift veröffentlicht wird. In Vorschau-Builds steht ein sichtbarer Hinweis.
"""

from __future__ import annotations

import html
import os
import re
import shutil
import sys
from pathlib import Path

# Nur diese Variablen werden eingesetzt; alles andere bleibt unberührt.
FIELDS = {
    "IMPRESSUM_STRASSE": "Straße und Hausnummer",
    "IMPRESSUM_ORT": "PLZ und Ort",
}
TOKEN = re.compile(r"\{\{([A-Z_]+)\}\}")


def main(source: Path, target: Path, production: bool) -> int:
    values = {name: os.environ.get(name, "").strip() for name in FIELDS}
    missing = [name for name, value in values.items() if not value]
    if missing and production:
        print("Produktions-Build abgebrochen, es fehlen Build-Variablen:")
        for name in missing:
            print(f"  {name} ({FIELDS[name]})")
        print("Bei Cloudflare unter Settings → Build → Variables and secrets setzen (site/README.md).")
        return 1

    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(source, target)

    errors: list[str] = []
    for page in sorted(target.rglob("*.html")):
        text = page.read_text(encoding="utf-8")

        def replace(match: re.Match[str]) -> str:
            name = match.group(1)
            if name not in FIELDS:
                errors.append(f"{page.relative_to(target)}: unbekannter Platzhalter {{{{{name}}}}}")
                return match.group(0)
            if values[name]:
                return html.escape(values[name])
            return f'<span class="todo">{FIELDS[name]} folgt beim Deploy</span>'

        rendered = TOKEN.sub(replace, text)
        if rendered != text:
            page.write_text(rendered, encoding="utf-8")

    for e in errors:
        print(e)
    if missing:
        print("Vorschau-Build: Anschrift fehlt, Hinweis statt Wert eingesetzt (" + ", ".join(missing) + ").")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3] == "1"))
