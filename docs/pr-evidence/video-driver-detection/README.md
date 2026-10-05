# Driver queries and optional encoder tests

Baseline: `16a09a986b73c7bf6abc3a80559ecdefb919c84a`, the existing upstream
feature PR head. Captured October 6, 2026. No account, Discord connection,
camera, microphone or screen capture was accessed.

The captures use the actual native UI crate and the existing offline eframe
example with an explicitly synthetic hardware fixture. They verify presentation,
not the hardware on this host or successful Discord negotiation. The default
Inter font is the same in both revisions.

- `before.png` and `after.png`: dark, 1120×760, 100%, Voice & Video scrolled to
  the bottom, Experimental/H.265. Before shows the automatic encoder scan's
  camera/screen results. After shows separate driver-reported codec support and
  the optional H.265 test with both source presets still **Not tested**.
- `encoder-test-after.png`: the same view with explicitly seeded successful
  H.265 test results. These results do not alter the driver report.
- `unknown-after.png`: unknown driver results leave H.265 selectable and the
  optional test unrun.
- `narrow-light-before.png` and `narrow-light-after.png`: light, 760×900,
  150%, negative fixtures with a preserved saved H.265 selection. The warning,
  optional action and explanatory text wrap within the scrolled settings body.
  Content above the viewport is reachable by scrolling.

All images were inspected after export. Each is below 2 MiB. The fixture reports
NVENC and AMF for H.264/H.265 and AMF for AV1. The pre-change scan fixture has
camera success only for NVENC on H.264/H.265 and screen success for both;
the optional H.265 test preserves those source-specific outcomes. Driver queries
report codec support without those preset distinctions. None of these positive
rows came from a physical GPU.

Prepare the isolated helper using this script and the desired checkout:

```sh
python3 docs/pr-evidence/video-driver-detection/prepare-preview.py /tmp/driver-preview /path/to/checkout
cargo build --locked --manifest-path /tmp/driver-preview/Cargo.toml
# Use an owned native display or Xvfb; always pass --demo.
WGPU_BACKEND=gl /tmp/driver-preview/target/debug/serein-ui-task-preview \
  --demo --page=voice --video-backend=experimental --video-codec=h265 \
  --capabilities=hardware --width=1120 --height=760 --scroll=100000 \
  --output=/tmp/after.png
# Optional-test fixture: --encoder-test=passed, failed, running or not-tested.
# Driver fixture: --capabilities=hardware, none, unknown or checking.
# Narrow negative: --capabilities=none --width=760 --height=900 --zoom=1.5 --light.
```

`prepare-preview.py` injects `before-fixture.rs` when the requested checkout has
the old model, otherwise `driver-fixture.rs`. These files are development
evidence only and are not imported by production capability detection.

This host's captures were freshly compiled from each revision's actual model,
session cache, core, extension, protocol, test support and UI sources. Direct
Rust compiler invocations reused one exact Cargo fingerprint dependency graph
because a full local native build is blocked by missing development metadata.
They used Rust 1.98.1, opt-level 1, debug information disabled and 16 codegen
units. The third-party libraries are real, unchanged cached artifacts. Build
commands remain in the task's scratch outputs; `build-identities.json` records
the full source/artifact hashes and `measurements.json` records the workloads.
This is an
auxiliary native helper, not a standard release package or executable test of
the full desktop application.

`sample-native.py` requires psutil, X11/XTest and an owned `DISPLAY`. It starts
the helper with `--demo`, sends 100 wheel-down events into settings, moves the
pointer out, warms up for six seconds in total, then samples process RSS and CPU
every second for 15 seconds. No encoder test or driver query is executed by this
fixture. Child processes are reported separately. A single before/after pair
does not establish a performance improvement or regression; renderer, font
atlas and allocator effects remain part of these observations.

`measure-scans.py` additionally compares freshly built wrappers around the
actual desktop detector and real native voice libraries. The baseline uses
the exact PR head's automatic synthetic encoder scan; after uses driver
queries. Each revision gets one warmup and five measured fresh processes,
alternated sequentially. Both link the same pinned FFmpeg 7.1.5 and OpenH264
2.6.0 runtime. Detector wrappers use opt-level 1/debug 0 and real egui context
types; voice libraries and C shims use opt-level 3. Wrapper/native source,
artifact and runtime-library hashes are in `scan-build-identities.json`.
No driver or encoder behavior is mocked in this measurement.

The host has no GPU device nodes or discoverable CUDA/AMF/Intel hardware
drivers. All nine NVENC/AMF/QSV × codec paths are **Unavailable** in all
twelve runs; the after wrapper asserts that no encoder-test report starts
during discovery. Median process elapsed time is 262.167 → 265.354 ms
(+3.187 ms, +1.216%). At 1 ms polling, the maximum observed simultaneous
helper count is one and no observed helper remains alive after exit. The
25 ms detector polling cadence and fast negative driver path dominate these
samples. They establish neither successful hardware throughput nor an
improvement/regression on a machine with a usable GPU; sampled RSS peaks are
not a process/GPU memory ceiling.

`cargo xtask check` and the standard release package attempt stop at missing
`glib-2.0.pc` development metadata on this host. Fresh full-desktop execution
and standard executable/installed/compressed package deltas remain unmeasured.
The auxiliary helper size in the raw data is explicitly not a package size.
