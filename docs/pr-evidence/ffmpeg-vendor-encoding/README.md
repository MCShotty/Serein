# AMD AMF and Intel Quick Sync verification — October 5, 2026

This extends the [FFmpeg migration](../ffmpeg-video-encoding/README.md).
Baseline `b4c32bb98edbe16e07b60bf8b2d2a138eb536e88` is that migration's verified
Linux release package, benchmark and native preview, copied before vendor changes.
The original project baseline and its measurements remain in the earlier report.
No UI layout change; native screenshots are not applicable to this backend work.

Windows/Linux selection is NVENC → AMD AMF → Intel QSV → OpenH264. macOS uses
VideoToolbox → OpenH264. A failed backend is excluded for the stream, including
bitrate restarts. Intel uses NV12 packing and a hardware-only oneVPL session.
Linux QSV uses a VA driver device as authorized by the owner; `h264_vaapi` and
Media Foundation encoding stay excluded. Missing drivers fall through to the
remaining backends. Windows ARM64 uses software encoding.

## Reproduction

Build the pinned native dependencies into a fresh prefix, set `FFMPEG_DIR`, then:

```sh
cargo xtask check
cargo fmt --manifest-path fuzz/Cargo.toml --all -- --check
python3 packaging/ffmpeg/test_bundle.py
python3 packaging/linux/test_package.py
python3 -m unittest discover -s packaging/flatpak -p test_build.py
bash packaging/macos/test_sign_release.sh
cargo build --locked -p discord-voice -p platform \
  --features winit/x11,winit/wayland --example linux_screen
target/debug/examples/linux_screen
target/debug/examples/linux_screen --niri-timestamps
cargo xtask package
```

The native library build registers exactly `libopenh264`, `h264_nvenc`,
`h264_amf` and `h264_qsv` on Linux x64, without decoders, CLI or VA-API encoder.
Host GStreamer's independently versioned FFmpeg decoder remains usable in the
same process. Native Windows import checks run only on Windows CI.

The [ABI harness](../ffmpeg-video-encoding/native_encoder.c) passes strict C11
warnings, ASan/UBSan and leak detection against the delivery libraries. Its
17 invalid/null requests and one-byte capacity/canary checks pass. Host `ffprobe`
decodes eight camera IDRs (Constrained Baseline) and eight screen frames (Main,
two IDRs and six deltas), with limited-range BT.601 matrix, BT.709 primaries and
sRGB transfer metadata. Only the shim/harness, rather than the upstream shared
libraries, are sanitizer-instrumented.

The [NV12 harness](../ffmpeg-video-encoding/native_nv12.c) includes the shim to
exercise its private helpers without opening a GPU. Compile it alone with the
same sanitizer/linker flags as the ABI harness. It verifies distinct U/V values,
padded rows, untouched canaries and rejected packet lengths larger than the
actual native backing allocation. The QSV source patch routes driver-advised
packet requests through the capped allocator before allocation/copying.
The harness also accepts the actual native option names, enum values and ranges
for both profiles on NVENC, AMF and QSV (six combinations), without opening a
device. This is option compatibility rather than GPU execution.

Full `cargo xtask check` passed 1,166 test executions with 26 ignored, plus
formatting, strict Clippy, production and policy checks. Fuzz formatting and
the cached-xtask workspace fixture pass. Both Linux screen variants and the
standard DEB pass. FFmpeg regression results are ten passes and one Windows-only
skip; the Debian fixture, four Flatpak preparation tests and synthetic signing
gates pass. The extracted package resolves all three codec libraries locally.

## Build and source provenance

The delivery prefix and retained builder match SHA-256
`5daeee263ee108de8958e315dc55ed3777cfb027d86c5e92f45bc3ee3a3e5be2`.
FFmpeg 7.1.5, OpenH264 2.6.0, NVENC headers 12.2.72.0, AMF headers 1.4.36 and
oneVPL 2.14.0 have separately recorded exact source pins. oneVPL is a static PIC
dispatcher inside the replaceable FFmpeg libraries; GPU runtimes stay provided
by the OS. Linux additionally links the distribution's libva/libva-drm/libdrm.
Windows CRT and native platform builds still require their CI runners.
Vendor license/notices are copied verbatim, including AMF's CRLF and upstream
trailing spaces. Task whitespace review excludes those two preserved notice
files; their exact hashes remain recorded in provenance.

The package includes 25,923,241 bytes of source archives. The 1,198,914-byte
OpenH264 source subset retains all 858 non-media entries with original bytes and
file modes, including build scripts, tests, docs, licenses and nested Android
resources. It omits only root `res/` test media. Its reproducible SHA-256 is
`783c8cdede353f0b0c77784a505438a32454917e9a95338b3b8d30a01f8ce0e5`.
The shared library was built from that same subset. The AMF subset retains all
57 public headers and its license, without the complete 171-MiB SDK. Offline
source selection succeeds with both original full archives absent. The retained
FFmpeg patch and recipe permit rebuilding from the packaged sources.

## Measurement method and limits

Use the existing [measurement helper](../ffmpeg-video-encoding/measure.py) and
its release camera/native-preview build commands. The camera run uses the real
`CameraEncoder`, a deterministic 640×480 RGB gradient, 30 warmup frames and
300 timed frames. One process warmup precedes five alternating measured pairs.
This machine has no GPU device, so both revisions use software encoding. This
does not measure AMD/Intel speed, quality, GPU memory or active screen capture.

Native preview sampling uses `profile_preview --demo --interactive
--page=appearance --width=1120 --height=760`, dark theme, scale 1, Xvfb :88,
forced Mesa lavapipe Vulkan and verified mapped driver. Each launch warms eight
seconds, moves the pointer to (1100,740), settles three seconds and receives
twenty one-second psutil CPU/RSS samples. Two fresh pairs reverse argument order.
CPU is percent of one logical core, peak RSS is sampled after warmup, and settled
RSS is the median of each launch's last five samples. No media adapter runs in
this fixture; software rendering and allocator variation affect the result.

Standard package measurements retain ordinary fat LTO, one codegen unit,
stripping and `--no-default-features` without demo. Installed byte counts sum
regular files extracted from the DEB, excluding symlinks/allocation overhead.
The camera/preview run directly after all compiler work stops. Raw samples and
final source/binary identities are in [measurements.json](measurements.json),
with a compact comparison in [performance.md](../../performance.md).

AMF's pinned wrapper does not forward forced picture types. Main-profile screen
IDR requests reopen it once output has started, avoiding a reopen loop while
startup output is pending. Camera GOP 1 does not reopen per frame. Reset cost and
actual hardware operation are unmeasured. Native SDK initialization, polling and
shutdown may block despite bounded queues; worker retirement waits for them.
Live account/call, native portal/camera capture, Windows/macOS, actual Nix/Flatpak
builds and dedicated license CI remain outside these local synthetic checks.
