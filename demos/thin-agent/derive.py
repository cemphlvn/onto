#!/usr/bin/env python3
"""Derive a Rust workspace's architecture as an onto category, mapped into
`ThinAgentDependencies` (standards/thin-agent.onto).

    python3 demos/thin-agent/derive.py [demos/thin-agent/roles.toml]
    onto functor demos/thin-agent/code.onto

People write the roles (which crate is a Host, Agent, Contract or
Substrate); the arrows come from code: workspace dependencies from
`cargo metadata`, world effects (durable writes, processes, network)
from the sources. Each arrow's text carries its evidence. Every arrow is
mapped onto the shortest route the standard allows between the two
roles; where it allows none, the arrow is mapped to `id` and onto refuses
the functor, naming it. Standard library only.
"""

import json
import os
import re
import subprocess
import sys
import tomllib
from collections import deque
from pathlib import Path

STANDARD = "ThinAgentDependencies"

# What counts as touching the world, read from source. Reads are not
# effects; a network call is Remote, kept apart from World (see the
# standard's header).
EFFECTS = {
    "World": re.compile(
        r"\b(fs::write|File::create|OpenOptions|create_dir_all|fs::rename|"
        r"remove_file|remove_dir_all|process::Command|Command::new)\b"
    ),
    "Remote": re.compile(r"\b(reqwest::|hyper::|TcpStream|UdpSocket)"),
}


def camel(name):
    return "".join(part.capitalize() for part in re.split(r"[-_]", name))


def ident(name):
    return re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_")


def standard_arrows(path):
    """(name, src, dst) of the standard's dependency category, in order."""
    text = Path(path).read_text()
    body = text[text.index(f"category {STANDARD}") :]
    body = body[: body.index("\n}")]
    return re.findall(r"^\s*(\w+):\s*(\w+)\s*->\s*(\w+)", body, re.M)


def route(arrows, src, dst):
    """Shortest path of the standard from src to dst as onto writes it
    (`g.f`, f first), `id` when src == dst, None when there is none."""
    if src == dst:
        return "id"
    seen, queue = {src}, deque([(src, [])])
    while queue:
        node, path = queue.popleft()
        for name, a, b in arrows:
            if a == node and b not in seen:
                if b == dst:
                    return ".".join(reversed(path + [name]))
                seen.add(b)
                queue.append((b, path + [name]))
    return None


def crate_sources(manifest):
    src = Path(manifest).parent / "src"
    return sorted(src.rglob("*.rs")) if src.exists() else []


def evidence(files, pattern, root):
    """Up to three `file:line` places where pattern occurs, and the count."""
    hits = []
    for f in files:
        for n, line in enumerate(f.read_text().splitlines(), 1):
            if pattern.search(line) and not line.lstrip().startswith("//"):
                hits.append(f"{f.relative_to(root)}:{n}")
    return hits[:3], len(hits)


def items_used(files, dep):
    """Which of dep's items a crate names, e.g. `walk::Answer`."""
    pat = re.compile(rf"\b{ident(dep)}::((?:\w+::)*(?:\{{[^}}]*\}}|\w+))")
    found = set()
    for f in files:
        for m in pat.finditer(f.read_text()):
            prefix, _, group = m.group(1).partition("{")
            names = group.rstrip("}").split(",") if group else [""]
            found.update(prefix + n.strip() for n in names if n.strip() or not group)
    return sorted(found)


def quote(s):
    return '"' + s.replace('"', "'") + '"'


def main():
    roles_path = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).with_name("roles.toml"))
    here = roles_path.parent
    cfg = tomllib.loads(roles_path.read_text())
    root = (here / cfg["workspace"]).resolve()
    roles = cfg["roles"]
    arrows = standard_arrows(here / cfg["standard"])

    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root, check=True, capture_output=True, text=True,
        ).stdout
    )
    crates = {p["name"]: p for p in meta["packages"]}
    unassigned = sorted(set(crates) - set(roles))
    if unassigned:
        sys.exit(f"no role for: {', '.join(unassigned)} (add them to {roles_path})")

    objects = [camel(c) for c in sorted(crates)]
    lines, maps, refused = [], [], []

    def arrow(name, src, dst, text, src_role, dst_role):
        lines.append(f"    {name}: {src} -> {dst} {quote(text)};")
        path = route(arrows, src_role, dst_role)
        if path is None:
            refused.append(f"{name} ({src_role} -> {dst_role})")
            maps.append(f"    {name}: id;  # the standard allows no {src_role} -> {dst_role}")
        else:
            maps.append(f"    {name}: {path};")

    for c in sorted(crates):
        files = crate_sources(crates[c]["manifest_path"])
        deps = sorted({d["name"] for d in crates[c]["dependencies"] if d["name"] in crates and d["kind"] is None})
        for d in deps:
            used = items_used(files, d)
            text = f"{c} depends on {d}" + (f"; names {', '.join(used[:6])}" if used else "")
            arrow(f"{ident(c)}_uses_{ident(d)}", camel(c), camel(d), text, roles[c], roles[d])
        for effect, pattern in EFFECTS.items():
            where, count = evidence(files, pattern, root)
            if count:
                if effect not in objects:
                    objects.append(effect)
                text = f"{c} touches {effect.lower()}: {count} site(s), e.g. {', '.join(where)}"
                arrow(f"{ident(c)}_to_{effect.lower()}", camel(c), effect, text, roles[c], effect)

    role_of = {camel(c): r for c, r in roles.items()} | {e: e for e in EFFECTS}
    out = (here / cfg["out"]).resolve()
    standard_rel = Path(os.path.relpath((here / cfg["standard"]).resolve(), out.parent)).as_posix()
    shown = out.relative_to(root) if out.is_relative_to(root) else out
    out.write_text(
        f"# GENERATED by derive.py from {roles_path.name} and the sources; do not edit.\n"
        f"# `onto functor {shown}`\n\n"
        f'import "{standard_rel}";\n\n'
        f"category {cfg['category']} {{\n"
        f"    objects: {', '.join(objects)};\n\n" + "\n".join(lines) + "\n}\n\n"
        f"functor Conforms: {cfg['category']} -> {STANDARD} {{\n"
        f"    objects: {', '.join(f'{o} -> {role_of[o]}' for o in objects)};\n\n"
        + "\n".join(maps)
        + "\n\n    require: invariants;\n}\n"
    )
    print(f"wrote {shown}: {len(objects)} objects, {len(lines)} arrows")
    for r in refused:
        print(f"  no route in the standard: {r}")


if __name__ == "__main__":
    main()
