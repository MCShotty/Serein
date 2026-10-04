# Shared FFmpeg video encoders

Camera and screen-share sending link to a small FFmpeg 7.1.5 LGPL-2.1-or-later
build of libavcodec/libavutil. It enables source-built OpenH264 2.6.0, NVIDIA
NVENC on Linux/Windows x64 and VideoToolbox on macOS. Windows ARM64 uses OpenH264.
The app chooses from these encoders only. Media Foundation and VA-API encoding,
GPL/nonfree components, command-line programs, networking, demuxers, decoders,
filters and scaling/resampling libraries are disabled in the bundled build.
Native capture and incoming decoding have separate platform dependencies.

On Linux/macOS install Python 3.12+, make, pkg-config, a C/C++ compiler and nasm;
Linux also needs patchelf. Then, from the repository root:

```sh
python3 scripts/build-ffmpeg.py --jobs 8
export FFMPEG_DIR="$PWD/target/ffmpeg/prefix"
cargo xtask check
cargo xtask package
```

Windows uses MSYS2 bash with make, pkgconf and nasm, and an initialized MSVC
developer environment matching the Rust target. Run the same builder through
Windows Python, then set `FFMPEG_DIR` to its absolute native prefix for Cargo.
The Windows CI jobs show the complete setup. Source archives have exact SHA-256
pins. Builds reuse a matching completed prefix; a failed/incompatible build
requires fresh `--prefix` and `--work-dir` paths. `--offline` consumes previously
downloaded archives in `--cache-dir` without network access.

Packages contain replaceable shared libraries and the complete corresponding
FFmpeg source archive, LGPL text, build recipe, source hashes, namespace patch and configure
arguments in `ffmpeg-source` (Linux: `share/doc/serein/ffmpeg-source`). OpenH264
and NVENC headers retain their BSD/MIT notices; OpenH264 is compiled from source,
without assuming Cisco binary-download patent coverage. Rebuilding a library
requires the compiler/tool versions recorded by the distributor's build logs.
Serein's MIT/Apache application sources remain available in the repository.
The FFmpeg build uses the `-serein` library suffix and `SEREIN_LIBAVCODEC_61` /
`SEREIN_LIBAVUTIL_59` ELF symbol versions so it can coexist with the host
FFmpeg used by GStreamer's incoming-video plugins. The two version-script
changes are retained as `serein-ffmpeg.patch` with the original source archive.

On Linux installed libraries live in `lib/serein`, with executable-relative
lookup; the portable staging tree uses `lib`. macOS uses `Contents/Frameworks`
with `@rpath` install names and signs each library before the app bundle.
Windows places the DLLs alongside `serein.exe`. FFmpeg loads NVIDIA driver
libraries only when attempting NVENC; driver or sandbox access failures select
OpenH264. No broader Flatpak permissions are added. Hardware availability and
live Discord delivery still require validation on the actual platform.
