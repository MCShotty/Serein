# Resolution through 8K and camera frame rate through 60 fps

Task baseline: `623ef63514c1b68c516bc4b047503cead6b23511` on the fork feature
branch. The upstream PR's equivalent baseline is `d53f46a0a3a403abfaef82b9ab165a2939afed7d`;
both have tree `1845aedc3bb1820ac2f24f0a6db5e9c904dcffd6`. This record describes
this addition, separately from the earlier FFmpeg/driver-query/reliability stages.

Camera and screen sharing now offer 480p, 720p, 1080p, 1440p, 4K and 8K. The camera
additionally offers saved 15/30/60 fps controls on both Stable and Experimental at
the bottom of Voice & Video. Defaults remain camera 640×480/15 fps and screen
1280×720/30 fps. Higher camera resolution requires sufficient native input pixels;
rate selection requests the closest supported native interval and caps output.
No small camera image is enlarged to claim a higher-resolution native source.
Native hardware and workload determine the actual frame rate.

## Native screenshots

All images were opened and inspected. The primary pairs use the same real
UI crate, default Inter, dark appearance, 1120×760 viewport, 100% scale and offline
synthetic data. The camera pair is scrolled to the end of Voice & Video, with
Experimental/H.265 and explicitly synthetic hardware rows. Before has no camera
resolution/rate controls; After selects 8K/60 fps. The screen pair selects the same
720p/30 fps source in both images and shows the newly available presets. Share is
disabled in the offline demo; no source is captured.

- `camera-before.png`, `camera-after.png`: matched settings pair.
- `screen-before.png`, `screen-after.png`: matched screen picker pair.
- `camera-narrow-light-after.png`: 760×900, light, 125% scale, AV1/8K/60 fps,
  scrolled to the bottom. Presets wrap and keyboard selection remains reachable.
- `screen-narrow-light-after.png`: 760×900, light, 150% scale. Presets wrap in the
  independently scrollable picker body; the Cancel/Share footer remains reachable.

Every PNG is smaller than 2 MiB. These are native egui/eframe helper captures,
not a standard production desktop launch, browser mockups, physical encoder
reports, or live Discord evidence. The helper excludes the production GTK/native
application adapters because local development packages are unavailable.

### Reproduce the offline helpers

Use the pinned toolchain and locked real dependencies. Create a detached baseline
worktree without switching the task branch:

```sh
git worktree add --detach /tmp/serein-8k-baseline 623ef63514c1b68c516bc4b047503cead6b23511
python3 docs/pr-evidence/video-8k-resolution/prepare-preview.py /tmp/serein-8k-before /tmp/serein-8k-baseline
python3 docs/pr-evidence/video-8k-resolution/prepare-preview.py /tmp/serein-8k-after
```

The preparer reuses the existing native preview and explicitly synthetic driver
fixture. At the baseline it adds only a `screen-share` launcher branch calling
`call_demo_state()` and `preview_screen_share()`; the baseline UI/capture code
stays unchanged. Current source includes that branch in the offline example.
The baseline performance helper was preserved before that launcher addition.

Build helpers sequentially and preserve each binary before sharing a Cargo target
directory. The prepared standalone manifest needs a helper-only lock entry; an
initial `cargo build --offline --manifest-path …/Cargo.toml` resolves it against
the copied pinned lockfile/cache. Subsequent builds can use `--locked`. Both builds
use `CARGO_INCREMENTAL=0`, opt-level 1/debug 0, and actual source path dependencies.

With a private X11 display (`DISPLAY=:96` here), software OpenGL, no saved session:

```sh
DISPLAY=:96 WGPU_BACKEND=gl /tmp/serein-8k-before/native-preview --demo --page=voice --video-backend=experimental --video-codec=h265 --capabilities=hardware --width=1120 --height=760 --scroll=100000 --output=/tmp/camera-before.png
DISPLAY=:96 WGPU_BACKEND=gl /tmp/serein-8k-after/native-preview --demo --page=voice --video-backend=experimental --video-codec=h265 --camera-resolution=8k --camera-fps=60 --capabilities=hardware --width=1120 --height=760 --scroll=100000 --output=/tmp/camera-after.png
DISPLAY=:96 WGPU_BACKEND=gl /tmp/serein-8k-before/native-preview --demo --page=screen-share --width=1120 --height=760 --output=/tmp/screen-before.png
DISPLAY=:96 WGPU_BACKEND=gl /tmp/serein-8k-after/native-preview --demo --page=screen-share --width=1120 --height=760 --output=/tmp/screen-after.png
```

Add `--width=760 --height=900 --light --zoom=1.25` for the narrow camera case;
use `--zoom=1.5` for the narrow screen case. Screen body content below the visible
quality presets is reachable by scrolling; its footer does not scroll away.

## Verification

See `verification.json` for actual outcomes. No test opened a physical camera,
microphone, screen picker/capture session, keyring or Discord account.

- Actual UI Cargo suite passed 428 tests (5 ignored). Resolution and rate controls
  are exercised on both backends at a 320-point content width. The final added
  keyboard assertions passed separately: Tab/Enter selects 30 and 60 fps.
- Fresh real Linux voice tests cover native format/rate ranking, jitter-tolerant
  cadence, output/preview geometry, encoder configuration, signaling, RTP pacing,
  raw/encoded allocation budgets and existing media behavior. The bounded camera
  channel wakes transport independently of the 20 ms audio tick, removing its
  50 fps consumption ceiling. The cadence test
  runs ten synthetic seconds at each selected rate with jitter, plus faster input,
  duplicates and stalls, without sleeping or opening a capture source.
- A real software FFmpeg H.264 encode/decode passed at 2560×1440, above the removed
  1080p output limit. A real 720p camera encode/decode kept its UI preview 640×360.
  8K axis and allocation tests exercise bounded geometry without retaining huge
  default-test fixtures. They are not successful physical 8K/60 fps measurements.
- Saved preferences round-trip all 54 codec/resolution/rate combinations, migrate
  absent camera fields to 480p/15 fps, reject unknown resolution/fps, and retain
  backend/codec validation. Focused core screen-preset bounds also pass.
- Strict scoped Clippy covers model/core/store, UI, actual voice library/tests and
  the offline camera-format example. Workspace/fuzz formatting and diff checks pass.
- Full `cargo xtask check` and standard `cargo xtask package` attempts stop at
  missing `glib-2.0.pc`. Native Windows/macOS adapter compilation, full desktop
  wiring, full workspace checks and standard packages require the new fork CI.

The fresh native tests use the existing
[`build-voice-tests.py`](../video-hardware-capabilities/build-voice-tests.py)
workflow copied unchanged into task scratch. It compiles actual current `build.rs`,
fresh C/C++ shims and the real Rust voice crate, using exact cached dependency
fingerprints and real FFmpeg/OpenH264/GStreamer runtimes. `native-build-metadata.json`
records source/artifact SHA-256 identities. This isolation avoids rebuilding unavailable
system development crates; it does not establish a complete production build.

## Performance and resource limits

`measurements.json` preserves the one matched idle UI sample pair, raw RSS samples,
commands, executable hashes, renderer, CPU/RAM and sampling method. Reproduce with
[`sample-native.py`](../video-driver-detection/sample-native.py), using the same
1120×760 default-Inter voice fixture, default camera preferences and no encoding.
Six-second warmup, 100 wheel-down events, pointer moved out, fifteen one-second
samples per revision, no children or compiler work during sampling.

The changed helper used 1,769,472 more sampled peak/settled RSS bytes (1.6875 MiB,
about 1.09%), zero measurable idle CPU in both samples, and 57,904 more executable
bytes. One short opt-level-1 pair is not a standard-release benchmark or proof of
a regression/improvement. Standard executable/installed/compressed-package deltas
remain unmeasured locally because GLib development metadata is absent. See
[`performance.md`](../../performance.md#8k-presets-and-camera-frame-rate-controls--october-6-2026).

Selected 8K output deliberately costs more native/worker memory: one packed BGRA
picture can be 132,710,400 bytes, RGB 99,532,800 bytes, and I420 49,766,400 bytes,
plus separately owned capture/codec surfaces. Native camera allocation budgets
follow negotiated geometry and bounded padding, beneath the global 8K ceiling.
Camera UI previews stay at most 640×480 RGB; screen previews stay 640×360 RGBA.
The default camera's encoded cap remains 128 KiB, rising by resolution to 2 MiB.
Queues retain their bounded item/byte policies. These bounds are not total RSS.

## Remaining limits

Physical NVENC/AMF/QSV/VideoToolbox execution, native 8K/60 fps capture and sustained
encoding/viewing are unverified. The optional encoder test retains its fixed
640×480/15 fps camera and 1280×720/30 fps screen presets; it does not validate a
selected 8K/60 fps configuration. Incoming playback remains H.264 with a 1080p
cap. Official-client codec/resolution/rate acceptance needs owner-controlled live
validation. No active-media CPU, GPU memory, frame/startup timing or sustained
leak measurement is claimed.
