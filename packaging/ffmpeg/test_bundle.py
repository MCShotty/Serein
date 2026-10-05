"""Synthetic library/source packaging and ELF coexistence checks; no media is opened."""

import importlib.util
import ctypes
import json
import hashlib
import io
import os
import re
from pathlib import Path
import tempfile
import subprocess
import tarfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import bundle

REPO = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ffmpeg_builder", REPO / "scripts/build-ffmpeg.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


def native_codec(prefix, system, mode=ctypes.DEFAULT_MODE):
    directory = prefix / ("bin" if system == "Windows" else "lib")
    # Loading the absolute dependencies also resolves @rpath IDs when macOS
    # Python itself has no LC_RPATH for a developer's private native prefix.
    dependencies = [ctypes.CDLL(str(directory / name), mode=mode) for name in reversed(bundle.LIBRARIES[system][1:])]
    codec = ctypes.CDLL(str(directory / bundle.LIBRARIES[system][0]), mode=mode)
    codec._serein_dependencies = dependencies
    return codec


class BundleTest(unittest.TestCase):
    def test_matching_stamp_rejects_incomplete_prefix_without_native_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prefix = root / "prefix"
            notices = prefix / "share/serein-ffmpeg"
            notices.mkdir(parents=True)
            backends = builder.encoder_backends("Linux", False)
            recipe = {"sources": builder.SOURCES, "system": "Linux", "architecture": "x86_64", **backends,
                      "encoders": sorted(builder.encoder_names(backends)),
                      "recipe_sha256": hashlib.sha256(Path(builder.__file__).read_bytes()).hexdigest()}
            (notices / "build.json").write_text(json.dumps(recipe))
            args = SimpleNamespace(prefix=prefix, cache_dir=root / "cache", work_dir=root / "work", jobs=1, offline=True)
            with patch.object(builder.platform, "system", return_value="Linux"), \
                    patch.dict(os.environ, {"VSCMD_ARG_TGT_ARCH": "x86_64"}), \
                    patch.object(builder, "fetch") as fetch, patch.object(builder, "run") as run:
                with self.assertRaisesRegex(ValueError, "Incomplete FFmpeg prefix"):
                    builder.build(args)
                fetch.assert_not_called()
                run.assert_not_called()

    def test_cached_build_requires_platform_artifacts_and_source_payload(self):
        headers = ("libavcodec/avcodec.h", "libavutil/avutil.h", "libavutil/error.h", "libavutil/frame.h",
                   "libavutil/hwcontext.h", "libavutil/mem.h", "libavutil/opt.h")
        provenance = ("build-ffmpeg.py", "configure.json", "serein-ffmpeg.patch", "COPYING.LGPLv2.1", "OpenH264-LICENSE",
                      "source/ffmpeg-7.1.5.tar.xz", "source/openh264-2.6.0-source.tar.bz2",
                      "nv-codec-headers-README", "source/nv-codec-headers-12.2.72.0.tar.gz", "AMF-LICENSE",
                      "source/AMF-1.4.36-headers.tar", "oneVPL-LICENSE", "oneVPL-third-party-programs.txt",
                      "source/libvpl-2.14.0.tar.gz")
        for system in bundle.LIBRARIES:
            with self.subTest(system=system), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                prefix = root / "prefix"
                relative_files = ["include/" + name for name in headers]
                relative_files += ["lib/pkgconfig/libavcodec-serein.pc", "lib/pkgconfig/libavutil-serein.pc"]
                libraries = [str(Path("bin" if system == "Windows" else "lib") / name)
                             for name in bundle.LIBRARIES[system]]
                relative_files += libraries
                aliases = {"Linux": ["lib/libavcodec-serein.so", "lib/libavutil-serein.so"],
                           "Darwin": ["lib/libavcodec-serein.dylib", "lib/libavutil-serein.dylib"],
                           "Windows": ["lib/avcodec-serein.lib", "lib/avutil-serein.lib"]}[system]
                relative_files += aliases + ["share/serein-ffmpeg/" + name for name in provenance]
                for name in relative_files:
                    target = prefix / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(b"synthetic required artifact")
                backends = builder.encoder_backends(system, False)
                recipe = {"sources": builder.SOURCES, "system": system, "architecture": "x86_64", **backends,
                          "encoders": sorted(builder.encoder_names(backends)),
                          "recipe_sha256": hashlib.sha256(Path(builder.__file__).read_bytes()).hexdigest()}
                (prefix / "share/serein-ffmpeg/build.json").write_text(json.dumps(recipe))
                args = SimpleNamespace(prefix=prefix, cache_dir=root / "cache", work_dir=root / "work", jobs=1, offline=True)
                with patch.object(builder.platform, "system", return_value=system), \
                        patch.dict(os.environ, {"VSCMD_ARG_TGT_ARCH": "x86_64"}), \
                        patch.object(builder, "fetch") as fetch, patch.object(builder, "run") as run:
                    self.assertEqual(builder.build(args), prefix)
                    for name in ("include/libavcodec/avcodec.h", libraries[0], aliases[0],
                                 "share/serein-ffmpeg/source/ffmpeg-7.1.5.tar.xz"):
                        target = prefix / name
                        original = target.read_bytes()
                        target.unlink()
                        with self.assertRaisesRegex(ValueError, "Incomplete FFmpeg prefix"):
                            builder.build(args)
                        target.write_bytes(b"")
                        with self.assertRaisesRegex(ValueError, "Incomplete FFmpeg prefix"):
                            builder.build(args)
                        target.write_bytes(original)
                    if system != "Windows":
                        alias = prefix / aliases[0]
                        alias.unlink()
                        alias.symlink_to(Path(libraries[0]).name)
                        self.assertEqual(builder.build(args), prefix)
                        (prefix / libraries[0]).unlink()
                        with self.assertRaisesRegex(ValueError, "Incomplete FFmpeg prefix"):
                            builder.build(args)
                    fetch.assert_not_called()
                    run.assert_not_called()

    def test_openh264_subset_preserves_build_scripts_and_android_resources(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw = root / "raw.tar.gz"
            with tarfile.open(raw, "w:gz") as archive:
                for name, mode, content in (("codec/common/generate_version.sh", 0o775, b"#!/bin/sh\n"),
                                            ("LICENSE", 0o664, b"BSD license"),
                                            ("codec/build/android/dec/res/layout/main.xml", 0o664, b"Android resource"),
                                            ("res/test.yuv", 0o664, b"test media omitted")):
                    entry = tarfile.TarInfo("openh264-2.6.0/" + name)
                    entry.size, entry.mode, entry.mtime, entry.uid = len(content), mode, 123456, 1000
                    archive.addfile(entry, io.BytesIO(content))
            first, second = root / "first.tar.bz2", root / "second.tar.bz2"
            builder.write_openh264_subset(raw, first)
            builder.write_openh264_subset(raw, second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with tarfile.open(first) as archive:
                self.assertNotIn("openh264-2.6.0/res/test.yuv", archive.getnames())
                retained = "openh264-2.6.0/codec/build/android/dec/res/layout/main.xml"
                self.assertEqual(archive.extractfile(retained).read(), b"Android resource")
                script = archive.getmember("openh264-2.6.0/codec/common/generate_version.sh")
                self.assertEqual(script.mode, 0o775)
                self.assertEqual((script.mtime, script.uid, script.gid), (0, 0, 0))

    def test_hardware_build_options_keep_vaapi_encoder_disabled(self):
        for system in ("Linux", "Windows", "Darwin"):
            for arm64 in (False, True):
                with self.subTest(system=system, arm64=arm64):
                    backends = builder.encoder_backends(system, arm64)
                    options = builder.hardware_options(system, backends)
                    self.assertIn("--disable-encoder=h264_vaapi,hevc_vaapi,av1_vaapi", options)
                    self.assertIn("--disable-mediafoundation", options)
                    enabled = {name for option in options if option.startswith("--enable-encoder=")
                               for name in option.split("=", 1)[1].split(",")}
                    for codec in ("h264", "hevc", "av1"):
                        self.assertEqual(f"{codec}_amf" in enabled, system == "Linux" or (system == "Windows" and not arm64))
                        self.assertEqual(f"{codec}_qsv" in enabled, system in {"Linux", "Windows"} and not arm64)
                        self.assertEqual(f"{codec}_nvenc" in enabled, system in {"Linux", "Windows"} and not (system == "Windows" and arm64))
                    self.assertEqual("hevc_videotoolbox" in enabled, system == "Darwin")
                    self.assertNotIn("av1_videotoolbox", enabled)
                    self.assertEqual("--enable-vaapi" in options, system == "Linux" and not arm64)
                    self.assertEqual("--enable-d3d11va" in options, system == "Windows" and not arm64)
                    self.assertNotIn("--enable-libmfx", options)

    @unittest.skipUnless(os.environ.get("FFMPEG_DIR") and bundle.platform.system() in bundle.LIBRARIES,
                         "Requires a native FFmpeg build; no media is opened")
    def test_native_codec_registry_is_exact_and_contains_no_decoders(self):
        prefix = Path(os.environ["FFMPEG_DIR"])
        system = bundle.platform.system()
        codec = native_codec(prefix, system)
        codec.av_codec_iterate.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
        codec.av_codec_iterate.restype = ctypes.c_void_p
        codec.av_codec_is_encoder.argtypes = [ctypes.c_void_p]
        codec.av_codec_is_encoder.restype = ctypes.c_int
        state = ctypes.c_void_p()
        registered = set()
        while value := codec.av_codec_iterate(ctypes.byref(state)):
            self.assertTrue(codec.av_codec_is_encoder(value))
            # AVCodec's first member is const char *name in pinned FFmpeg 7.
            registered.add(ctypes.cast(value, ctypes.POINTER(ctypes.c_char_p)).contents.value.decode())
        recipe = json.loads((prefix / "share/serein-ffmpeg/build.json").read_text())
        self.assertEqual(registered, set(recipe["encoders"]))
        self.assertEqual(registered, builder.encoder_names({name: recipe[name] for name in ("nvenc", "amf", "qsv", "videotoolbox")}))

    def test_amf_offline_rebuild_uses_shipped_headers_without_sdk(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(builder.urllib.request, "urlopen") as download:
            cache = Path(directory)
            source = dict(builder.SOURCES["amf-headers"])
            source["sha256"] = hashlib.sha256(b"shipped verified headers").hexdigest()
            archive = cache / source["file"]
            archive.write_bytes(b"shipped verified headers")
            with patch.dict(builder.SOURCES, {"amf-headers": source}):
                self.assertEqual(builder.amf_headers(cache, offline=True), archive)
            self.assertFalse((cache / builder.SOURCES["amf-sdk"]["file"]).exists())
            download.assert_not_called()

    def test_qsv_allocation_patch_fails_on_unexpected_upstream_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "libavcodec").mkdir()
            (root / "libavutil").mkdir()
            (root / "libavcodec/libavcodec.v").write_text("LIBAVCODEC_MAJOR { local: *; };\n")
            (root / "libavutil/libavutil.v").write_text("LIBAVUTIL_MAJOR { local: *; };\n")
            (root / "libavcodec/qsvenc.c").write_text("ret = av_new_packet(&pkt.pkt, q->packet_size);\n")
            (root / "configure").write_text('hevc_qsv_encoder_select="hevcparse qsvenc"\n')
            source_patch = builder.patch_ffmpeg(root)
            self.assertIn("+ret = ff_get_encode_buffer(avctx, &pkt.pkt, q->packet_size, 0);", source_patch)
            with self.assertRaisesRegex(ValueError, "exactly one"):
                builder.patch_ffmpeg(root)

    def test_msvc_arm64_does_not_require_uninstalled_assembler_preprocessor(self):
        self.assertIn("--arch=aarch64", builder.toolchain_options("Windows", True))
        self.assertIn("--disable-asm", builder.toolchain_options("Windows", True))
        self.assertNotIn("--disable-asm", builder.toolchain_options("Windows", False))
        self.assertEqual(builder.toolchain_options("Linux", True), [])
        self.assertEqual(builder.toolchain_options("Darwin", True), [])

    @unittest.skipUnless(os.environ.get("FFMPEG_DIR") and bundle.platform.system() == "Windows",
                         "Requires the native MSVC FFmpeg build and dumpbin")
    def test_windows_codec_dlls_do_not_require_dispatcher_or_cpp_runtime_dlls(self):
        prefix = Path(os.environ["FFMPEG_DIR"])
        for name in bundle.LIBRARIES["Windows"]:
            with self.subTest(library=name):
                dependencies = subprocess.check_output(["dumpbin", "/dependents", str(prefix / "bin" / name)], text=True)
                imports = {name.lower() for name in re.findall(r"\b[\w.-]+\.dll\b", dependencies, flags=re.I)}
                self.assertFalse(any(name.startswith(("libvpl", "vpl", "msvcp140", "vcruntime140")) for name in imports),
                                 f"Codec must use static dispatcher/CRT, got {sorted(imports)}")

    @unittest.skipUnless(os.name == "posix" and os.environ.get("FFMPEG_DIR") and bundle.platform.system() == "Linux",
                         "Requires the native Linux FFmpeg build; no media is opened")
    def test_host_decoder_plugin_does_not_bind_to_encoder_only_ffmpeg(self):
        prefix = Path(os.environ["FFMPEG_DIR"])
        codec = native_codec(prefix, "Linux", mode=ctypes.RTLD_GLOBAL)
        codec.avcodec_find_encoder_by_name.argtypes = [ctypes.c_char_p]
        codec.avcodec_find_encoder_by_name.restype = ctypes.c_void_p
        codec.avcodec_find_decoder_by_name.argtypes = [ctypes.c_char_p]
        codec.avcodec_find_decoder_by_name.restype = ctypes.c_void_p
        self.assertTrue(codec.avcodec_find_encoder_by_name(b"libopenh264"))
        for name in ("h264_vaapi", "hevc_vaapi", "av1_vaapi"):
            self.assertFalse(codec.avcodec_find_encoder_by_name(name.encode()))
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
                (notices / "build.json").write_text(json.dumps({"system": system, "nvenc": nvenc, "amf": nvenc, "qsv": nvenc,
                    "sources": {"ffmpeg": builder.SOURCES["ffmpeg"]}}))
                for name in ("configure.json", "build-ffmpeg.py", "serein-ffmpeg.patch", "COPYING.LGPLv2.1", "OpenH264-LICENSE",
                             "source/ffmpeg-7.1.5.tar.xz", "source/openh264-2.6.0-source.tar.bz2", "nv-codec-headers-README",
                             "source/nv-codec-headers-12.2.72.0.tar.gz", "AMF-LICENSE", "source/AMF-1.4.36-headers.tar",
                             "oneVPL-LICENSE", "oneVPL-third-party-programs.txt", "source/libvpl-2.14.0.tar.gz"):
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
                self.assertEqual("AMF-1.4.36-headers.tar" in names, nvenc)
                self.assertNotIn("AMF-1.4.36.tar.gz", names)
                self.assertEqual("libvpl-2.14.0.tar.gz" in names, nvenc)

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
