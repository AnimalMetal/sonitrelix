"""Build offline HTML and the native F1 text from the same bilingual guide sources.

Run after editing resources/guide.en.html or guide.es.html. --check verifies generated
files, heading/link/table structure and parity without changing anything. Standard
library only; Python is a documentation build tool, not an application dependency.
"""
import argparse
import json
from html import escape
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STYLE = """+:root { color-scheme: light dark; }
* { box-sizing: border-box; }
html { scroll-padding-block-start: 1.5rem; }
body { margin: 0; background: #f3f6fa; color: #182638; font: 1.125rem/1.65 system-ui, sans-serif; }
main { max-width: 78rem; margin: 2rem auto; padding: clamp(1rem, 4vw, 3.5rem); background: #fff; border: 1px solid #c6d0dd; border-radius: .6rem; }
h1, h2, h3 { line-height: 1.25; overflow-wrap: anywhere; }
h1 { font-size: clamp(2rem, 5vw, 3.2rem); margin-block-start: 0; }
h2 { font-size: 1.75rem; margin-block-start: 2.7rem; padding-block-start: .5rem; border-block-start: 2px solid #c6d0dd; }
h3 { font-size: 1.3rem; margin-block-start: 2rem; }
p, li, dd { max-width: 78ch; }
li + li { margin-block-start: .5rem; }
a { color: #124c96; text-underline-offset: .18em; }
a:hover { text-decoration-thickness: .18em; }
:focus-visible { outline: 3px solid #8b3700; outline-offset: 4px; }
.skip { display: block; padding: .75rem 1rem; background: #fff; }
.lead { font-size: 1.3rem; }
nav { padding: .2rem 1.5rem 1rem; border: 1px solid #c6d0dd; background: #f3f6fa; }
nav h2 { border: 0; margin-block-start: 1rem; }
dt { font-weight: 700; margin-block-start: .8rem; }
dd { margin-inline-start: 1.2rem; }
kbd, code { font: inherit; font-family: ui-monospace, Consolas, monospace; overflow-wrap: anywhere; }
kbd { padding: .06em .24em; border: 1px solid #a5b3c5; border-radius: .2em; }
table { width: 100%; border-collapse: collapse; margin-block: 1rem 2rem; table-layout: fixed; }
caption { text-align: start; font-weight: 600; padding-block: .6rem; }
th, td { border: 1px solid #a5b3c5; text-align: start; vertical-align: top; padding: .7rem; overflow-wrap: anywhere; }
thead th { background: #e3ecf7; }
th:first-child { width: 36%; }
tbody th { font-weight: 600; }
tbody tr:nth-child(even) { background: #f3f6fa; }
footer { margin-block-start: 3rem; border-block-start: 1px solid #c6d0dd; padding-block-start: 1rem; }
@media (max-width: 40rem) { main { margin: 0; border: 0; border-radius: 0; } th, td { padding: .5rem; } }
@media (prefers-color-scheme: dark) {
 body { background: #111820; color: #e5edf5; }
 main, .skip { background: #17222e; }
 nav, tbody tr:nth-child(even) { background: #1d2c3c; }
 thead th { background: #293e55; }
 a { color: #a4ceff; } :focus-visible { outline-color: #ffd084; }
}
@media (forced-colors: active) { body, main, nav, .skip, th, td { background: Canvas; color: CanvasText; } a { color: LinkText; } }
@media print {
 body { background: white; color: black; font-size: 11pt; }
 main { margin: 0; padding: 0; max-width: none; border: 0; }
 main, nav, thead th, tbody tr:nth-child(even) { background: white; color: black; }
 .skip, footer a { display: none; } a { color: black; }
 h2, h3, caption { break-after: avoid; } tr { break-inside: avoid; }
 thead { display: table-header-group; }
}
""".strip()


class Guide(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack = []
        self.ids = set()
        self.links = []
        self.labels = []
        self.headings = []
        self.keys = set()
        self.text = []
        self.tables = 0
        self.captions = 0
        self.cells = 0
        self.in_row = False

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag in {"script", "iframe", "img", "style"}:
            raise ValueError(f"Unexpected guide source element: {tag}")
        if any(a.startswith("on") for a in attrs):
            raise ValueError("Guide must not contain scripted event handlers")
        if "id" in attrs:
            assert attrs["id"] not in self.ids, f"Duplicate ID {attrs['id']}"
            self.ids.add(attrs["id"])
        if "aria-labelledby" in attrs:
            self.labels.extend(attrs["aria-labelledby"].split())
        if tag == "a":
            self.links.append(attrs["href"])
        if "data-key" in attrs:
            assert attrs["data-key"] not in self.keys
            self.keys.add(attrs["data-key"])
        if tag.startswith("h") and len(tag) == 2 and tag[1].isdigit():
            level = int(tag[1])
            assert level <= 3
            assert not self.headings or level <= self.headings[-1] + 1, "Skipped heading level"
            self.headings.append(level)
            self.text.append("\n\n" + ("## " if level > 1 else ""))
        elif tag in {"p", "dl", "dt", "dd", "ul", "ol", "table", "caption"}:
            self.text.append("\n\n")
        elif tag == "li":
            self.text.append("\n- ")
        elif tag == "tr":
            self.text.append("\n")
            self.in_row = True
            self.cells = 0
        elif tag in {"th", "td"}:
            if self.cells:
                self.text.append(": ")
            self.cells += 1
        if tag == "th":
            assert attrs.get("scope") in {"col", "row"}, "Table header lacks scope"
        if tag == "table":
            self.tables += 1
        if tag == "caption":
            self.captions += 1
        self.stack.append(tag)

    def handle_endtag(self, tag):
        assert self.stack and self.stack[-1] == tag, f"Unbalanced closing tag {tag}"
        self.stack.pop()
        if tag == "tr":
            assert self.cells == 2, "Shortcut tables must have two columns"
            self.in_row = False
        if tag in {"p", "li", "dt", "dd", "tr", "caption"} or tag in {"h1", "h2", "h3"}:
            self.text.append("\n")

    def handle_data(self, data):
        if data.strip():
            if data[:1].isspace():
                self.text.append(" ")
            self.text.append(" ".join(data.split()))
            # Preserve whitespace between adjacent inline elements and words.
            if data[-1:].isspace():
                self.text.append(" ")

    def validate(self):
        assert not self.stack, f"Unclosed elements {self.stack}"
        assert self.headings[0] == 1 and self.headings.count(1) == 1
        assert self.tables == self.captions == 6
        assert set(self.labels) <= self.ids
        for link in self.links:
            assert (link.startswith("#") and link[1:] in self.ids) or link in {
                "user-guide.en.html", "user-guide.es.html",
                "https://github.com/masonasons/FastPlugs/releases/tag/latest",
            }, f"Invalid guide link {link}"

    def plaintext(self):
        lines = [line.strip() for line in "".join(self.text).splitlines()]
        result = []
        for line in lines:
            if line or (result and result[-1]):
                result.append(line)
        return "\n".join(result).strip() + "\n"


def outputs():
    parsed = {}
    for lang in ("en", "es"):
        source = (ROOT / f"resources/guide.{lang}.html").read_text(encoding="utf-8")
        guide = Guide()
        guide.feed(source)
        guide.close()
        guide.validate()
        parsed[lang] = guide
        title = "Sonitrelix user guide" if lang == "en" else "Guía de Sonitrelix"
        skip = "Skip to the user guide" if lang == "en" else "Saltar a la guía"
        back = "Back to contents" if lang == "en" else "Volver al contenido"
        description = "Beginner to advanced, keyboard workflows and shortcuts." if lang == "en" else "Desde principiante hasta avanzado, uso con teclado y tablas de atajos."
        page = f'''<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="description" content="{escape(title)} — {description}">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>{title}</title>
<style>{STYLE}</style>
</head>
<body>
<a class="skip" href="#guide">{skip}</a>
<main id="guide" tabindex="-1">
{source.strip()}
<footer><p><a href="#contents">{back}</a></p></footer>
</main>
</body>
</html>
'''
        yield ROOT / f"docs/user-guide.{lang}.html", page
        resource = ROOT / f"resources/{lang}.json"
        data = json.loads(resource.read_text(encoding="utf-8"))
        data["guide"] = guide.plaintext()
        yield resource, json.dumps(data, ensure_ascii=False, indent=2) + "\n"
    assert parsed["en"].ids == parsed["es"].ids, "Translated sections differ"
    assert parsed["en"].keys == parsed["es"].keys, "Translated shortcut tables differ"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    # Validate both languages before writing anything.
    files = list(outputs())
    stale = []
    for path, text in files:
        if args.check:
            if not path.is_file() or path.read_text(encoding="utf-8") != text:
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.write_text(text, encoding="utf-8")
    if stale:
        parser.exit(1, "Guide files are stale; run python tools/build_user_guide.py: " + ", ".join(stale) + "\n")
    print("Verified bilingual guide structure, links, six two-column shortcut tables and F1 text synchronization." if args.check else "Built English/Spanish HTML and synchronized the native F1 guide.")


if __name__ == "__main__":
    main()
