#!/usr/bin/env python3
"""Check that this app's Cargo.lock uses core/node/Cargo.lock's versions.

The Linux app links Raven's engine (node/crates/raven-client) by path, so its
own Cargo.lock pins the engine's dependencies a second time. They must stay
the versions the node workspace builds and tests:

1. Every package the engine needs (raven-client and everything it depends on,
   followed through the lock file's dependency lists) is in node/Cargo.lock
   with the same name, version, source and checksum, and where both lock
   files list a dependency of it by name, they name the same version of it
   (a dependency only one of them lists comes from features the other tree
   does not turn on).
2. Every other package whose name is also in node/Cargo.lock (the GTK tree
   shares a few small crates with the node tree) uses one of node's versions,
   unless node has no version in the same semver line (for example toml 1.x
   for GTK beside node's toml 0.5).

Cargo keeps one version per semver line, so where the GTK tree needs a newer
version in a line node also uses, both trees get the newer one. Such a crate
is accepted only when it is listed in ALLOWED with the reason; the entry is
reported as no longer needed once node/Cargo.lock has that version.

Python 3.11+ standard library only. Exit status 0 when both hold, 1 otherwise.

    python3 tools/check-lock-sync.py [Cargo.lock] [core/node/Cargo.lock]
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DESKTOP_LOCK = ROOT / "Cargo.lock"
NODE_LOCK = ROOT / "core" / "node" / "Cargo.lock"
ENGINE = "raven-client"

# Same semver line, another version than node's, because the GTK tree needs
# it. Name -> (desktop version, reason).
ALLOWED = {
    "pkg-config": (
        "0.3.34",
        "system-deps 9 (gtk4-sys 0.11) requires >= 0.3.34 and node/Cargo.lock has 0.3.33; "
        "in the engine tree it is only a build dependency of libsqlite3-sys, whose build "
        "script does not call it for the bundled SQLite (LIBSQLITE3_SYS_USE_PKG_CONFIG unset). "
        "Remove this entry once node/Cargo.lock moves to 0.3.34",
    ),
}


def load(path: Path) -> list[dict]:
    with path.open("rb") as f:
        return tomllib.load(f).get("package", [])


def line(version: str) -> tuple:
    """The semver-compatible line of a version: 1.2.3 -> (1,), 0.3.4 -> (0, 3)."""
    m = re.match(r"(\d+)\.(\d+)\.(\d+)", version)
    if not m:
        return (version,)
    major, minor, patch = (int(x) for x in m.groups())
    if major:
        return (major,)
    if minor:
        return (0, minor)
    return (0, 0, patch)


def key(p: dict) -> tuple:
    return (p["name"], p["version"], p.get("source"))


def resolver(packages: list[dict]):
    """The package a lock file's dependency entry ("name", "name version" or
    "name version (source)") names in that lock file."""
    by_name: dict[str, list[dict]] = {}
    for p in packages:
        by_name.setdefault(p["name"], []).append(p)

    def resolve(dep: str) -> dict:
        parts = dep.split(" ", 2)
        found = by_name.get(parts[0], [])
        if len(parts) >= 2:
            found = [p for p in found if p["version"] == parts[1]]
        if len(parts) == 3:
            source = parts[2].strip("()")
            found = [p for p in found if p.get("source") == source]
        if len(found) != 1:
            raise SystemExit(f"lock file: cannot resolve dependency {dep!r}")
        return found[0]

    return resolve


def engine_closure(packages: list[dict]) -> list[dict]:
    """raven-client and every package it depends on, from the lock's edges."""
    by_name: dict[str, list[dict]] = {}
    for p in packages:
        by_name.setdefault(p["name"], []).append(p)
    resolve = resolver(packages)

    roots = by_name.get(ENGINE, [])
    if len(roots) != 1:
        raise SystemExit(f"lock file: expected one {ENGINE} package, found {len(roots)}")
    seen: dict[tuple, dict] = {}
    todo = [roots[0]]
    while todo:
        p = todo.pop()
        if key(p) in seen:
            continue
        seen[key(p)] = p
        todo.extend(resolve(d) for d in p.get("dependencies", []))
    return sorted(seen.values(), key=key)


def is_allowed(p: dict, node_versions: dict[str, set[str]]) -> bool:
    """Listed in ALLOWED at this version, beside node's version of the same line."""
    want = ALLOWED.get(p["name"], (None, ""))[0]
    same_line = [v for v in node_versions.get(p["name"], ()) if line(v) == line(p["version"])]
    return want == p["version"] and bool(same_line)


def link_versions(p: dict, resolve) -> dict[str, str]:
    """A package's dependencies, by name, at the version the lock gives each."""
    return {resolve(d)["name"]: resolve(d)["version"] for d in p.get("dependencies", [])}


def allowed_text(p: dict, node_versions: dict[str, set[str]], where: str) -> str:
    node_has = ", ".join(sorted(node_versions.get(p["name"], ())))
    return f"{where}: {p['name']} {p['version']} (node has {node_has}): {ALLOWED[p['name']][1]}"


def main(argv: list[str]) -> int:
    desktop_path = Path(argv[1]) if len(argv) > 1 else DESKTOP_LOCK
    node_path = Path(argv[2]) if len(argv) > 2 else NODE_LOCK
    desktop = load(desktop_path)
    node = load(node_path)
    node_by_key = {key(p): p for p in node}
    node_versions: dict[str, set[str]] = {}
    for p in node:
        node_versions.setdefault(p["name"], set()).add(p["version"])

    errors: list[str] = []

    closure = engine_closure(desktop)
    closure_keys = {key(p) for p in closure}
    desktop_resolve, node_resolve = resolver(desktop), resolver(node)
    allowed: list[str] = []
    for p in closure:
        twin = node_by_key.get(key(p))
        if twin is not None:
            mine, theirs = link_versions(p, desktop_resolve), link_versions(twin, node_resolve)
            for dep in sorted(mine.keys() & theirs.keys()):
                # A dependency ALLOWED at another version is reported once, below.
                if mine[dep] != theirs[dep] and ALLOWED.get(dep, (None,))[0] != mine[dep]:
                    errors.append(
                        f"engine tree: {p['name']} {p['version']} depends on {dep} "
                        f"{mine[dep]}, node/Cargo.lock on {dep} {theirs[dep]}"
                    )
        if twin is None and is_allowed(p, node_versions):
            allowed.append(allowed_text(p, node_versions, "engine tree"))
        elif twin is None:
            errors.append(
                f"engine tree: {p['name']} {p['version']} is not in node/Cargo.lock "
                f"(node has {sorted(node_versions.get(p['name'], [])) or 'none'})"
            )
        elif twin.get("checksum") != p.get("checksum"):
            errors.append(f"engine tree: {p['name']} {p['version']} has another checksum")

    shared = exact = 0
    other_lines: list[str] = []
    for p in sorted(desktop, key=key):
        name, version = p["name"], p["version"]
        if key(p) in closure_keys or name not in node_versions:
            continue
        shared += 1
        if version in node_versions[name]:
            twin = node_by_key.get(key(p))
            if twin is not None and twin.get("checksum") != p.get("checksum"):
                errors.append(f"{name} {version} has another checksum than in node/Cargo.lock")
            else:
                exact += 1
            continue
        same_line = sorted(v for v in node_versions[name] if line(v) == line(version))
        if not same_line:
            other_lines.append(
                f"{name} {version} (node has {', '.join(sorted(node_versions[name]))}: another semver line)"
            )
        elif is_allowed(p, node_versions):
            allowed.append(allowed_text(p, node_versions, "GTK tree"))
        else:
            errors.append(
                f"{name} {version}: node/Cargo.lock has {', '.join(same_line)} in the same "
                f"semver line; use it (cargo update -p {name}@{version} --precise {same_line[-1]})"
            )

    for name, (version, _) in ALLOWED.items():
        if version in node_versions.get(name, set()):
            print(f"note: ALLOWED[{name!r}] is no longer needed (node/Cargo.lock has {version})")

    in_node = sum(1 for p in closure if key(p) in node_by_key)
    print(f"{desktop_path} against {node_path}")
    print(f"  engine tree ({ENGINE} and its dependencies): {len(closure)} packages, "
          f"{in_node} identical in node/Cargo.lock")
    print(f"  outside the engine tree, named as in node/Cargo.lock: {shared} packages: {exact} at one of "
          f"node's versions, {len(other_lines)} on another semver line (GTK only)")
    for text in other_lines:
        print(f"  separate line (GTK only): {text}")
    for text in allowed:
        print(f"  allowed: {text}")
    for text in errors:
        print(f"  ERROR: {text}")
    print("OK" if not errors else f"FAILED ({len(errors)} errors)")
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
