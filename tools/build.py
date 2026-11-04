"""Build pinned Aidoku source crates and package their exact WASM outputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SDK_REV = "ae6485e3c14ecd1fd5497d170eae884c68d04a87"
TARGET = "wasm32-unknown-unknown"
LICENSES = {
    "LICENSE": "LICENSE.txt",
    "LICENSE-MIT": "LICENSE-MIT.txt",
    "LICENSE-AIDOKU-SDK": "LICENSE-AIDOKU-SDK.txt",
    "NOTICE": "NOTICE.txt",
}


def read_json(path: Path):
    def unique_keys(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"Duplicate JSON key in {path}: {key}")
            result[key] = value
        return result
    return json.loads(path.read_text(encoding="utf-8-sig"), object_pairs_hook=unique_keys)


def source_metadata(directory: Path) -> tuple[dict, str]:
    manifest = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))
    source = read_json(directory / "res/source.json")
    info = source["info"]
    if not re.fullmatch(r"[a-z][a-z0-9]*(?:[.-][a-z0-9]+)+", info["id"]):
        raise ValueError(f"Invalid source ID: {info['id']}")
    if info["id"] != directory.name:
        raise ValueError(f"Source ID does not match directory: {directory}")
    if type(info["version"]) is not int or info["version"] < 1:
        raise ValueError("Source version must be a positive integer")
    if not info["name"] or not info["languages"] or not all(isinstance(x, str) and x for x in info["languages"]):
        raise ValueError("Source name and languages are required")
    if info.get("contentRating", 0) not in (0, 1, 2):
        raise ValueError("Invalid content rating")
    if "cdylib" not in manifest.get("lib", {}).get("crate-type", []):
        raise ValueError(f"Source must build a cdylib: {directory}")
    crate = manifest.get("lib", {}).get("name", manifest["package"]["name"].replace("-", "_"))
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", crate):
        raise ValueError(f"Invalid library target: {crate}")
    return info, crate


def validate_sdk(manifest_path: Path) -> None:
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    if "aidoku" not in manifest.get("dependencies", {}):
        raise ValueError(f"{manifest_path}: missing Aidoku SDK dependency")
    for section in ("dependencies", "dev-dependencies"):
        for name, dependency in manifest.get(section, {}).items():
            if name not in ("aidoku", "aidoku-test"):
                continue
            if not isinstance(dependency, dict) or dependency.get("rev") != SDK_REV:
                raise ValueError(f"{manifest_path}: {name} must pin SDK revision {SDK_REV}")
            if dependency.get("git", "").removesuffix(".git") != "https://github.com/Aidoku/aidoku-rs":
                raise ValueError(f"{manifest_path}: unexpected SDK repository")
    lock_path = manifest_path.with_name("Cargo.lock")
    lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    for package in lock["package"]:
        if package["name"] in ("aidoku", "aidoku-test") and not package.get("source", "").endswith("#" + SDK_REV):
            raise ValueError(f"{lock_path}: unexpected SDK revision")


def source_directories(root: Path) -> list[Path]:
    sources = sorted(path.parent for path in (root / "sources").glob("*/Cargo.toml"))
    if not sources:
        raise ValueError("No source crates found")
    identifiers = [source_metadata(path)[0]["id"] for path in sources]
    if len(identifiers) != len(set(identifiers)):
        raise ValueError("Duplicate source IDs")
    return sources


def crate_directories(root: Path) -> list[Path]:
    return sorted(path.parent for folder in ("sources", "templates") for path in (root / folder).glob("*/Cargo.toml"))


def run(command: list[str], directory: Path, *, offline_runtime: bool = False) -> str:
    print("+", " ".join(command), f"(in {directory.relative_to(ROOT)})", flush=True)
    environment = os.environ.copy()
    environment.update(CARGO_BUILD_JOBS="2", RAYON_NUM_THREADS="2")
    if offline_runtime:
        # Cargo is --offline here; the pinned test runner uses reqwest's system
        # proxy support. A stray network call in a fixture fails locally instead
        # of depending on a public site or sending a request to it.
        environment.update(HTTP_PROXY="http://127.0.0.1:9", HTTPS_PROXY="http://127.0.0.1:9",
                           ALL_PROXY="http://127.0.0.1:9", NO_PROXY="",
                           http_proxy="http://127.0.0.1:9", https_proxy="http://127.0.0.1:9",
                           all_proxy="http://127.0.0.1:9", no_proxy="")
    result = subprocess.run(command, cwd=directory, env=environment, text=True, encoding="utf-8", errors="replace",
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=1200)
    print(result.stdout, end="", flush=True)
    logs = ROOT / "build/logs"
    logs.mkdir(parents=True, exist_ok=True)
    name = directory.name + "-" + command[1] + ".log"
    (logs / name).write_text(result.stdout, encoding="utf-8")
    if result.returncode:
        raise subprocess.CalledProcessError(result.returncode, command)
    return result.stdout


def validate(root: Path) -> None:
    source_directories(root)
    for directory in crate_directories(root):
        validate_sdk(directory / "Cargo.toml")
    for name in LICENSES:
        if not (root / name).is_file() or not (root / name).read_bytes().strip():
            raise ValueError(f"Missing required notice: {name}")


def lint(root: Path) -> None:
    for directory in crate_directories(root):
        run(["cargo", "fmt", "--check"], directory)
        run(["cargo", "clippy", "--locked", "--target", TARGET, "--", "-D", "warnings"], directory)


def test(root: Path, *, live: bool = False) -> None:
    counts = {}
    failures = {}
    for directory in crate_directories(root):
        # Fetch/compile with normal Cargo networking before switching the runner
        # to fixture-only networking. The same test binaries are then reused.
        command = ["cargo", "test", "--locked", "--target", TARGET]
        if live:
            manifest = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))
            if "live-tests" not in manifest.get("features", {}):
                continue
            command.extend(["--features", "live-tests"])
        relative = directory.relative_to(root).as_posix()
        try:
            run(command + ["--no-run"], directory)
            output = run(command + ["--offline", "--", "--nocapture"], directory, offline_runtime=not live)
            summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed;", output)
            if not summaries or any(int(failed) for _, failed in summaries):
                raise ValueError("No successful test-runner summary")
            counts[relative] = sum(int(passed) for passed, _ in summaries)
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, ValueError) as error:
            failures[relative] = str(error)
            print(f"FAILED {relative}: {error}", flush=True)
    if not failures and (not counts or sum(counts.values()) == 0):
        failures["test-runner"] = "No tests executed"
    report = root / ("build/live-tests.json" if live else "build/offline-tests.json")
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps({"passed": not failures, "live": live, "tests": counts, "errors": failures}, indent=2) + "\n",
                      encoding="utf-8")
    if failures:
        raise ValueError("Test suites failed: " + "; ".join(f"{name}: {error}" for name, error in failures.items()))


def package_source(directory: Path, destination: Path, root: Path) -> dict:
    info, crate = source_metadata(directory)
    wasm = directory / "target" / TARGET / "release" / f"{crate}.wasm"
    executable = wasm.read_bytes()
    if not executable.startswith(b"\0asm\x01\0\0\0"):
        raise ValueError(f"Not a WebAssembly 1 module: {wasm}")
    files = {"main.wasm": executable}
    for path in sorted((directory / "res").rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Symlink resource: {path}")
        if path.is_file():
            relative = path.relative_to(directory / "res").as_posix()
            if relative in files:
                raise ValueError(f"Reserved resource path: {relative}")
            files[relative] = path.read_bytes()
    for source, name in LICENSES.items():
        if name in files:
            raise ValueError(f"Resource overrides a required license: {name}")
        files[name] = (root / source).read_bytes()
    icon = files["icon.png"]
    if not icon.startswith(b"\x89PNG\r\n\x1a\n") or len(icon) < 24:
        raise ValueError("A PNG source icon is required")
    width, height = struct.unpack_from(">II", icon, 16)
    if width != 128 or height != 128:
        raise ValueError("The source icon must be 128x128, as required by the official verifier")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, content in sorted(files.items()):
            entry = zipfile.ZipInfo("Payload/" + name, (1980, 1, 1, 0, 0, 0))
            entry.create_system = 3
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, content)
    with zipfile.ZipFile(destination) as archive:
        if archive.testzip() is not None or archive.read("Payload/main.wasm") != executable:
            raise ValueError("Packaged module does not match its exact Cargo target")
    return {"id": info["id"], "version": info["version"], "size": destination.stat().st_size,
            "sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
            "wasm_sha256": hashlib.sha256(executable).hexdigest(), "crate": crate}


def package(root: Path, output: Path, *, compile_sources: bool = True, source_commit: str | None = None) -> None:
    metadata = read_json(root / "repository.json")
    if not metadata.get("name"):
        raise ValueError("Repository name is required")
    feed = {"name": metadata["name"], "sources": []}
    if metadata.get("feedbackURL"):
        feed["feedbackURL"] = metadata["feedbackURL"]
    records = []
    for directory in source_directories(root):
        if compile_sources:
            run(["cargo", "build", "--locked", "--release", "--target", TARGET], directory)
        info, _ = source_metadata(directory)
        filename = f"{info['id']}-v{info['version']}.aix"
        destination = output / "sources" / filename
        record = package_source(directory, destination, root)
        icon_name = f"{info['id']}-v{info['version']}.png"
        record.update(packageURL="sources/" + filename, iconURL="icons/" + icon_name)
        records.append(record)
        (output / "icons").mkdir(parents=True, exist_ok=True)
        shutil.copyfile(directory / "res/icon.png", output / "icons" / icon_name)
        entry = {key: info[key] for key in ("id", "name", "version", "languages")}
        entry.update(downloadURL="sources/" + filename, iconURL="icons/" + icon_name,
                     contentRating=info.get("contentRating", 0))
        for key in ("altNames", "minAppVersion", "maxAppVersion"):
            if key in info:
                entry[key] = info[key]
        base_url = info.get("url") or next(iter(info.get("urls", [])), None)
        if base_url:
            entry["baseURL"] = base_url
        feed["sources"].append(entry)
    output.mkdir(parents=True, exist_ok=True)
    (output / "index.json").write_text(json.dumps(feed, ensure_ascii=False, indent="\t") + "\n", encoding="utf-8")
    (output / "index.min.json").write_text(json.dumps(feed, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    if source_commit is None:
        source_commit = subprocess.check_output(
            ["git", "-c", f"safe.directory={root}", "rev-parse", "HEAD"], cwd=root, text=True
        ).strip()
    if not re.fullmatch(r"[0-9a-f]{40}", source_commit):
        raise ValueError("Invalid source commit")
    if os.environ.get("GITHUB_SHA") and os.environ["GITHUB_SHA"] != source_commit:
        raise ValueError("Checkout does not match the workflow commit")
    (output / "build-manifest.json").write_text(
        json.dumps({"sourceCommit": source_commit, "sdkRevision": SDK_REV, "packages": records}, indent=2) + "\n", encoding="utf-8"
    )
    print(f"Packaged {len(records)} sources into {output}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("validate", "lint", "test", "test-live", "package", "all"))
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    validate(ROOT)
    if args.command in ("lint", "all"):
        lint(ROOT)
    if args.command in ("test", "test-live", "all"):
        test(ROOT, live=args.command == "test-live")
    if args.command in ("package", "all"):
        package(ROOT, args.output.resolve())


if __name__ == "__main__":
    main()
