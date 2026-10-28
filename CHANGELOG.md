# Changelog

All notable changes to NextLine. Release notes on GitHub are taken from the matching section below.

## [0.2.1] - 2026-10-28

### Added
- **About** section in Settings with a link to contact the author on LinkedIn, report an issue and view the source.

## [0.2.0] - 2026-10-28

### Added
- **Update check.** NextLine looks for a newer GitHub release about once a day and shows a slim banner with *View release* and *Skip this version*. It is notify-only: nothing is downloaded or run for you. Turn it off, or check on demand, under **Settings > Updates**.
- **Agent skill** (`skills/nextline-scripts`) so AI coding agents can write NextLine scripts for you, with a JSON schema, examples and a validator.
- Public source release under the MIT license.

### Changed
- Sample scripts use generic example names.
- Wording refers to "meeting apps" rather than a specific product.

### Privacy
- The update check is the only network request NextLine makes. It is a single request to `api.github.com` for the latest release and sends nothing about you or your scripts. It never runs while a step is being typed and fails silently when offline.

## [0.1.0] - 2026-10-08

### Added
- First release: one-key demo steps (F2 type, F3 skip, Shift+F3 back, Shift+F2 stop), human-paced typing, no clipboard use, presenter mode, hide from screen share, notes, undo/redo, tray mode, light/dark themes, JSON import and export.
