# FFmpeg outgoing video verification

Baseline: `6313e271e9c34b39069eb20787afb1476741b21c`, initially clean `main`.
The implementation uses the pinned Rust 1.98.1 toolchain and locked dependencies.
No UI layout or interaction change; the active encoder diagnostic label now names
FFmpeg. Native before/after screenshots are not applicable to this backend change.

## Reproduction

Build the native libraries, then run the ordinary repository checks and package:

```sh
python3 scripts/build-ffmpeg.py
export FFMPEG_DIR="$PWD/target/ffmpeg/prefix"
cargo xtask check
cargo xtask package
python3 packaging/ffmpeg/test_bundle.py
cargo run --locked -p discord-voice --example linux_screen
cargo run --locked -p discord-voice --example linux_screen -- --niri-timestamps
```

The Linux example uses synthetic video/audio and pre-cancelled portal requests.
It checks raw capture admission, local preview, secure readiness, transport
pressure and independently decodable FFmpeg H.264 without capturing a desktop,
camera or microphone. The FFmpeg packaging test checks explicit library/source
staging and coexistence with a separately versioned host FFmpeg decoder plugin.

The local Linux container used Debian 13, GStreamer 1.26, an Intel Xeon Platinum
8573C (five visible logical CPUs), and approximately 17 GiB visible RAM. No native
GPU, camera, microphone or logged-in account was used. Default debug artifacts
exhausted the 32-GiB workspace; the successful checks disable dev/test debug
symbols and incremental caches with `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, and three build jobs. Standard
release/package settings are unchanged.

## Release workload

`camera::synthetic_camera_encode_workload` calls the actual `CameraEncoder` with
a deterministic 640×480 RGB gradient, including RGB allocation/conversion and
H.264 encoding. Each process warms the encoder with 30 frames, then times 300
frames with `Instant`, requiring 300 bounded encoded packets. The same test-only
function was appended to the detached baseline worktree; baseline production
code and its previously built standard package were unchanged.

Build once per worktree, then execute the emitted test binary directly for one
warmup process and five alternating measured baseline/current pairs:

```sh
cargo test --release --locked -p discord-voice -p platform \
  --features winit/x11,winit/wayland --lib synthetic_camera_encode_workload --no-run
target/release/deps/discord_voice-<hash> \
  --exact camera::synthetic_camera_encode_workload --ignored --nocapture
```

This machine exercises software encoding. Results measure a synthetic camera
component, not screen conversion, hardware encoding, end-to-end call latency or
live Discord interoperability. Package byte counts include the replaceable native
libraries, corresponding FFmpeg source and notices. Installed bytes sum regular
files extracted with `dpkg-deb --extract`, excluding symlinks and allocation
overhead. See `measurements.json` and the current entry in `docs/performance.md`
for the final source/binary identities, samples and limitations.

Five measured camera runs have medians of 1,271.643 ms before and 1,061.770 ms
after (−16.50%). The old camera uses one OpenH264 thread; the FFmpeg wrapper uses
two and different defaults. This is not a quality-matched speed comparison.
Standard executable bytes are 85,705,128 → 85,702,192; installed regular-file
bytes are 90,219,249 → 104,848,408; complete DEB bytes are
44,277,080 → 56,252,124. Source/notices and three shared libraries account for
the package increase.

The [measurement helper](measure.py) records direct camera runs and whole native
preview process samples. It requires Python `psutil`; idle mode also needs
`xdotool`, Xvfb and the Debian Mesa lavapipe ICD. Build both examples first, start
an isolated Xvfb server, then invoke the helper with the two binary paths and an
output JSON path whose parent directory exists:

```sh
cargo rustc --release --locked -p serein --no-default-features --features demo \
  --example profile_preview -- -C lto=off
Xvfb :88 -screen 0 1120x760x24 -nolisten tcp
python3 docs/pr-evidence/ffmpeg-video-encoding/measure.py idle \
  /path/to/baseline/target/release/examples/profile_preview \
  target/release/examples/profile_preview target/idle.json
```

The fixture opens dark Appearance at scale 1. Each launch receives eight seconds
of warmup, a pointer move to (1100,740), three seconds of settling and twenty
one-second CPU/RSS samples. Repeat with reversed binary arguments; the second
JSON's phase labels follow its argument positions. There were no child processes.
CPU is percent of one logical core; peak RSS is sampled after warmup, and settled
RSS is the median of the final five samples. Software rendering consumed several
cores: per-launch baseline/after mean pairs were 356.130→385.475%, then
387.105→383.970%. The direction reverses, and a thread sample found five
`llvmpipe` workers dominating CPU. No media adapters run in this preview, so these
samples do not isolate encoding cost or establish a production-idle improvement.

`cargo xtask check` passed in full: 1,162 passing test executions and 26 ignored,
including the opt-in benchmark. Strict Clippy, format, production and policy
checks passed. The native ABI check below, both Linux screen harness variants,
six FFmpeg packaging tests, the synthetic Debian package regression, four offline
Flatpak preparation tests and the offline macOS signing fixture passed. Actual
Windows/macOS, Nix/Flatpak and hardware builds remain unverified locally.

The measured Linux package retains native recipe SHA-256
`b6d02591d4da6afa8e16f11ae3db387e1dcfbf84ac3a63d85737e7dfc426eb5a`
and its matching bundled builder source. The final repository builder also adds
`--disable-asm` for Windows ARM64 to avoid an unavailable `gas-preprocessor.pl`.
Linux configure options and native code are unchanged; existing provenance was
not restamped. Its strict recipe cache requires a fresh prefix for future builds.

The [native ABI harness](native_encoder.c) also checks rejected settings, null
arguments, exact input lengths, one-byte native/output limits, output canaries,
eight independently decodable camera IDRs and a Main-profile screen sequence
with six delta frames and a forced IDR. Run the shim under ASan/UBSan, including
leak detection, from an ignored output directory (Linux example):

```sh
mkdir -p target/native-ffmpeg-evidence
cc -std=c11 -Wall -Wextra -Wpedantic -Wconversion -Wshadow -Werror \
  -fsanitize=address,undefined -fno-omit-frame-pointer -g \
  -I crates/discord-voice/src -isystem "$FFMPEG_DIR/include" \
  crates/discord-voice/src/video_encode_ffmpeg.c \
  docs/pr-evidence/ffmpeg-video-encoding/native_encoder.c \
  -L "$FFMPEG_DIR/lib" -Wl,-rpath,"$FFMPEG_DIR/lib" \
  -lavcodec-serein -lavutil-serein -o target/native-ffmpeg-evidence/native_encoder
(cd target/native-ffmpeg-evidence && ASAN_OPTIONS=detect_leaks=1 ./native_encoder)
```

An independently installed `ffprobe -count_frames` verifies eight decoded frames
per stream and limited-range BT.601 matrix / BT.709 primaries / sRGB transfer
metadata. The shipped FFmpeg build contains no CLI or decoders; only the shim is
instrumented in this check.

## Platform limits

NVENC and VideoToolbox remain selected when available, with sticky software
fallback after hardware encoding failure. The local host cannot validate those
drivers, Windows/macOS builds, native portal capture or owner-controlled calls.
Incoming decoding and platform capture remain separate from outgoing encoding.
Linux now uses CPU BGRA readback, scaling and I420 conversion before hardware
upload; native conversion cost needs measurement on an actual desktop.

Existing Windows installations need the installer or manual extraction for the
first FFmpeg upgrade: their old updater rejects the new root-level DLL names.
The new updater admits and requires the complete native video payload for later
updates. Dedicated license CI and the native platform CI matrix are separate
checks; local packaging copies source/notices without gating on license coverage.
