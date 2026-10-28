#!/usr/bin/env python3
"""Check benchmark version lines without installing packages or using the network."""
import json
from pathlib import Path
import re
import sys
import tomllib

HERE = Path(__file__).resolve().parent
EXACT = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")
MANIFESTS = ("package.json", "requirements.txt", "Cargo.toml")


def dependencies(path):
    """Return direct pins in a common form; reject unpinned/unsupported inputs."""
    pins = {}

    def add(name, version):
        if name in pins and pins[name] != version:
            raise ValueError(f"{name}: conflicting direct pins {pins[name]!r} and {version!r}")
        pins[name] = version

    if path.name == "package.json":
        data = json.loads(path.read_text())
        for section in ("dependencies", "devDependencies", "optionalDependencies", "peerDependencies"):
            for name, version in data.get(section, {}).items():
                add(name, version)
        return pins
    if path.name == "requirements.txt":
        for line in path.read_text().splitlines():
            line = line.split("#", 1)[0].strip()
            if not line:
                continue
            match = re.fullmatch(r"([A-Za-z0-9_.-]+)==(.+)", line)
            if not match:
                raise ValueError(f"expected package==version, got {line!r}")
            add(match[1].lower().replace("_", "-"), match[2])
        return pins
    data = tomllib.loads(path.read_text())
    for table in [data, *data.get("target", {}).values()]:
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            for name, spec in table.get(section, {}).items():
                version = spec.get("version", "") if isinstance(spec, dict) else spec
                if not isinstance(version, str):
                    raise ValueError(f"{name}: expected an exact Cargo version, got {version!r}")
                add(name, version[1:] if version.startswith("=") else f"unpinned: {version}")
    return pins


def check(bench=HERE, projects=None):
    """Return all diagnostics, including project and package, rather than stop early."""
    bench = Path(bench)
    lines = json.loads((bench / "fixture-lines.json").read_text())
    questions = json.loads((bench / "questions.json").read_text())["questions"]
    project_dir = Path(projects) if projects is not None else bench / "projects"
    projects = {p.name for p in project_dir.iterdir() if p.is_dir()}
    errors = []
    for label, actual in [("project directories", projects),
                          ("question projects", {q["project"] for q in questions})]:
        for name in sorted(set(lines) ^ actual):
            errors.append(f"{name}: {label} and fixture-lines.json disagree")
    for project, expected in lines.items():
        paths = [project_dir / project / name for name in MANIFESTS
                 if (project_dir / project / name).is_file()]
        if len(paths) != 1:
            errors.append(f"{project}: expected one fixture manifest, found {len(paths)}")
            continue
        try:
            pins = dependencies(paths[0])
        except (ValueError, TypeError) as exc:
            errors.append(f"{project}/{paths[0].name}: {exc}")
            continue
        for package in sorted(set(pins) | set(expected)):
            version = pins.get(package)
            line = expected.get(package)
            if line is None:
                errors.append(f"{project}/{package}: dependency has no intended version line")
            elif version is None:
                errors.append(f"{project}/{package}: missing pin for intended line {line}")
            elif not isinstance(version, str) or not EXACT.fullmatch(version):
                errors.append(f"{project}/{package}: expected exact {line}.x pin, got {version!r}")
            elif not version.startswith(line + "."):
                errors.append(f"{project}/{package}: expected line {line}, got {version}")
        for q in questions:
            if q["project"] == project and q["package"] not in pins:
                errors.append(f"{project}/{q['package']}: question {q['id']} has no direct dependency")
        for left, right in [("react", "react-dom"), ("prisma", "@prisma/client"),
                            ("react-router", "react-router-dom")]:
            if left in pins and right in pins and pins[left] != pins[right]:
                errors.append(f"{project}/{left} + {right}: companion pins must match exactly, "
                              f"got {pins[left]} and {pins[right]}")
    return errors


def main():
    errors = check(projects=Path(sys.argv[1]) if len(sys.argv) > 1 else None)
    if errors:
        print("Benchmark fixture integrity failed:\n" + "\n".join(errors), file=sys.stderr)
        return 1
    print("Benchmark fixture lines and companion pins agree with the complete question census.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
