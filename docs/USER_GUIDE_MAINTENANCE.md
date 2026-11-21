# Maintaining the user guide

Edit resources/guide.en.html and resources/guide.es.html together. They are the authoritative guide content. Keep section IDs and data-key shortcut identifiers identical across languages; translate visible text. The HTML uses headings without skipped levels, ordered task steps, landmarks and six two-column shortcut tables with captions and scoped headers.

Update both sources in the same change as any new or changed command, shortcut, dialog, default or supported format. Check the current menu labels, keyboard focus and confirmation steps in the app. Remove superseded instructions and keep acceptance results in VALIDATION.md. Include shortcut changes in SHORTCUTS.md and visible changes in the changelog.

Run:

```powershell
python tools/build_user_guide.py
python tools/build_user_guide.py --check
cargo test -p sonitrelix-windows-ui --locked
```

The standard-library generator creates self-contained docs/user-guide.en.html and user-guide.es.html and synchronizes resources/en.json and es.json guide text. Do not edit those generated guide fields or HTML pages independently. No Python or internet is needed to read the guide at runtime. The CI check rejects stale outputs and invalid heading/link/table structure. Rust tests check all configurable shortcut defaults against both HTML tables, and verify embedded guide extraction and native Help controls.

F1 still opens the modeless read-only text viewer. Its Open HTML guide button opens the same content in the default browser. Both HTML languages are embedded and refreshed under data/docs, so EXE-only updates also refresh the guide and its language links. Portable packages include the standalone HTML under docs. About and Changelog remain read-only text.

Manual acceptance: open F1 in each language, verify text focus/search/sections, activate Open HTML guide, navigate headings and tables using NVDA/JAWS, follow contents and language links, test browser search, enlarged text, narrow width, dark/high-contrast mode and printing. Return with Alt+Tab and verify editor focus. Do not claim reader compatibility from structure checks alone.
