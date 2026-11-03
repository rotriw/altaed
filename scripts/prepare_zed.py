#!/usr/bin/env python3
"""Fetch the pinned editor source into this repository and replay our patch.

No neighboring checkout or build cache is required. --source can reuse an
existing Git checkout as an archive source without modifying that checkout.
"""
import argparse
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile

from patch_editor_baseline import apply as patch_editor

FETCH_TIMEOUT_SECONDS = 180

def git(source, *arguments):
    return subprocess.check_output(["git", "-C", str(source), *arguments])


def prepare(project: Path, source: Path | None = None) -> Path:
    specification = json.loads((project / "vendor/zed-source.json").read_text())
    revision = specification["revision"]
    destination = project / "vendor/zed"
    marker = destination / ".altaed-source-commit"
    if destination.exists():
        if not marker.is_file() or marker.read_text().strip() != revision:
            raise RuntimeError(f"Refusing to overwrite unverified source: {destination}")
        patch_editor(destination)
        return destination

    vendor = destination.parent
    vendor.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".zed-", dir=vendor) as staging:
        staging = Path(staging)
        if source is None:
            source = staging / "checkout"
            source.mkdir()
            subprocess.run(["git", "init", "--quiet", str(source)], check=True)
            subprocess.run(["git", "-C", str(source), "fetch", "--depth=1", specification["repository"], revision],
                check=True, timeout=FETCH_TIMEOUT_SECONDS)
        actual = git(source, "rev-parse", "--verify", f"{revision}^{{commit}}").decode().strip()
        if actual != revision:
            raise RuntimeError("Source revision does not match vendor/zed-source.json")
        archive = git(source, "archive", revision)
        extracted = staging / "source"
        extracted.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as files:
            files.extractall(extracted, filter="data")
        (extracted / marker.name).write_text(revision + "\n")
        patch_editor(extracted)
        # Only publish a fully extracted, successfully patched source tree.
        extracted.rename(destination)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, help="Optional local Zed Git checkout")
    args = parser.parse_args()
    project = Path(__file__).resolve().parents[1]
    print(prepare(project, args.source.resolve() if args.source else None))


if __name__ == "__main__":
    main()
