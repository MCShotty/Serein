# Shared FFmpeg video encoders

Camera and screen-share sending link to a small FFmpeg 7.1.5 LGPL-2.1-or-later
build of libavcodec/libavutil. It enables source-built OpenH264 2.6.0, NVIDIA
NVENC and AMD AMF on Linux/Windows x64, Intel Quick Sync on Linux/Windows x64,
and VideoToolbox on macOS. Linux ARM64 builds include NVENC/AMF; driver support
determines whether they can open. Windows ARM64 uses OpenH264.
The app chooses from these encoders only. Media Foundation and the `h264_vaapi` encoder,
GPL/nonfree components, command-line programs, networking, demuxers, decoders,
filters and scaling/resampling libraries are disabled in the bundled build.
Native capture and incoming decoding have separate platform dependencies.

On Linux/macOS install Python 3.12+, make, pkg-config, a C/C++ compiler and nasm;
Linux also needs CMake, patchelf, libva and libdrm development packages. Then,
from the repository root:

```sh
python3 scripts/build-ffmpeg.py --jobs 8
export FFMPEG_DIR="$PWD/target/ffmpeg/prefix"
cargo xtask check
cargo xtask package
```

Windows uses MSYS2 bash with make, pkgconf and nasm, native CMake/NMake, and an initialized MSVC
developer environment matching the Rust target. Run the same builder through
Windows Python, then set `FFMPEG_DIR` to its absolute native prefix for Cargo.
The Windows CI jobs show the complete setup. Source archives have exact SHA-256
pins. Builds reuse a matching completed prefix; a failed/incompatible build
requires fresh `--prefix` and `--work-dir` paths. `--offline` consumes previously
downloaded archives in `--cache-dir` without network access.

Packages contain replaceable shared libraries and the complete corresponding
FFmpeg source archive, LGPL text, build recipe, source hashes, source patch and configure
arguments in `ffmpeg-source` (Linux: `share/doc/serein/ffmpeg-source`). OpenH264
and NVENC headers retain their BSD/MIT notices. AMF 1.4.36 public headers retain
AMD's MIT license and standards/patent notice. Intel oneVPL 2.14.0's MIT license,
third-party notice and complete source archive accompany its statically linked
PIC dispatcher; no Intel GPU runtime or AMD driver is redistributed. OpenH264 is compiled from source,
without assuming Cisco binary-download patent coverage. Rebuilding a library
requires the compiler/tool versions recorded by the distributor's build logs.
Serein's MIT/Apache application sources remain available in the repository.
The FFmpeg build uses the `-serein` library suffix and `SEREIN_LIBAVCODEC_61` /
`SEREIN_LIBAVUTIL_59` ELF symbol versions so it can coexist with the host
FFmpeg used by GStreamer's incoming-video plugins. The version-script changes
and QSV allocation change are retained as `serein-ffmpeg.patch` with the original
source archive. QSV requests packet allocation through the app's capped buffer
callback, so an excessive driver-advised packet size fails before allocation.

Packages include FFmpeg, NVENC-header and oneVPL archives for the enabled
platform, a deterministic 1,198,914-byte OpenH264 source archive, and a
deterministic 620 KiB AMF public-header archive. The OpenH264 subset omits only
the upstream root `openh264-2.6.0/res/` directory of test media. Every other
source/build/test/docs file, license, executable mode and nested Android
`res/` directory is preserved. Sorted USTAR entries use fixed ownership and
timestamps with bzip2 level 9 compression; the original and subset hashes are
recorded separately. The shared-library build uses this same subset, and tests
needing the omitted media require the original upstream archive.
The AMF subset contains only upstream `LICENSE.txt` and `amf/public/include/` from
the checked 1.4.36 SDK archive; it is not the complete SDK. `build.json` records
both original SDK and subset SHA-256 hashes. The builder regenerates the same
plain USTAR subset from the pinned SDK when needed, with sorted paths and fixed
metadata. To rebuild from a package without downloading the 171 MiB SDK, copy
its `ffmpeg-source` directory to a writable location, then run there:

```sh
python3 build-ffmpeg.py --offline --cache-dir source \
  --prefix rebuilt/prefix --work-dir rebuilt/work --jobs 8
```

Use the original package's OS/architecture and install the build prerequisites.
The recipe applies its recorded FFmpeg changes automatically. The static oneVPL
dispatcher's pkg-config file provides the required Linux C++/thread/dl link
flags and Windows registry/COM/GUID libraries without statically linking libva.

On Linux installed libraries live in `lib/serein`, with executable-relative
lookup; the portable staging tree uses `lib`. macOS uses `Contents/Frameworks`
with `@rpath` install names and signs each library before the app bundle.
Windows places the three DLLs alongside `serein.exe`. OpenH264's MSVC recipe
uses `-MT`; oneVPL's Windows build selects its static CRT, and FFmpeg explicitly
uses `-MT` too. Dispatcher-owned objects are released by the matching SDK APIs;
FFmpeg owns its frame/packet buffers. This avoids adding a Visual C++ runtime or
dispatcher DLL requirement for software fallback. Windows CI checks actual DLL
imports for those dependencies; that binary check still requires a Windows
runner. FFmpeg loads NVIDIA/AMD driver libraries only when attempting those
encoders, and the oneVPL dispatcher loads a system-provided compatible Intel
GPU runtime (`libmfx-gen.so.1.2` or
`libmfxhw64.so.1` on Linux). Linux QSV uses libva/libva-drm/libdrm to create the
Intel driver device; VA-API encoding remains disabled. Windows AMF/QSV use
D3D11 devices. AMF can create its own Vulkan context on Linux without enabling
FFmpeg's Vulkan subsystem. Driver or sandbox access failures select OpenH264.
No broader Flatpak permissions are added. Hardware availability and
live Discord delivery still require validation on the actual platform.
