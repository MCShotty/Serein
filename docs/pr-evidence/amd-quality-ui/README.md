# AMD quality and responsive native controls

UI baseline: `be644d9064e0b53caf6db103037c8a48a5c7baa8`. Screenshots use actual
native eframe/wgpu framebuffers and synthetic fixtures, with the same arguments
before and after. `voice-bottom-after.png` additionally demonstrates the relocated
encoding controls at the bottom of the page.

The environment lacks GTK4/WebKit6 libraries needed by the desktop platform crate.
The isolated preview uses the unchanged desktop `profile_preview.rs` source and
the real UI crate, fixes tray/startup availability to false, and adds two fixture
flags: `--zoom` and `--reply` (a long synthetic Unicode author). It has no service,
microphone or capture adapters. These are development fixtures, not production
application changes or live Discord sessions.

Prepare and build each revision separately, passing its checkout explicitly:

```bash
python3 docs/pr-evidence/amd-quality-ui/prepare-preview.py /tmp/serein-preview /path/to/checkout
cargo build --offline --manifest-path /tmp/serein-preview/Cargo.toml
```

Run under an X display with `WGPU_BACKEND=gl`. Replace `PREVIEW` with that revision's
freshly built binary, and choose a different output path for each revision:

```bash
PREVIEW=/tmp/serein-preview/target/debug/serein-ui-task-preview
"$PREVIEW" --demo --page=voice --video-backend=experimental --video-codec=h265 \
  --width=1120 --height=760 --output=/tmp/voice.png
"$PREVIEW" --demo --page=voice --video-backend=experimental --video-codec=h265 \
  --width=1120 --height=760 --scroll=100000 --output=/tmp/voice-bottom.png
"$PREVIEW" --demo --page=voice --width=760 --height=520 --zoom=1.5 --light \
  --output=/tmp/settings.png
"$PREVIEW" --demo --page=friends --tab=add --width=760 --height=520 --zoom=1.5 \
  --scroll=100000 --output=/tmp/friends.png
"$PREVIEW" --demo --page=markdown --reply --width=760 --height=520 \
  --output=/tmp/reply.png
python3 docs/pr-evidence/amd-quality-ui/sample-native.py "$PREVIEW" REVISION /tmp/native.json
```

The sampler requires `psutil`. Run it after compilation stops: three seconds of
warmup, then 15 one-second RSS samples and aggregate process CPU time. There is
one run per revision. `measurements.json` records runtime source hashes and raw
samples. Tiny differences are noise; this is a development preview comparison,
not release performance, startup/frame latency or a memory leak soak test.

The existing `video-normal-modes/encoder-check.c` is compiled with
`NORMAL=1, HEVC_MAX_QUALITY=1` for the new AMF policy. It resolves actual FFmpeg
options across 20 Linux configurations, exercises the software ABI, and checks
forced keyframes and malformed input. Both 330-frame H.264 streams decode without
errors. It never initializes physical GPUs; AMF quality, throughput and driver
compatibility remain unmeasured. See `docs/performance.md` for the compiler command.
