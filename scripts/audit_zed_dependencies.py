#!/usr/bin/env python3
"""Inspect local Cargo edges without modifying or building the source checkout.

This conservatively traverses available local manifests: all targets and optional
normal/build dependencies are included; dev and external crates are excluded.
The graph is incomplete whenever an unresolved dependency is reported.
Cargo metadata, when available, is the authority for a resolved target graph.
"""

import argparse
import io
import json
import posixpath
from pathlib import Path
import subprocess
import tarfile
import tomllib


def git(source: Path, *arguments: str) -> bytes:
    return subprocess.check_output(["git", "-C", str(source), *arguments])


def read_manifests(source: Path, revision: str | None) -> tuple[dict, str]:
    if revision:
        commit = git(source, "rev-parse", "--verify", f"{revision}^{{commit}}").decode().strip()
        paths = [
            path for path in git(source, "ls-tree", "-r", "--name-only", commit).decode().splitlines()
            if path == "Cargo.toml" or path.endswith("/Cargo.toml")
        ]
        archive = git(source, "archive", commit, "--", *paths)
        with tarfile.open(fileobj=io.BytesIO(archive)) as files:
            manifests = {}
            for member in files:
                if member.name in paths:
                    stream = files.extractfile(member)
                    if stream is None:
                        raise ValueError(f"Not a regular manifest: {member.name}")
                    manifests[member.name] = tomllib.loads(stream.read().decode())
        return manifests, commit
    paths = [source / "Cargo.toml"]
    for directory in ("crates", "tooling", "extensions"):
        paths.extend(sorted((source / directory).rglob("Cargo.toml")))
    return {
        path.relative_to(source).as_posix(): tomllib.loads(path.read_text())
        for path in paths
    }, "working-tree"


def dependencies(manifest: dict):
    for section in [manifest, *manifest.get("target", {}).values()]:
        for kind in ("dependencies", "build-dependencies"):
            yield from section.get(kind, {}).items()


def audit(manifests: dict, roots: list[str]) -> dict:
    inherited = manifests["Cargo.toml"]["workspace"].get("dependencies", {})
    by_name = {
        manifest["package"]["name"]: path
        for path, manifest in manifests.items() if "package" in manifest
    }
    graphs = {}
    for root in roots:
        if root not in by_name:
            graphs[root] = {"error": "package absent from this source"}
            continue
        pending = [by_name[root]]
        seen = set()
        edges = {}
        unresolved = []
        while pending:
            path = pending.pop()
            if path in seen:
                continue
            seen.add(path)
            manifest = manifests[path]
            name = manifest["package"]["name"]
            targets = set()
            for key, spec in dependencies(manifest):
                if not isinstance(spec, dict):
                    continue
                base = posixpath.dirname(path)
                if spec.get("workspace"):
                    if key not in inherited:
                        unresolved.append({"from": name, "dependency": key, "reason": "missing workspace declaration"})
                        continue
                    spec = inherited[key]
                    base = ""
                if not isinstance(spec, dict) or "path" not in spec:
                    continue
                target = posixpath.normpath(posixpath.join(base, spec["path"], "Cargo.toml"))
                if target not in manifests:
                    unresolved.append({"from": name, "dependency": key, "manifest": target, "reason": "missing local manifest"})
                    continue
                targets.add(manifests[target]["package"]["name"])
                pending.append(target)
            edges[name] = sorted(targets)
        names = sorted(edges)
        graphs[root] = {
            "graph_complete": not unresolved,
            "local_package_count_including_root": len(names),
            "packages": names,
            "application_dependencies_present": sorted(set(names) & {
                "project", "workspace", "client", "rpc", "git", "git_ui", "collab", "telemetry", "db", "zed",
            }),
            "unresolved": sorted(unresolved, key=lambda item: (item["from"], item["dependency"])),
            "edges": dict(sorted(edges.items())),
        }
    return graphs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--revision", help="Read the committed source without restoring deleted files")
    parser.add_argument("--roots", nargs="+", default=["text", "language", "multi_buffer", "editor", "zed_editor"])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    source = args.source.resolve()
    manifests, identity = read_manifests(source, args.revision)
    result = {
        "source": str(source),
        "revision": identity,
        "method": "local normal/build manifest closure; all targets and optional edges; excludes dev/external dependencies; not Cargo feature resolution",
        "roots": audit(manifests, args.roots),
    }
    serialized = json.dumps(result, indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(serialized)
    else:
        print(serialized, end="")


if __name__ == "__main__":
    main()
