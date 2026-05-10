#!/usr/bin/env python3
"""Publish the Rerun workspace crates to a private Cargo registry.

This publishes only publishable packages under `crates/`, in dependency order.
Examples, tests, docs snippets, Python packages, and publish=false crates are
ignored.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import os
from collections import deque
from pathlib import Path


def run(cmd: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd), flush=True)
    return subprocess.run(cmd, check=check, text=True)


def cargo_metadata() -> dict:
    output = subprocess.check_output(
        ["cargo", "+stable", "metadata", "--format-version", "1", "--no-deps"],
        text=True,
    )
    return json.loads(output)


def publishable_workspace_packages(metadata: dict) -> dict[str, dict]:
    root = Path(metadata["workspace_root"]).resolve()
    packages = {}

    for package in metadata["packages"]:
        manifest = Path(package["manifest_path"]).resolve()
        try:
            relative = manifest.relative_to(root)
        except ValueError:
            continue

        if not str(relative).startswith("crates/"):
            continue

        # cargo metadata represents `publish = false` as an empty list.
        if package.get("publish") == []:
            continue

        packages[package["id"]] = package

    return packages


def topological_order(packages: dict[str, dict]) -> list[dict]:
    selected_ids = set(packages)
    outgoing: dict[str, set[str]] = {package_id: set() for package_id in selected_ids}
    incoming_count: dict[str, int] = {package_id: 0 for package_id in selected_ids}

    for package_id, package in packages.items():
        for dep in package["dependencies"]:
            dep_path = dep.get("path")
            if dep_path is None:
                continue

            dep_id = next(
                (
                    candidate_id
                    for candidate_id, candidate in packages.items()
                    if Path(candidate["manifest_path"]).parent.resolve() == Path(dep_path).resolve()
                ),
                None,
            )

            if dep_id is not None and dep_id in selected_ids:
                if package_id not in outgoing[dep_id]:
                    outgoing[dep_id].add(package_id)
                    incoming_count[package_id] += 1

    queue = deque(sorted((id_ for id_, count in incoming_count.items() if count == 0), key=lambda id_: packages[id_]["name"]))
    ordered = []

    while queue:
        package_id = queue.popleft()
        ordered.append(packages[package_id])

        for dependent_id in sorted(outgoing[package_id], key=lambda id_: packages[id_]["name"]):
            incoming_count[dependent_id] -= 1
            if incoming_count[dependent_id] == 0:
                queue.append(dependent_id)

    if len(ordered) != len(packages):
        stuck = sorted(packages[id_]["name"] for id_, count in incoming_count.items() if count > 0)
        raise RuntimeError(f"dependency cycle or unresolved internal ordering: {stuck}")

    return ordered


def publish_package(
    name: str,
    registry: str,
    *,
    dry_run: bool,
    allow_dirty: bool,
    retries: int,
    retry_delay: float,
) -> None:
    cmd = ["cargo", "+stable", "publish", "-p", name, "--registry", registry]
    if dry_run:
        cmd.append("--dry-run")
    if allow_dirty:
        cmd.append("--allow-dirty")

    for attempt in range(1, retries + 1):
        print("+", " ".join(cmd), flush=True)
        env = os.environ.copy()
        env.setdefault("RERUN_DISABLE_WEB_VIEWER_SERVER", "1")
        result = subprocess.run(
            cmd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=env,
        )
        print(result.stdout, end="", flush=True)

        if result.returncode == 0:
            return

        already_published_markers = (
            "already uploaded",
            "already exists",
            "crate version already exists",
            "status 409",
        )
        if any(marker in result.stdout.lower() for marker in already_published_markers):
            print(f"{name} already exists in {registry}; skipping", flush=True)
            return

        if attempt == retries:
            raise subprocess.CalledProcessError(result.returncode, cmd)

        print(
            f"publish failed for {name}; retrying in {retry_delay:.0f}s "
            f"({attempt}/{retries})",
            file=sys.stderr,
            flush=True,
        )
        time.sleep(retry_delay)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--registry", default="azakura-rerun")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--allow-dirty", action="store_true")
    parser.add_argument("--retries", type=int, default=5)
    parser.add_argument("--retry-delay", type=float, default=20.0)
    parser.add_argument("--list", action="store_true", help="print publish order and exit")
    args = parser.parse_args()

    metadata = cargo_metadata()
    packages = publishable_workspace_packages(metadata)
    ordered = topological_order(packages)

    if args.list:
        for package in ordered:
            print(package["name"])
        return

    for package in ordered:
        publish_package(
            package["name"],
            args.registry,
            dry_run=args.dry_run,
            allow_dirty=args.allow_dirty,
            retries=args.retries,
            retry_delay=args.retry_delay,
        )


if __name__ == "__main__":
    main()
