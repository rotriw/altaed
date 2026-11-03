# Third-party source and assets

The editor dependency is Zed, pinned by `vendor/zed-source.json`. Its source is
obtained by `scripts/prepare_zed.py`; original license files remain in that tree.
The consumed editor, language and UI manifests declare GPL-3.0-or-later; GPUI
and other crates have their own declared licenses. The root Zed repository also
contains other licensing terms. Consult each consumed component's license.

The C++ language query files retain their upstream GPL license at
`crates/altaed/languages/LICENSE-GPL`.

The outlined welcome wordmark was derived from IBM Plex Sans. The SIL Open Font
License and copyright notice are retained at
`crates/altaed/assets/LICENSE-IBM-PLEX.txt`. No ENDFIELD game assets are included.

This notice records third-party provenance; it does not assign a license to
altaed's original code. A project-wide license has not yet been selected.
