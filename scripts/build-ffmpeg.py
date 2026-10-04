#!/usr/bin/env python3
"""Build Serein's LGPL-only shared video encoders; never capture or open media.

Linux/macOS: Python 3.12+, make, pkg-config, C/C++ compiler, nasm; Linux also
needs patchelf. Windows: run from MSYS2 bash with make/pkgconf/nasm and the
MSVC developer environment (x64 or arm64). Sources are checksum verified.
"""

import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import urllib.request


SOURCES = {
    "ffmpeg": {
        "file": "ffmpeg-7.1.5.tar.xz",
        "url": "https://ffmpeg.org/releases/ffmpeg-7.1.5.tar.xz",
        "sha256": "de668509caf9e35e3cd162473441fdb29538c6d96ed080292b3cf9e6fc5d558f",
    },
    "openh264": {
        "file": "openh264-2.6.0.tar.gz",
        "url": "https://codeload.github.com/cisco/openh264/tar.gz/refs/tags/v2.6.0",
        "sha256": "558544ad358283a7ab2930d69a9ceddf913f4a51ee9bf1bfb9e377322af81a69",
    },
    "nv-codec-headers": {
        "file": "nv-codec-headers-12.2.72.0.tar.gz",
        "url": "https://codeload.github.com/FFmpeg/nv-codec-headers/tar.gz/refs/tags/n12.2.72.0",
        "sha256": "dbeaec433d93b850714760282f1d0992b1254fc3b5a6cb7d76fc1340a1e47563",
    },
}


def run(*args, cwd=None, env=None):
    subprocess.run([str(arg) for arg in args], cwd=cwd, env=env, check=True)


def posix(path):
    """MSVC is driven by MSYS make/configure; use its paths in build recipes."""
    if os.name == "nt":
        return subprocess.check_output(["cygpath", "-u", str(path)], text=True).strip()
    return str(path)


def fetch(source, cache, offline):
    destination = cache / source["file"]
    if not destination.exists():
        if offline:
            raise ValueError(f"Offline source missing: {destination}")
        temporary = destination.with_suffix(destination.suffix + ".download")
        request = urllib.request.Request(source["url"], headers={"User-Agent": "Serein-source-build"})
        try:
            with urllib.request.urlopen(request, timeout=60) as response, temporary.open("wb") as output:
                total = 0
                while block := response.read(1024 * 1024):
                    total += len(block)
                    if total > 128 * 1024 * 1024:
                        raise ValueError("Dependency source exceeds 128 MiB")
                    output.write(block)
            temporary.replace(destination)
        finally:
            temporary.unlink(missing_ok=True)
    if hashlib.sha256(destination.read_bytes()).hexdigest() != source["sha256"]:
        raise ValueError(f"Source SHA-256 mismatch: {destination}")
    return destination


def unpack(archive, destination):
    destination.mkdir(parents=True)
    with tarfile.open(archive) as source:
        members = source.getmembers()
        roots = {Path(item.name).parts[0] for item in members if Path(item.name).parts}
        if len(roots) != 1:
            raise ValueError(f"Expected one source root: {archive}")
        if sum(item.size for item in members) > 1024 * 1024 * 1024:
            raise ValueError("Expanded source exceeds 1 GiB")
        source.extractall(destination, filter="data")
    return destination / roots.pop()


def toolchain_options(system, arm64):
    if system != "Windows":
        return []
    options = ["--toolchain=msvc", "--target-os=win32", f"--arch={'aarch64' if arm64 else 'x86_64'}"]
    if arm64:
        # FFmpeg 7's MSVC ARM assembly needs gas-preprocessor.pl, which our
        # build tools do not include. Match OpenH264's C-only Windows ARM build.
        options.append("--disable-asm")
    return options


def build(args):
    system = platform.system()
    if system.startswith(("MSYS", "MINGW", "CYGWIN")):
        system = "Windows"
    if system not in {"Linux", "Darwin", "Windows"}:
        raise ValueError(f"Unsupported native FFmpeg build host: {system}")
    architecture = os.environ.get("VSCMD_ARG_TGT_ARCH", platform.machine()).lower()
    arm64 = architecture in {"arm64", "aarch64"}
    if architecture not in {"x86_64", "amd64", "x64", "arm64", "aarch64"}:
        raise ValueError(f"Unsupported native FFmpeg architecture: {architecture}")
    nvenc = system in {"Linux", "Windows"} and not (system == "Windows" and arm64)
    prefix = args.prefix.resolve()
    cache = args.cache_dir.resolve()
    work = args.work_dir.resolve()
    cache.mkdir(parents=True, exist_ok=True)
    recipe = {"sources": SOURCES, "system": system, "architecture": architecture,
              "nvenc": nvenc, "videotoolbox": system == "Darwin", "recipe_sha256":
              hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    stamp = prefix / "share/serein-ffmpeg/build.json"
    if stamp.is_file() and json.loads(stamp.read_text()) == recipe:
        print(f"Reusing verified FFmpeg build: {prefix}")
        return prefix
    if prefix.exists() and any(prefix.iterdir()):
        raise ValueError(f"FFmpeg prefix contains another build; choose a fresh prefix: {prefix}")
    if work.exists() and any(work.iterdir()):
        raise ValueError(f"FFmpeg work directory contains an incomplete build; choose a fresh work directory: {work}")
    work.mkdir(parents=True, exist_ok=True)
    archives = {name: fetch(source, cache, args.offline) for name, source in SOURCES.items()
                if name != "nv-codec-headers" or nvenc}
    trees = {name: unpack(archive, work / name) for name, archive in archives.items()}
    # Keep host GStreamer's FFmpeg in its own ELF symbol namespace. A distinct
    # SONAME alone cannot prevent LIBAVCODEC_61/LIBAVUTIL_59 interposition.
    version_patch = []
    for name, namespace in (("avcodec", "LIBAVCODEC"), ("avutil", "LIBAVUTIL")):
        relative = f"lib{name}/lib{name}.v"
        path = trees["ffmpeg"] / relative
        before = path.read_text()
        after = before.replace(namespace + "_MAJOR", "SEREIN_" + namespace + "_MAJOR")
        if before == after:
            raise ValueError(f"Expected FFmpeg version namespace missing: {relative}")
        version_patch.extend(difflib.unified_diff(before.splitlines(keepends=True), after.splitlines(keepends=True),
                                                fromfile="a/" + relative, tofile="b/" + relative))
        path.write_text(after)
    env = dict(os.environ)
    env["PKG_CONFIG_PATH"] = posix(prefix / "lib/pkgconfig") + os.pathsep + env.get("PKG_CONFIG_PATH", "")
    # MSYS pkgconf uses ':' even when driven by Windows Python.
    if system == "Windows":
        env["PKG_CONFIG_PATH"] = posix(prefix / "lib/pkgconfig")
    openh264_args = [f"PREFIX={posix(prefix)}", f"ARCH={'arm64' if arm64 else 'x86_64'}"]
    if system == "Windows":
        openh264_args += ["OS=msvc", "USE_ASM=No" if arm64 else "USE_ASM=Yes"]
    run("make", f"-j{args.jobs}", *openh264_args, "install-shared", cwd=trees["openh264"], env=env)
    if system == "Windows":
        # FFmpeg/pkgconf's -lopenh264 must select the shared import library.
        shutil.copyfile(prefix / "lib/openh264_dll.lib", prefix / "lib/openh264.lib")
    if nvenc:
        run("make", f"PREFIX={posix(prefix)}", "install", cwd=trees["nv-codec-headers"], env=env)
    configure = [
        f"--prefix={posix(prefix)}", "--build-suffix=-serein", "--disable-autodetect", "--disable-everything",
        "--disable-programs", "--disable-doc", "--disable-network", "--disable-static", "--enable-shared",
        "--disable-gpl", "--disable-nonfree", "--disable-version3", "--disable-avdevice", "--disable-avformat",
        "--disable-avfilter", "--disable-swscale", "--disable-swresample", "--disable-postproc",
        "--disable-vaapi", "--disable-mediafoundation", "--disable-vdpau", "--disable-vulkan",
        "--disable-d3d11va", "--disable-dxva2", "--disable-cuvid", "--disable-nvdec", "--disable-cuda-llvm",
        "--enable-avcodec", "--enable-avutil", "--enable-libopenh264", "--enable-encoder=libopenh264",
        f"--extra-cflags=-I{posix(prefix / 'include')}", f"--extra-ldflags=-L{posix(prefix / 'lib')}",
    ]
    if nvenc:
        configure += ["--enable-ffnvcodec", "--enable-nvenc", "--enable-encoder=h264_nvenc"]
    else:
        configure += ["--disable-ffnvcodec", "--disable-nvenc"]
    if system == "Darwin":
        configure += ["--enable-videotoolbox", "--enable-encoder=h264_videotoolbox", "--install-name-dir=@rpath"]
    else:
        configure += ["--disable-videotoolbox"]
    configure += toolchain_options(system, arm64)
    if system == "Windows":
        configure = [arg for arg in configure if not arg.startswith("--extra-ldflags=")]
        configure += [f"--extra-ldflags=-libpath:{(prefix / 'lib').as_posix()}"]
    run("bash", "configure", *configure, cwd=trees["ffmpeg"], env=env)
    configuration = (trees["ffmpeg"] / "config.h").read_text()
    if "#define CONFIG_GPL 0" not in configuration or "#define CONFIG_NONFREE 0" not in configuration:
        raise ValueError("FFmpeg build must remain LGPL without GPL/nonfree components")
    run("make", f"-j{args.jobs}", cwd=trees["ffmpeg"], env=env)
    run("make", "install", cwd=trees["ffmpeg"], env=env)
    if system == "Windows":
        # FFmpeg installs MSVC import libraries with DLLs in bin; Cargo searches lib.
        for name in ("avcodec-serein.lib", "avutil-serein.lib"):
            shutil.copyfile(prefix / "bin" / name, prefix / "lib" / name)
    if system == "Linux":
        for library in (prefix / "lib").glob("*.so.*"):
            if not library.is_symlink():
                # Keep compiler-runtime store paths in Nix builds, while making
                # the replaceable sibling codecs resolve before those fallbacks.
                original = subprocess.check_output(["patchelf", "--print-rpath", str(library)], text=True).strip()
                paths = [path for path in original.split(":") if path and path != str(prefix / "lib") and path != "$ORIGIN"]
                run("patchelf", "--set-rpath", ":".join(["$ORIGIN", *paths]), library)
    if system == "Darwin":
        for library in (prefix / "lib").glob("*.dylib"):
            if library.is_symlink():
                continue
            # FFmpeg already uses @rpath and its major-version IDs. The physical
            # filenames include minor versions and are not shipped as aliases.
            if library.name.startswith("libopenh264"):
                run("install_name_tool", "-id", "@rpath/libopenh264.8.dylib", library)
            dependencies = subprocess.check_output(["otool", "-L", str(library)], text=True).splitlines()[1:]
            for line in dependencies:
                name = line.strip().split(" (", 1)[0]
                if name.startswith(str(prefix / "lib") + "/"):
                    run("install_name_tool", "-change", name, f"@rpath/{Path(name).name}", library)
        # Apple Silicon rejects modified Mach-O files whose linker signatures
        # were invalidated above. Sign the source-run/Nix libraries after every
        # install-name edit; packaging signs its staged copies again afterward.
        for library in (prefix / "lib").glob("*.dylib"):
            if not library.is_symlink():
                run("codesign", "--force", "--sign", "-", library)
    notices = stamp.parent
    (notices / "source").mkdir(parents=True)
    # The LGPL corresponding FFmpeg source travels with every official binary.
    shutil.copyfile(archives["ffmpeg"], notices / "source" / SOURCES["ffmpeg"]["file"])
    shutil.copyfile(Path(__file__), notices / "build-ffmpeg.py")
    (notices / "configure.json").write_text(json.dumps(configure, indent=2) + "\n")
    (notices / "serein-ffmpeg.patch").write_text("".join(version_patch))
    shutil.copyfile(trees["ffmpeg"] / "COPYING.LGPLv2.1", notices / "COPYING.LGPLv2.1")
    shutil.copyfile(trees["openh264"] / "LICENSE", notices / "OpenH264-LICENSE")
    if nvenc:
        shutil.copyfile(trees["nv-codec-headers"] / "README", notices / "nv-codec-headers-README")
        shutil.copyfile(archives["nv-codec-headers"], notices / "source" / SOURCES["nv-codec-headers"]["file"])
    stamp.write_text(json.dumps(recipe, indent=2) + "\n")
    print(f"FFMPEG_DIR={prefix}")
    return prefix


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, default=Path("target/ffmpeg/prefix"))
    parser.add_argument("--work-dir", type=Path, default=Path("target/ffmpeg/build"))
    parser.add_argument("--cache-dir", type=Path, default=Path("target/ffmpeg/sources"))
    parser.add_argument("--jobs", type=int, default=min(os.cpu_count() or 1, 8))
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if not 1 <= args.jobs <= 256:
        parser.error("--jobs must be between 1 and 256")
    build(args)


if __name__ == "__main__":
    main()
