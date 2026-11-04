import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import build
import publish


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in build.LICENSES:
            (self.root / name).write_text("Notice for " + name + "\n", encoding="utf-8")
        (self.root / "repository.json").write_text(json.dumps({"name": "Test sources", "feedbackURL": "https://example.test/issues"}))

    def source(self, identifier="ru.fixture", crate="fixture", wasm=b"\0asm\x01\0\0\0\0\x01\0"):
        directory = self.root / "sources" / identifier
        (directory / "res").mkdir(parents=True)
        (directory / "Cargo.toml").write_text(
            '[package]\nname = "' + crate + '"\nversion = "0.1.0"\nedition = "2024"\n'
            '[lib]\ncrate-type = ["cdylib"]\n', encoding="utf-8"
        )
        info = {"id": identifier, "name": "Источник", "version": 3, "languages": ["ru"],
                "contentRating": 1, "url": "https://example.test", "minAppVersion": "0.7.1"}
        (directory / "res/source.json").write_text(json.dumps({"info": info}, ensure_ascii=False), encoding="utf-8")
        # The packer only reads the PNG signature and IHDR dimensions; real
        # source assets are also checked by the official schema verifier.
        (directory / "res/icon.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\0\0\0\rIHDR" + struct.pack(">II", 128, 128))
        target = directory / "target" / build.TARGET / "release"
        target.mkdir(parents=True)
        (target / f"{crate}.wasm").write_bytes(wasm)
        return directory

    def test_exact_named_module_is_packaged_even_when_another_wasm_is_present(self):
        expected = b"\0asm\x01\0\0\0\0\x01\0"
        directory = self.source(wasm=expected)
        target = directory / "target" / build.TARGET / "release"
        (target / "aaa_wrong_source.wasm").write_bytes(b"\0asm\x01\0\0\0wrong")
        output = self.root / "package.aix"
        record = build.package_source(directory, output, self.root)
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(archive.read("Payload/main.wasm"), expected)
            self.assertEqual(len(archive.namelist()), len(set(archive.namelist())))
        self.assertEqual(record["wasm_sha256"], hashlib.sha256(expected).hexdigest())

    def test_all_copyright_and_license_notices_are_embedded_byte_for_byte(self):
        directory = self.source()
        output = self.root / "package.aix"
        build.package_source(directory, output, self.root)
        with zipfile.ZipFile(output) as archive:
            for original, name in build.LICENSES.items():
                self.assertEqual(archive.read("Payload/" + name), (self.root / original).read_bytes())

    def test_identical_inputs_make_byte_identical_packages(self):
        directory = self.source()
        first, second = self.root / "first.aix", self.root / "second.aix"
        build.package_source(directory, first, self.root)
        build.package_source(directory, second, self.root)
        self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_invalid_wasm_is_rejected(self):
        directory = self.source(wasm=b"not wasm")
        with self.assertRaisesRegex(ValueError, "WebAssembly"):
            build.package_source(directory, self.root / "package.aix", self.root)

    def test_reserved_wasm_resource_cannot_replace_the_compiled_module(self):
        directory = self.source()
        (directory / "res/main.wasm").write_bytes(b"wrong")
        with self.assertRaisesRegex(ValueError, "Reserved resource"):
            build.package_source(directory, self.root / "package.aix", self.root)

    def test_source_id_mismatch_and_duplicate_json_keys_fail(self):
        directory = self.source()
        (directory / "res/source.json").write_text('{"info":{},"info":{}}')
        with self.assertRaisesRegex(ValueError, "Duplicate JSON"):
            build.source_metadata(directory)
        (directory / "res/source.json").write_text(json.dumps({"info": {"id": "ru.different"}}))
        with self.assertRaisesRegex(ValueError, "does not match"):
            build.source_metadata(directory)

    def test_distribution_manifest_and_feed_bind_each_package_to_its_content_and_commit(self):
        first = self.source("ru.first", "first")
        second = self.source("ru.second", "second", b"\0asm\x01\0\0\0\0\x02\0\0")
        output = self.root / "dist"
        commit = "c" * 40
        with patch.dict("os.environ", {"GITHUB_SHA": commit}):
            build.package(self.root, output, compile_sources=False, source_commit=commit)
        feed = build.read_json(output / "index.min.json")
        manifest = build.read_json(output / "build-manifest.json")
        self.assertEqual(manifest["sourceCommit"], commit)
        self.assertEqual(manifest["sdkRevision"], build.SDK_REV)
        self.assertEqual([item["id"] for item in feed["sources"]], [first.name, second.name])
        for entry, record in zip(feed["sources"], manifest["packages"], strict=True):
            package = output / entry["downloadURL"]
            self.assertEqual(entry["downloadURL"], record["packageURL"])
            self.assertEqual(package.stat().st_size, record["size"])
            self.assertEqual(hashlib.sha256(package.read_bytes()).hexdigest(), record["sha256"])
            self.assertTrue((output / entry["iconURL"]).is_file())
            self.assertEqual(entry["baseURL"], "https://example.test")
        with patch("publish.ROOT", self.root):
            verified = publish.verify(output, commit)
            self.assertEqual(len(verified), 7)
            with self.assertRaisesRegex(ValueError, "tested source commit"):
                publish.verify(output, "d" * 40)
            changed_package = output / feed["sources"][0]["downloadURL"]
            changed_package.write_bytes(changed_package.read_bytes() + b"tampered")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                publish.verify(output, commit)


class TestDriverTests(unittest.TestCase):
    def test_compile_or_runtime_failure_does_not_skip_the_remaining_crate_suites(self):
        for failure_at in ("compile", "runtime"):
            with self.subTest(failure_at=failure_at), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                crates = [root / "sources/ru.first", root / "sources/ru.second", root / "templates/shared"]
                calls = []

                def run(command, directory, **options):
                    calls.append((directory, command, options))
                    compiling = "--no-run" in command
                    if directory == crates[0] and compiling == (failure_at == "compile"):
                        raise subprocess.CalledProcessError(101, command)
                    return "" if compiling else "test result: ok. 3 passed; 0 failed;\n"

                with patch("build.crate_directories", return_value=crates), patch("build.run", side_effect=run):
                    with self.assertRaisesRegex(ValueError, "sources/ru.first"):
                        build.test(root)
                report = build.read_json(root / "build/offline-tests.json")
                self.assertFalse(report["passed"])
                self.assertEqual(set(report["errors"]), {"sources/ru.first"})
                self.assertEqual(report["tests"], {"sources/ru.second": 3, "templates/shared": 3})
                completed_runtime = [directory for directory, command, _ in calls if "--offline" in command]
                self.assertEqual(completed_runtime[-2:], crates[-2:])
                for _, command, options in calls:
                    if "--offline" in command:
                        self.assertTrue(options["offline_runtime"])

    def test_missing_or_zero_test_summaries_fail_with_a_saved_report(self):
        for output in ("runner exited without test results", "test result: ok. 0 passed; 0 failed;\n"):
            with self.subTest(output=output), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                directory = root / "sources/ru.fixture"
                with patch("build.crate_directories", return_value=[directory]), patch("build.run", return_value=output):
                    with self.assertRaises(ValueError):
                        build.test(root)
                report = build.read_json(root / "build/offline-tests.json")
                self.assertFalse(report["passed"])
                self.assertTrue(report["errors"])


if __name__ == "__main__":
    unittest.main()
