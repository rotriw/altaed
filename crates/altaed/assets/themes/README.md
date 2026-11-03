# Bundled themes

Fourteen offline themes, including all six variants declared by Horizon v2.0.2:
Horizon / Italic / Bold and Horizon Bright / Italic / Bold.

`sources/*/SOURCE.json` records immutable upstream revisions, distribution hashes
where generated themes came from the official marketplace, and SHA-256 hashes of
all retained source files. Original licenses are retained in each directory.

Run `python3 scripts/convert_themes.py` from the repository to regenerate
`bundled.json` and the web prototype's `themes.json`. Conversion is offline and
does not execute any upstream package scripts. Keep both outputs together.

TextMate foregrounds and font styles map to the C++ tree-sitter captures used by
our pinned editor. Scope systems differ, so this is an explicit adaptation rather
than a VS Code extension runtime. Horizon's warm Bright background, dark palette,
syntax transparency appearance and italic/bold variants are preserved; alpha
syntax colors are composited over the editor surface for GPUI TextStyle. Application surfaces
use semantic roles; dark separators are raised slightly for visible boundaries.
Original syntax contrast is retained, including subdued upstream comments.
