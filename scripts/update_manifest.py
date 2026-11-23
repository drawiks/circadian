#!/usr/bin/env python3
"""Build the signed-update manifest from release artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from datetime import datetime, timezone
from pathlib import Path


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def artifact(repo: str, tag: str, version: str, dist: Path, suffix: str) -> dict:
    name = f"Circadian-{version}-{suffix}" if suffix else f"Circadian-Setup-{version}.exe"
    path = dist / name
    if not path.is_file():
        raise SystemExit(f"missing release artifact: {path}")
    return {
        "url": f"https://github.com/{repo}/releases/download/{tag}/{name}",
        "sha256": sha256(path),
        "size": path.stat().st_size,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--dist", type=Path, required=True)
    parser.add_argument("--published-at", default=None)
    args = parser.parse_args()

    if args.tag != f"v{args.version}":
        raise SystemExit(f"tag {args.tag!r} does not match version {args.version!r}")
    published_at = args.published_at or datetime.now(timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )

    manifest = {
        "schema_version": 1,
        "version": args.version,
        "tag": args.tag,
        "published_at": published_at,
        "notes_url": f"https://github.com/{args.repo}/releases/tag/{args.tag}",
        "installer": artifact(args.repo, args.tag, args.version, args.dist, ""),
        "portable": artifact(
            args.repo, args.tag, args.version, args.dist, "portable.zip"
        ),
    }

    path = args.dist / "latest.json"
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(path)
    print(published_at)


if __name__ == "__main__":
    main()
