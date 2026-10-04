"""Synthetic library/source packaging and ELF coexistence checks; no media is opened."""

import importlib.util
import ctypes
import json
import os
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch

import bundle

REPO = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ffmpeg_builder", REPO / "scripts/build-ffmpeg.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class BundleTest(unittest.TestCase):
    def test_msvc_arm64_does_not_require_uninstalled_assembler_preprocessor(self):
        self.assertIn("--arch=aarch64", builder.toolchain_options("Windows", True))
        self.assertIn("--disable-asm", builder.toolchain_options("Windows", True))
        self.assertNotIn("--disable-asm", builder.toolchain_options("Windows", False))
        self.assertEqual(builder.toolchain_options("Linux", True), [])
        self.assertEqual(builder.toolchain_options("Darwin", True), [])

    @unittest.skipUnless(os.name == "posix" and os.environ.get("FFMPEG_DIR") and bundle.platform.system() == "Linux",
                         "Requires the native Linux FFmpeg build; no media is opened")
    def test_host_decoder_plugin_does_not_bind_to_encoder_only_ffmpeg(self):
        prefix = Path(os.environ["FFMPEG_DIR"])
        codec = ctypes.CDLL(str(prefix / "lib/libavcodec-serein.so.61"), mode=ctypes.RTLD_GLOBAL)
        codec.avcodec_find_encoder_by_name.argtypes = [ctypes.c_char_p]
        codec.avcodec_find_encoder_by_name.restype = ctypes.c_void_p
        codec.avcodec_find_decoder_by_name.argtypes = [ctypes.c_char_p]
        codec.avcodec_find_decoder_by_name.restype = ctypes.c_void_p
        self.assertTrue(codec.avcodec_find_encoder_by_name(b"libopenh264"))
        self.assertFalse(codec.avcodec_find_decoder_by_name(b"h264"))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "host.c").write_text('void *avcodec_find_decoder_by_name(const char *name) { static int codec; return &codec; }\n')
            (root / "host.v").write_text("LIBAVCODEC_61 { global: avcodec_*; local: *; };\n")
            (root / "plugin.c").write_text('void *avcodec_find_decoder_by_name(const char *);\n'
                                           'int incoming_decoder_present(void) { return avcodec_find_decoder_by_name("h264") != 0; }\n')
            subprocess.run(["cc", "-shared", "-fPIC", "-Wl,-soname,libavcodec.so.61",
                            "-Wl,--version-script=" + str(root / "host.v"), "-o", str(root / "libavcodec.so.61"),
                            str(root / "host.c")], check=True)
            subprocess.run(["cc", "-shared", "-fPIC", "-Wl,-rpath,$ORIGIN", "-o", str(root / "incoming.so"),
                            str(root / "plugin.c"), "-L" + str(root), "-l:libavcodec.so.61"], check=True)
            plugin = ctypes.CDLL(str(root / "incoming.so"))
            plugin.incoming_decoder_present.restype = ctypes.c_int
            self.assertEqual(plugin.incoming_decoder_present(), 1,
                             "Host decoder plugin must use host FFmpeg, even when Serein's encoders are loaded globally")

    def test_relocatable_shared_libraries_and_exact_source_allowlist(self):
        for system in bundle.LIBRARIES:
            with self.subTest(system=system), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                prefix = root / "prefix"
                libs = prefix / ("bin" if system == "Windows" else "lib")
                libs.mkdir(parents=True)
                for name in bundle.LIBRARIES[system]:
                    (libs / name).write_bytes(name.encode())
                (libs / "unrelated-private-log").write_text("must not ship")
                notices = prefix / "share/serein-ffmpeg"
                (notices / "source").mkdir(parents=True)
                nvenc = system != "Darwin"
                (notices / "build.json").write_text(json.dumps({"system": system, "nvenc": nvenc,
                    "sources": {"ffmpeg": builder.SOURCES["ffmpeg"]}}))
                for name in ("configure.json", "build-ffmpeg.py", "serein-ffmpeg.patch", "COPYING.LGPLv2.1", "OpenH264-LICENSE",
                             "source/ffmpeg-7.1.5.tar.xz", "nv-codec-headers-README", "source/nv-codec-headers-12.2.72.0.tar.gz"):
                    (notices / name).write_text("synthetic source/notice")
                (notices / "private.log").write_text("must not ship")
                stage = root / "dist"
                with patch.object(bundle.platform, "system", return_value=system):
                    bundle.bundle(stage, prefix)
                names = {p.name for p in stage.rglob("*") if p.is_file()}
                self.assertTrue(set(bundle.LIBRARIES[system]).issubset(names))
                self.assertIn("ffmpeg-7.1.5.tar.xz", names)
                self.assertNotIn("private.log", names)
                self.assertNotIn("unrelated-private-log", names)
                self.assertEqual("nv-codec-headers-12.2.72.0.tar.gz" in names, nvenc)

    def test_source_checksum_mismatch_fails_without_using_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            source = builder.SOURCES["ffmpeg"]
            (cache / source["file"]).write_bytes(b"corrupt source")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                builder.fetch(source, cache, offline=True)

    def test_offline_missing_source_never_attempts_download(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(builder.urllib.request, "urlopen") as download:
            with self.assertRaisesRegex(ValueError, "Offline source missing"):
                builder.fetch(builder.SOURCES["ffmpeg"], Path(directory), offline=True)
            download.assert_not_called()

    def test_bundle_requires_build_prefix(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(bundle.platform, "system", return_value="Linux"):
            with self.assertRaisesRegex(ValueError, "FFMPEG_DIR"):
                bundle.bundle(Path("unused"))


if __name__ == "__main__":
    unittest.main()
