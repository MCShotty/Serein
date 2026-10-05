# Hardware codec detection evidence

Base: `a08d5a7b6fef26959166708b690378bfd1e3da6c` (fork main). October 5, 2026.
No account, Discord session, camera, microphone or screen capture was used.

`before.png` and `after.png` are inspected native dark framebuffer captures at
1120×760, 100%, Voice & Video scrolled to the bottom, Experimental/H.265.
The new screenshot uses an **explicit synthetic** matrix: camera NVENC for
H.264/H.265, screen NVENC+AMF, and AMF AV1. It does not show this host's hardware.
The narrow light pair is 760×900 at 150%; the new negative fixture demonstrates a
preserved saved H.265 selection and the H.264 fallback warning. A 760×520 window
at 150% has a pre-existing collapsed content area on both revisions; it is not used
as evidence that the new controls are visible.

Prepare the isolated preview against the requested checkout:

```sh
python3 docs/pr-evidence/video-hardware-capabilities/prepare-preview.py /tmp/cap-preview /path/to/checkout
cargo build --locked --manifest-path /tmp/cap-preview/Cargo.toml
# Use an owned Xvfb display or native desktop; launch only --demo.
WGPU_BACKEND=gl /tmp/cap-preview/target/debug/serein-ui-task-preview --demo --page=voice \
  --video-backend=experimental --video-codec=h265 --capabilities=hardware \
  --width=1120 --height=760 --scroll=100000 --output=/tmp/after.png
# Negative: --capabilities=none --width=760 --height=900 --zoom=1.5 --light
# Other fixture modes: checking / unknown. Base has no capabilities fixture.
```

The preview uses the actual UI crate and the existing native example, without
platform service adapters. `capability-fixture.rs` is injected only into that
scratch preview. No preview fixture was added to production capability detection.
`sample-native.py` requires psutil, X11/XTest and an isolated owned DISPLAY. It
launches --demo, sends 100 scroll-down events, moves the pointer out, warms up for
six seconds, then samples RSS and process CPU every second for 15 seconds.

`measurements.json` preserves the single baseline/after sample and six real
headless scans (one warmup, five measured). These previews use opt-level 1/debug 0;
baseline Cargo incremental defaults and after incremental disabled for disk use.
They are auxiliary observations, not release-performance evidence. No memory or
CPU improvement is claimed. Standard release executable, installed package and
compressed size comparisons are unavailable because native development metadata
is missing, as recorded in `package.log` and `xtask-check.log`.

Verification used fresh current Rust source and a freshly compiled real C shim,
linked to the pinned FFmpeg 7.1.5 prefix and installed native runtime libraries.
Exact cached dependency identities and source/artifact hashes are recorded in
`native-*.json`, `verification.json` and the direct-build command files. The
scratch scripts preserve this host's commands/paths; they require those caches
and extracted runtime libraries and are not portable substitutes for Cargo.
`scanner.rs` imports the actual desktop detector. For its standalone build,
`build-scanner.py` re-exports the **real matching egui Context type** under the
eframe name to avoid unrelated native-window dependency/proc-macro collisions
between independent artifact caches. No encoder, driver or detector behavior
is mocked in the real scan. This does not verify the full desktop binary.

- UI: 422 passed, 5 ignored; includes real Tab/Enter, disabled/unknown codecs,
  pending/recheck states and wrapped text at 220/320/600 points and 100%/150%.
- Desktop detector: 7 passed, including invalid helper arguments/status,
  deadlines, cancellation, demo settling and session teardown. Strict Clippy
  passed for the UI tests, detector tests and current native voice tests.
- Voice: 97 passed, 4 ignored on both host OpenH264 2.6 and Ubuntu OpenH264 2.4
  (the latter serial). Its initial parallel run had a timing-sensitive existing
  stride fixture failure during concurrent compilation; the isolated and serial
  runs passed. `voice-ubuntu-parallel-initial.log` retains that failed run.
- Real host scan:all nine NVENC/AMF/QSV×codec pairs unavailable, no `/dev/dri`.
  Median 265 ms; maximum one simultaneous child, none retained after completion.
  Peak RSS observations are sampled, not a hard process/GPU-memory ceiling.
- Workspace/fuzz formatting and diff checks passed. `cargo xtask check` and
  the standard package attempt stopped at missing `glib-2.0.pc`; whole-workspace
  tests/Clippy and a fresh full release package remain unverified here.

Positive vendor-hardware results, Windows/macOS runtime behavior, larger stream
presets, concurrent GPU sessions and live peer negotiation remain unverified.
GPU capability never substitutes for transport negotiation.
