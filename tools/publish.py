"""Verify the tested artifact and publish a feed with a normal Git fast-forward."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile

from build import LICENSES, ROOT, SDK_REV, read_json, source_directories, source_metadata


def verify(directory: Path, source_commit: str) -> list[str]:
    if not re.fullmatch(r"[0-9a-f]{40}", source_commit):
        raise ValueError("Invalid source commit")
    manifest = read_json(directory / "build-manifest.json")
    if manifest["sourceCommit"] != source_commit or manifest["sdkRevision"] != SDK_REV:
        raise ValueError("Artifact does not belong to the tested source commit and SDK")
    feed = read_json(directory / "index.min.json")
    if feed != read_json(directory / "index.json"):
        raise ValueError("The two feed formats disagree")
    repository = read_json(ROOT / "repository.json")
    if {k: v for k, v in feed.items() if k != "sources"} != repository:
        raise ValueError("Unexpected repository metadata")
    sources = source_directories(ROOT)
    if len(feed["sources"]) != len(sources) or len(manifest["packages"]) != len(sources):
        raise ValueError("The artifact is missing a source or contains an extra source")
    expected = {"index.json", "index.min.json", "build-manifest.json"}
    for source, entry, record in zip(sources, feed["sources"], manifest["packages"], strict=True):
        info, crate = source_metadata(source)
        package_url = f"sources/{info['id']}-v{info['version']}.aix"
        icon_url = f"icons/{info['id']}-v{info['version']}.png"
        wanted_entry = {k: info[k] for k in ("id", "name", "version", "languages")}
        wanted_entry.update(downloadURL=package_url, iconURL=icon_url,
                            contentRating=info.get("contentRating", 0))
        for key in ("altNames", "minAppVersion", "maxAppVersion"):
            if key in info:
                wanted_entry[key] = info[key]
        base_url = info.get("url") or next(iter(info.get("urls", [])), None)
        if base_url:
            wanted_entry["baseURL"] = base_url
        if entry != wanted_entry:
            raise ValueError(f"Feed metadata differs from the tested source: {info['id']}")
        for key, value in {"id": info["id"], "version": info["version"], "crate": crate,
                           "packageURL": package_url, "iconURL": icon_url}.items():
            if record[key] != value:
                raise ValueError(f"Manifest mismatch: {info['id']} {key}")
        package = directory / package_url
        data = package.read_bytes()
        if len(data) != record["size"] or hashlib.sha256(data).hexdigest() != record["sha256"]:
            raise ValueError(f"Package checksum mismatch: {package_url}")
        resources = {p.relative_to(source / "res").as_posix(): p.read_bytes()
                     for p in (source / "res").rglob("*") if p.is_file()}
        resources.update({name: (ROOT / original).read_bytes() for original, name in LICENSES.items()})
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            wanted = {"Payload/main.wasm", *("Payload/" + p for p in resources)}
            if len(names) != len(wanted) or set(names) != wanted or archive.testzip() is not None:
                raise ValueError(f"Invalid package contents: {package_url}")
            wasm = archive.read("Payload/main.wasm")
            if not wasm.startswith(b"\0asm\x01\0\0\0") or hashlib.sha256(wasm).hexdigest() != record["wasm_sha256"]:
                raise ValueError(f"Invalid package module: {package_url}")
            if any(archive.read("Payload/" + path) != content for path, content in resources.items()):
                raise ValueError(f"Package resources differ from the tested source: {package_url}")
        if (directory / icon_url).read_bytes() != resources["icon.png"]:
            raise ValueError(f"Feed icon differs from its package: {icon_url}")
        expected.update((package_url, icon_url))
    actual = set()
    for path in directory.rglob("*"):
        if path.is_symlink():
            raise ValueError("Symlinks are not allowed in the publication artifact")
        if path.is_file():
            actual.add(path.relative_to(directory).as_posix())
    if actual != expected:
        raise ValueError(f"Unexpected artifact files: {actual.symmetric_difference(expected)}")
    return sorted(expected)


def git(*arguments: str, environment: dict | None = None, work_tree: Path | None = None,
        input_text: str | None = None) -> str:
    command = ["git", "-c", f"safe.directory={ROOT}"]
    if work_tree:
        command += [f"--git-dir={ROOT / '.git'}", f"--work-tree={work_tree}"]
    result = subprocess.run(command + list(arguments), cwd=work_tree or ROOT,
                            env=environment, input=input_text, text=True, encoding="utf-8",
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    return result.stdout.strip()


def publish(directory: Path, source_commit: str, run_url: str) -> None:
    files = verify(directory, source_commit)
    if git("rev-parse", "HEAD") != source_commit:
        raise ValueError("Publisher checkout differs from the tested commit")
    latest = git("ls-remote", "origin", "refs/heads/main").split()[0]
    if latest != source_commit:
        print("A newer source commit exists; skipping this superseded build.")
        return
    existing = git("ls-remote", "origin", "refs/heads/repo")
    parent = None
    if existing:
        git("fetch", "--no-tags", "origin", "refs/heads/repo")
        parent = git("rev-parse", "FETCH_HEAD")
        for path in files:
            if not path.startswith(("sources/", "icons/")):
                continue
            previous = git("ls-tree", parent, "--", path)
            if previous:
                old_blob = previous.split()[2]
                new_blob = git("hash-object", "--no-filters", str(directory / path))
                if old_blob != new_blob:
                    raise ValueError(f"Published version is immutable; bump info.version before changing {path}")
    # A separate Git index leaves the source checkout untouched. Loading the
    # existing tree preserves older versioned packages for cached source lists.
    with tempfile.TemporaryDirectory(prefix="aidoku-publish-") as temporary:
        environment = os.environ.copy()
        environment.update(GIT_INDEX_FILE=str(Path(temporary) / "index"),
                           GIT_AUTHOR_NAME="github-actions[bot]",
                           GIT_AUTHOR_EMAIL="41898282+github-actions[bot]@users.noreply.github.com",
                           GIT_COMMITTER_NAME="github-actions[bot]",
                           GIT_COMMITTER_EMAIL="41898282+github-actions[bot]@users.noreply.github.com")
        git("read-tree", parent or "--empty", environment=environment)
        git("-c", "core.autocrlf=false", "add", "--", *files,
            environment=environment, work_tree=directory)
        tree = git("write-tree", environment=environment)
        if parent and tree == git("rev-parse", parent + "^{tree}"):
            print("The tested source packages are already published.")
            return
        arguments = ["commit-tree", tree]
        if parent:
            arguments += ["-p", parent]
        commit = git(*arguments, environment=environment,
                     input_text=f"Publish sources from {source_commit}\n\nVerified build: {run_url}\n")
        git("push", "origin", commit + ":refs/heads/repo")
        print(f"Published verified source feed at repo commit {commit}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--run-url", default="local artifact verification")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    directory = args.directory.resolve()
    if args.verify_only:
        files = verify(directory, args.source_commit)
        print(f"Verified {len(files)} publication files from {args.source_commit}")
    else:
        publish(directory, args.source_commit, args.run_url)


if __name__ == "__main__":
    main()
