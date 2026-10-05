#!/usr/bin/env python3
"""Offline driver-query regression fixtures. Never load real GPU drivers.

Usage: python3 crates/discord-voice/tests/native/test_video_queries.py --prefix "$FFMPEG_DIR"
Linux loader fixtures complement the native Windows/macOS compilation checks.
"""
import argparse
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile


def run(prefix):
    if sys.platform != "linux":
        print("Linux driver-loader fixtures skipped; native platform builds still required")
        return
    here = Path(__file__).resolve().parent
    source = here.parents[1] / "src"
    include = prefix / "include"
    compiler = shlex.split(os.environ.get("CC", "cc"))
    flags = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-I" + str(include)]
    with tempfile.TemporaryDirectory(prefix="serein-driver-fixtures-") as directory:
        out = Path(directory)
        if (include / "ffnvcodec/nvEncodeAPI.h").is_file():
            for name, soname in [("cuda", "libcuda.so.1"), ("nvenc", "libnvidia-encode.so.1")]:
                subprocess.run([*compiler, *flags, "-fPIC", "-shared",
                                str(here / f"video_query_nvenc_{name}.c"),
                                "-Wl,-soname," + soname, "-o", str(out / soname)], check=True)
            executable = out / "nvenc-test"
            subprocess.run([*compiler, *flags, "-DSEREIN_HAVE_NVENC_QUERY=1",
                            str(here / "video_query_nvenc_test.c"), str(source / "video_query_nvenc.c"),
                            "-L" + str(out), "-Wl,--no-as-needed", "-l:libcuda.so.1",
                            "-l:libnvidia-encode.so.1", "-ldl", "-o", str(executable)], check=True)
            subprocess.run([str(executable)], env=dict(os.environ, LD_LIBRARY_PATH=str(out)), check=True)
        else:
            print("NVENC SDK absent: loader fixture skipped")
        if (include / "vpl/mfxdispatcher.h").is_file():
            executable = out / "qsv-test"
            subprocess.run([*compiler, *flags, "-DSEREIN_HAVE_QSV_QUERY=1",
                            str(here / "video_query_qsv_test.c"), str(source / "video_query_qsv.c"),
                            "-o", str(executable)], check=True)
            subprocess.run([str(executable)], check=True)
        else:
            print("QSV SDK absent: dispatcher fixture skipped")
        executable = out / "dispatcher-test"
        subprocess.run([*compiler, *flags, "-DSEREIN_HAVE_AMF_QUERY=1",
                        str(here / "video_query_dispatcher_test.c"), str(source / "video_query.c"),
                        "-o", str(executable)], check=True)
        subprocess.run([str(executable)], check=True)
        executable = out / "no-sdk-test"
        subprocess.run([*compiler, *flags, str(here / "video_query_no_sdk_test.c"),
                        *(str(source / name) for name in ["video_query.c", "video_query_nvenc.c",
                                                        "video_query_qsv.c", "video_query_videotoolbox.c"]),
                        "-o", str(executable)], check=True)
        subprocess.run([str(executable)], check=True)
    if (include / "AMF/core/Factory.h").is_file():
        spec = importlib.util.spec_from_file_location("amf_fixture", here / "test_amf_query.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.run(include)
    else:
        print("AMF SDK absent: component fixture skipped")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    run(parser.parse_args().prefix.resolve())
