# Performance findings

Recent synthetic/offline measurements are workload-specific. They do not establish live Discord
performance, universal device results or application-wide memory bounds. The older upstream PR screenshot,
log and per-run evidence archive has been removed; the summaries below retain the useful results.

## Voice default-device polling — October 7, 2026

An offline probe compared creating a fresh PulseAudio client for every metadata poll with reusing
one client. Across five measured runs after warmup, 300 polls created 300 clients and left 300 peer
sockets connected in the fresh-client pattern; reuse created one client and left one socket. The
probe used synthetic socket pairs and a 2 ms pause per query. This identifies dependency resource
retention in the polling pattern; it does not establish application socket behavior or whole-process
memory use.

Both standard voice-enabled macOS packages were the same size: executable 68,180,992 B, installed
app 74,201,013 B, distribution 74,265,778 B. ZIP sizes differed by 37 B (48,039,907 vs 48,039,944 B),
which is packaging variation, not a runtime change. No Discord call, microphone, physical device,
callback latency, frame timing or CPU/RSS comparison was measured.

## CPU and memory audit — October 2, 2026

Release workloads compared a 100,000-event synthetic timeline, cursor scans, and synthetic video
frame ownership on an Apple M1 with Rust 1.98.1:

| Workload | Before | After | Result |
| --- | ---: | ---: | ---: |
| Reducer replay median | 153.985 ms | 53.504 ms | −65.25% |
| Full-history cursor query batch | 263.428 ms | 6.948 ms | −97.36% |
| Cursor batch with deleted tail rows | 341.132 ms | 16.075 ms | −95.29% |
| Coalesced 1080p frame peak RSS | 34.406 MiB | 26.516 MiB | −22.93% |
| Standard executable | 62,006,096 B | 62,006,112 B | +16 B |
| Installed bundle | 68,016,525 B | 68,016,541 B | +16 B |

The reducer retained 500 rows in both builds. Cursor timings isolate hot lookup work and do not
predict whole-UI gains. Frame conversion time was effectively unchanged; the lower peak RSS came
from reusing the undisplayed pending frame. With uploads every third frame, peak RSS was unchanged.

A focused native idle sample showed mean CPU of 1.180% before and 1.275% after, and settled RSS of
108.766 MiB and 111.484 MiB. This small, noisy sample supports no idle-performance improvement
claim. Startup latency, full-frame p95, GPU memory, live traffic and other platforms were unmeasured.

These workloads can be repeated with the pinned toolchain using `cargo replay` for the reducer and
the existing ignored client-core, desktop-frame and replay-soak workloads for detailed memory work.
The delivery skill documents how to compare a task baseline with the changed build. Do not compare
results from different machines or claim live-client behavior from synthetic fixtures.

## FFmpeg camera and screen encoding — October 4, 2026

Baseline `6313e271e9c34b39069eb20787afb1476741b21c` is compared with the
runtime source hashes in [the measurements](pr-evidence/ffmpeg-video-encoding/measurements.json).
Debian 13 / Linux x86_64 container, Intel Xeon Platinum 8573C, five visible
logical CPUs, 18,882,699,264 B RAM, GStreamer 1.26.2, pinned Rust 1.98.1 and
locked dependencies. No build ran during measurement. All inputs are synthetic;
no account, camera, microphone or native desktop capture was used.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| 300 camera frames, median of five release runs | 1,271.643 ms | 1,061.770 ms | −209.873 ms / −16.50% |
| Native preview CPU, mean of 40 one-second samples, one core | 371.618% | 384.722% | +13.105 percentage points |
| Native preview sampled peak RSS, maximum over two launches | 204,468,224 B | 202,825,728 B | -1,642,496 B / -0.80% |
| Native preview settled RSS, median over two launches | 204,244,992 B | 202,770,432 B | -1,474,560 B / -0.72% |
| Standard voice-inclusive executable | 85,705,128 B | 85,702,192 B | −2,936 B / −0.0034% |
| Installed regular files | 90,219,249 B | 104,848,408 B | +14,629,159 B / +16.22% |
| Complete compressed DEB | 44,277,080 B | 56,252,124 B | +11,975,044 B / +27.05% |

The camera fixture calls the actual `CameraEncoder` on a deterministic 640×480
RGB gradient, including allocation, conversion and H.264 encoding. Both target
600 kbit/s with independent IDRs and a 128-KiB packet cap. The same test-only
function was added to the detached baseline after its production package was
built. Each process warms 30 frames and times 300; one process warmup precedes
five alternating measured pairs. Baseline elapsed range: 1,213.192–1,327.404 ms;
after: 1,048.393–1,075.687 ms. Each emits 300 packets (5,952,600 vs 5,831,668 B).
The old camera uses one OpenH264 thread; FFmpeg uses two and different wrapper
defaults. This is a fixture result, not a quality-matched or hardware speed claim.
Component-process RSS is also sampled every 10 ms in the raw evidence; those
samples can miss brief peaks and do not describe whole-application memory.

Native sampling uses the existing offline `profile_preview --demo --interactive
--page=appearance` at 1120×760, scale 1, dark, isolated Xvfb/X11 with forced Mesa
lavapipe Vulkan and its mapped driver verified. Both release examples use
`--no-default-features --features demo` and final-example `-C lto=off`. After eight
seconds, move the pointer to (1100,740), settle three seconds, then sample twenty
one-second CPU/RSS intervals. Two fresh launches per revision reverse the order;
settled RSS is each launch's last-five-sample median. No helper children appeared.

The software renderer consumes several cores in this fixture. Per-launch mean
CPU pairs are 356.130→385.475%, then 387.105→383.970%; the direction reverses.
A thread sample found five `llvmpipe` workers doing most of the CPU work. These
fixtures use no media adapters, and the UI implementation is unchanged. This
small launch set does not isolate encoding-related idle cost or establish an
application performance improvement. Small RSS differences also include driver
and allocator variation. Startup/frame p95, GPU memory, active screen conversion,
hardware encoding and live-call latency remain unmeasured.

Both standard `cargo xtask package` DEBs pass smoke and shared-library closure
checks. They use normal fat LTO, one codegen unit, stripped executables and no
default/demo features. Installed bytes sum regular extracted files, excluding
symlinks and allocation overhead. The increase includes three replaceable codec
libraries, complete FFmpeg source, exact build recipe/namespace patch and notices;
there are 225 baseline files and 239 after. The extracted new package resolves
its private libraries under `usr/lib/serein` through relative RPATH before the
development-prefix fallback.

The measured Linux package retains its exact builder source/provenance. A final
Windows ARM64-only assembler flag repair leaves Linux configure options and
native outputs unchanged; it is verified by the six packaging tests. Metadata
was not restamped. Future native builds use a fresh prefix for the new recipe.
The [evidence README](pr-evidence/ffmpeg-video-encoding/README.md) records commands,
checks and limitations. Full `cargo xtask check` passes (1,162 passing test
executions, 26 ignored). ASan/UBSan ABI, independent H.264 decode, synthetic Linux
screen/readiness/pressure and offline packaging regressions pass. NVENC,
VideoToolbox and native Windows/macOS CI remain unverified locally.

Outgoing encoding bounds remain 128 KiB per camera packet and 2 MiB per screen
packet, with at most four pending native packets. The encoder uses two camera
threads or four screen threads, no B-frames/lookahead and a bounded single latest
raw screen frame. These are component limits, not an application-wide RAM cap.

## FFmpeg AMD AMF and Intel Quick Sync — October 5, 2026

The preserved, verified initial FFmpeg migration `b4c32bb98edbe16e07b60bf8b2d2a138eb536e88`
is compared with the vendor extension's final source/binary hashes in
[the raw measurements](pr-evidence/ffmpeg-vendor-encoding/measurements.json).
The original project baseline `6313e27` and initial migration measurements above
remain historical evidence. This comparison adds AMD AMF and Intel QSV while
retaining NVENC/VideoToolbox and software fallback. Linux QSV uses a VA driver
device; `h264_vaapi` encoding remains excluded.

Debian 13 / Linux x86_64 Docker, Intel Xeon Platinum 8573C, five visible logical
CPUs, 18,882,699,264 B RAM, GStreamer 1.26.2, pinned Rust 1.98.1 and locked
dependencies. Compiler work was serialized and stopped before measurement.
No GPU device, account, camera, microphone or native desktop capture was used.

| Metric / method | FFmpeg baseline | With AMD/Intel | Delta |
| --- | ---: | ---: | ---: |
| 300 camera frames, median of five release runs | 986.725 ms | 979.601 ms | −7.124 ms / −0.72% |
| Native preview CPU, mean of 40 one-second samples, one core | 347.475% | 336.945% | −10.530 percentage points |
| Native preview sampled peak RSS, maximum over two launches | 202,584,064 B | 201,699,328 B | −884,736 B / −0.44% |
| Native preview settled RSS, median over two launches | 201,234,432 B | 201,469,952 B | +235,520 B / +0.12% |
| Standard voice-inclusive executable | 85,702,192 B | 85,704,880 B | +2,688 B / +0.0031% |
| Installed regular files | 104,848,408 B | 120,678,267 B | +15,829,859 B / +15.10% |
| Complete compressed DEB | 56,252,124 B | 70,674,624 B | +14,422,500 B / +25.64% |

The actual `CameraEncoder` workload includes RGB allocation/conversion and
encoding of a deterministic 640×480 gradient. Each process warms 30 frames,
then times 300. One process warmup precedes five alternating measured pairs.
Baseline range is 967.395–1,005.343 ms; after is 973.157–1,021.608 ms. Both
produce 300 independent packets and exactly 5,831,668 encoded bytes per run.
The ranges overlap; no software speed improvement is claimed. Camera-process
RSS sampled every 10 ms is retained in the raw data and can miss brief peaks.
Both revisions use the same software codec/tuning on this machine; hardware
speed, quality and active screen conversion remain unmeasured.

The native fixture uses `profile_preview --demo --interactive --page=appearance`
at 1120×760, dark, scale 1, isolated Xvfb :88 and forced Mesa lavapipe Vulkan,
with its mapped driver verified. Release builds use no default features, demo
and final-example `-C lto=off`. Eight seconds of warmup precede a pointer move
to (1100,740), three seconds of settling and twenty one-second CPU/RSS samples.
Two fresh pairs reverse order. Per-launch baseline/after mean CPU is
356.775→344.280%, then 338.175→329.610%. There are no helper children.
Settled RSS is each launch's last-five-sample median. The preview has no media
adapters and consumes several cores in software rendering; this small sample
does not establish encoder-related or production-idle CPU/RAM improvement.
Small RSS differences include allocator/driver variation. Startup/full-frame
p95 latency, GPU memory, AMF IDR reset cost and live calls remain unmeasured.

The standard package passes Debian smoke and host shared-library closure.
Normal fat LTO, one codegen unit, stripping and no default/demo features are
unchanged. Installed bytes sum regular extracted files, excluding symlinks and
allocation overhead (239→248 files). The extracted executable resolves all
three private codec libraries from `usr/lib/serein` before its development-prefix
fallback. The final DEB includes 25,923,241 bytes of source archives, three
replaceable codec libraries and notices. The source-built static oneVPL
dispatcher, exact AMF public headers and OpenH264 rebuild source explain most
of the package increase. The OpenH264 subset omits only root test media and
retains every source/build/license entry and file mode. Compared with the
original project package, final executable/installed/DEB sizes are
85,704,880 / 120,678,267 / 70,674,624 B, versus
85,705,128 / 90,219,249 / 44,277,080 B; package growth is the main tradeoff.

The delivery prefix and shipped builder match recipe SHA-256
`5daeee263ee108de8958e315dc55ed3777cfb027d86c5e92f45bc3ee3a3e5be2`.
Full `cargo xtask check` passes (1,166 test executions, 26 ignored).
Strict C11 ASan/UBSan/leak ABI, NV12/canary/packet bounds, actual native option
compatibility for six hardware profile combinations, independent decode and
both synthetic Linux screen variants pass. FFmpeg native/package checks pass
10 tests with the Windows-only import check skipped. Debian, four Flatpak
preparation tests and the synthetic signing fixture pass. See the
[evidence README](pr-evidence/ffmpeg-vendor-encoding/README.md) for reproduction.
Physical GPUs, native Windows/macOS, actual Nix/Flatpak and other Linux package
formats remain unverified. GPU drivers are not redistributed.

Component caps remain 128 KiB per camera packet, 2 MiB per screen packet and
four pending native pictures. QSV packs directly into owned NV12 storage and
its patched packet path uses the capped allocator before allocation/copying.
Returned packets must fit their actual backing allocation before inspection.
Failed backends remain excluded through bitrate restarts. AMF Main-profile IDR
requests reopen the context only after output has started; pending startup
output can drain and camera GOP 1 avoids a per-frame restart. SDK startup,
polling and shutdown can still block; bounded queues do not bound native call
time or remove the worker retirement barrier.


## Video backend and codec settings — October 5, 2026

Compare the verified local starting commit `65811f20a7dea160ba4c78459c63dd339a794a5a`
(the AMD/Intel FFmpeg stage) with Stable/Experimental and codec selection. Remote
base remains `6313e27`; this comparison measures the latest settings/codec stage,
not the entire branch against original main. Baseline package, native preview and
camera test executable were preserved before any new Cargo build. All work is
synthetic/offline, with no physical media or account session. Raw samples and
identities are in [video settings evidence](pr-evidence/video-backend-settings/README.md)
and its [measurements.json](pr-evidence/video-backend-settings/measurements.json).

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Standard executable, bytes | 85,704,880 | 85,746,216 | 41,336 / +0.05% |
| Installed regular files, bytes | 120,678,267 | 120,976,132 | 297,865 / +0.25% |
| Compressed DEB, bytes | 70,674,624 | 70,795,724 | 121,100 / +0.17% |
| FFmpeg software, 300 camera frames | 993.070 ms | 1,024.365 ms | 31.295 ms / +3.15% |
| Default: FFmpeg → Stable software, 300 frames | 1,070.348 ms | 961.377 ms | -108.971 ms / -10.18% |
| FFmpeg camera sampled peak RSS, median | 19.855 MiB | 20.391 MiB | 0.535 MiB / +2.70% |
| Default camera sampled peak RSS, median | 19.957 MiB | 22.145 MiB | 2.188 MiB / +10.96% |
| Voice & Video idle mean CPU, one core | 0.000% | 0.000% | 0.000 percentage points; below sampling resolution |
| Idle sampled peak RSS | 194.012 MiB | 193.664 MiB | -0.348 MiB / -0.18% |
| Idle settled RSS | 193.578 MiB | 193.082 MiB | -0.496 MiB / -0.26% |

Debian 13.7, Linux 6.18.44, Intel Xeon Platinum 8573C, five visible logical CPUs,
18,882,699,264 bytes RAM, Rust 1.98.1 and locked dependencies. Standard voice-enabled
`cargo xtask package` uses fat LTO, one codegen unit and stripping; the 248-file DEB
smoke and source/library/notice identity checks passed. Installed bytes sum regular
files, excluding symlink/allocation overhead. The native prefix and shipped recipe
match SHA-256 `3e6f390ecf333833313c7aaee760cd53ade18d741f535b17bdc32ee1d7538c59`.

Camera uses unchanged deterministic 640×480 RGB data, 30 warmup frames and
300 timed conversion/allocation/encoding calls. Each comparison has one process
warmup and five alternating baseline/after pairs; RSS is sampled every 10 ms.
The FFmpeg pair ranges overlap (981.882..1120.076 → 1000.103..1079.431 ms); its
median is 3.15% higher, while all runs produce 300 packets/5,831,668 bytes. The
restored Stable default is measured separately: 978.513..1289.284 →
948.913..988.283 ms, 300 packets/5,830,468 after bytes. Its median is 10.18% lower
in that set but sampled peak test-process RSS is 2.188 MiB higher. Backend/thread
settings differ, visual quality is unmeasured, and baseline medians drifted
between sets; these observations do not establish general performance gains.
An initial camera set overlapped the synthetic Linux check and was excluded;
reported sets ran after all compilation and checks stopped.

Idle uses the release native preview (`--no-default-features --features demo`,
example link `-C lto=off`) at 1120×760, dark, scale 1, Xvfb :88 and verified Mesa
lavapipe Vulkan. Each launch warms eight seconds on Appearance, clicks Voice &
Video, moves the pointer outside controls, settles three seconds, then records
twenty one-second CPU/RSS samples. Two fresh pairs reverse launch order. CPU is
percent of one core and every value was below the sampling resolution; settled
RSS is the last five samples' median, aggregated across launches. Baseline peak
RSS runs are 194.012/193.145 MiB versus 192.500/193.664 after. The small aggregate
RSS difference is within variation; no idle improvement is claimed and no
children were observed. Startup/frame p95, GPU memory and active screen quality
are unmeasured.

`cargo xtask check` passed formatting, strict workspace/all-target Clippy, 1,183
passing test executions (27 ignored), no-default-features and policy. Native
strict C11/ASan/UBSan/buffer bounds, 18 codec hardware-option combinations without
devices, exact encoder-only bundle registry, both synthetic Linux screen variants
and package preparation checks passed. Hardware H.265/AV1 requires Experimental
and explicit codec negotiation; only H.264 has bundled software fallback and
incoming decoding remains H.264. Physical GPUs/capture, official-client live
compatibility (including AV1 DAVE framing), Windows/macOS, actual Nix/Flatpak
packages, dedicated license CI and remote checks remain unverified.

## Repository-wide lifecycle and state bug hunt — October 5, 2026

Compare the verified local starting commit `b6c32b6ddfa3bef63f01bc9b101de5aa20203017`
with the repository-wide bug-hunt fixes. The baseline and after binaries were built
from isolated worktrees with locked dependencies. Testing used synthetic offline data
and did not require an account, physical capture device or GPU. Raw samples, artifact
identities and environment details are in the
[bug-hunt measurements](pr-evidence/repository-bug-hunt/measurements.json).

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Standard executable, bytes | 85,746,216 | 85,796,648 | 50,432 / +0.059% |
| Installed regular files, bytes | 120,976,132 | 121,026,564 | 50,432 / +0.042% |
| Compressed DEB, bytes | 70,795,724 | 70,809,936 | 14,212 / +0.020% |
| Reducer replay median, 100,000 events | 79.729 ms | 80.958 ms | 1.229 ms / +1.54% |
| Appearance idle mean CPU, one core | 360.955% | 347.380% | -13.575 percentage points / -3.76% |
| Appearance idle sampled peak RSS | 192.594 MiB | 194.070 MiB | 1.477 MiB / +0.77% |

The environment was Debian 13.7 on Linux 6.18.44 x86_64, Intel Xeon Platinum
8573C, five visible logical CPUs, 18,882,699,264 bytes RAM and Rust 1.98.1.
The standard `cargo xtask package` build used the normal release profile and
passed the 248-file DEB smoke covering its executable, desktop entry, metadata,
ownership, content and host shared-library closure. Installed size sums regular
file content and excludes filesystem allocation and symlink overhead.

The reducer benchmark replays 100,000 deterministic gateway events into a fresh
process. After one warmup per revision, five baseline/after pairs alternated order.
Baseline samples ranged from 73.512 to 111.762 ms and after samples from 75.801 to
94.434 ms. Both revisions retained 500 records with an estimated timeline size of
331,992 to 332,477 bytes. The overlapping ranges and 1.54% median difference are
consistent with run-to-run noise, so no reducer regression or improvement is
claimed.

The native idle check used the release `profile_preview` example with demo data on
the Appearance page at 1120×760 under Xvfb and Mesa lavapipe Vulkan. Each revision
warmed for eight seconds, settled for three seconds after pointer movement, then
recorded twenty one-second CPU and RSS samples. Neither process spawned children.
After mean CPU was 13.575 percentage points lower and sampled RSS was 1,548,288
bytes higher. This single software-renderer pair does not establish a performance
change; frame timing, GPU memory and active media workloads remain unmeasured.

`cargo xtask check` passed the full workspace tests, strict all-target Clippy,
documentation, no-default-feature build and policy checks. Focused GStreamer,
voice, packaging and repository-script tests also passed, followed by the standard
package smoke. Native Windows/macOS builds, physical GPU and capture devices, live
accounts, live Wayland compositor events and extended soak testing remain
unverified.

## Encoder build and Windows update regression fixes — October 5, 2026

Compare the verified package from starting commit
`8cd15690c060f8d2ffde2f04c736ab300d6cba84` with the compiler, Flatpak dependency,
Windows migration and CI fixes. Artifact identities and local validation results
are in the [regression measurements](pr-evidence/encoder-regression-fixes/measurements.json).
The baseline is the retained, verified standard package from the preceding stage.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Standard executable, bytes | 85,796,648 | 85,796,648 | 0 / 0% |
| Installed regular files, bytes | 121,026,564 | 121,029,988 | 3,424 / +0.0028% |
| Compressed DEB, bytes | 70,809,936 | 70,810,788 | 852 / +0.0012% |

Both packages contain 248 regular files. Measurement uses one normal
voice-enabled `cargo xtask package` release build per revision, with locked
dependencies, fat LTO, one codegen unit and stripping. Installed bytes sum
regular-file sizes from `dpkg-deb --fsys-tarfile`, excluding symlinks and filesystem
allocation. The environment matches the preceding Debian 13.7 measurement.
The final package passed executable, metadata, ownership, content and host
shared-library closure checks. These tiny size differences establish no runtime
performance change.

`cargo xtask check` passed 1,235 test executions (27 ignored), strict all-target
Clippy, formatting, documentation, no-default-features and policy checks. Debug
symbols were disabled for the dev/test profiles to fit the workspace; the release
profile remained unchanged. The pinned FFmpeg compiler probe, real Linux bundle,
release smoke, Flatpak preparation and Windows archive fixtures passed. The exact
pinned patchelf source passed 54 upstream checks, with two skips.

These fixes change building and updater package selection, with no visible UI or
encoder algorithm change. CPU/RSS/frame timing were not remeasured. Full native
Windows/macOS and Nix/Flatpak validation requires CI. Physical GPU encoding,
capture devices, live interoperability and extended soak testing remain unverified.

## Normal video encoder modes — October 5, 2026

Compared application commit `f87aa5db9df9acb3eedbe288e2b2ef1c2a5d8740` with the
removal of explicit low-latency video modes. The
[C ABI workload](pr-evidence/video-normal-modes/encoder-check.c) compiles the real
FFmpeg shim and checks all 20 available Linux encoder/codec/profile option
configurations without opening GPUs. It also encodes camera and screen pictures
through OpenH264, checks camera independence, forced screen keyframes and malformed
input rejection. The configuration check does not establish working GPU encoding.

Environment: Debian 13 x86-64, Intel Xeon Platinum 8573C, five visible CPUs,
17 GiB RAM, pinned FFmpeg 7.1.5/OpenH264 2.6.0, GCC `-O2`. Software input is
640x480 limited-range BT.601 I420, 15 fps and 600 kbps. Each run warms up with 30
pictures then measures 300; one run per revision is discarded, followed by five
alternating baseline/after runs. RSS is sampled every 10 ms for the C process.
There is no renderer, display, audio device, desktop capture or network connection.
[Raw samples and bitstream hashes](pr-evidence/video-normal-modes/measurements.json)
record the workload and actual baseline revision.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Median 300 camera pictures | 891.491 ms | 860.949 ms | -30.542 ms / -3.43% |
| Median 300 screen pictures | 223.383 ms | 218.711 ms | -4.672 ms / -2.09% |
| Median sampled peak C-process RSS | 9,805,824 bytes | 9,891,840 bytes | +86,016 bytes / +0.88% |

The before/after bitstreams are byte-identical for both profiles. All 330 camera
and 330 screen pictures decode with `ffmpeg -v error -xerror -i <stream> -f null -`.
These small timing/RSS differences measure software noise, not hardware quality or
latency improvements. Physical GPUs are unavailable; GPU buffering, quality and
latency remain unmeasured. Native demo CPU/RSS and executable/installed/compressed
package sizes could not be measured: `cargo xtask check` and both baseline/after
`cargo xtask package` attempts fail on the inherited, read-only
`target/debug/.cargo-build-lock`. The environment also lacks GTK4/WebKit6 runtime
libraries and native development headers. A focused Cargo-only voice test rebuild
hits the existing `winit` platform-feature error. Formatting and the strict C
compiler checks pass; complete native validation remains a CI requirement.

To reproduce the C checks with the pinned FFmpeg prefix in `FFMPEG_DIR`:

```bash
cc -std=c11 -O2 -Wall -Wextra -Werror -DNORMAL=1 -DHEVC_MAX_QUALITY=1 \
  "-DSHIM=\"$PWD/crates/discord-voice/src/video_encode_ffmpeg.c\"" \
  -Icrates/discord-voice/src -I"$FFMPEG_DIR/include" \
  docs/pr-evidence/video-normal-modes/encoder-check.c \
  -L"$FFMPEG_DIR/lib" -Wl,-rpath,"$FFMPEG_DIR/lib" \
  -lavcodec-serein -lavutil-serein -o /tmp/serein-encoder-check
/tmp/serein-encoder-check /tmp/serein-camera.h264 /tmp/serein-screen.h264
```

For the baseline, extract its shim with `git show f87aa5d:crates/discord-voice/src/video_encode_ffmpeg.c`
into a temporary file, select that file with `SHIM`, and compile with `-DNORMAL=0`.
The current H.265 AMF policy is selected with `HEVC_MAX_QUALITY=1`; use `0` to
check the earlier all-balanced AMF policy from the initial normal-modes change.

## AMD quality and responsive native controls — October 5, 2026

UI baseline `be644d9064e0b53caf6db103037c8a48a5c7baa8` already contains the normal
encoder modes above. This change keeps AMF AV1 balanced and selects HEVC
`usage=high_quality, quality=quality`, while fixing responsive native controls
and moving video encoding below the camera settings. The pinned FFmpeg shim
passes its strict C compilation and 20 actual AVOption configurations. Both
330-picture software streams decode without errors. Physical GPU encoding,
driver compatibility, quality and throughput remain unverified; software checks
do not measure the HEVC preset's hardware cost.

[Raw samples and source hashes](pr-evidence/amd-quality-ui/measurements.json) and
[reproduction scripts](pr-evidence/amd-quality-ui/README.md) record a fresh native
before/after comparison. Environment: Debian 13 x86-64, Xeon Platinum 8573C,
five visible CPUs, 17 GiB RAM, Rust 1.98.1, eframe/wgpu under Xvfb with
`WGPU_BACKEND=gl` and Mesa software output. Both builds use the same isolated
native preview, `dev` profile with optimization level 1 and no debug info, actual
UI code, synthetic long-name reply fixture, dark appearance, 1120x760 pixels
and 100% zoom. Tray/startup availability is fixed to false; the helper contains
no service or capture adapters. These are development preview measurements,
not standard release/package measurements.

Compilation stopped before sampling. One run per revision warms up for three
seconds, then samples process RSS once per second for 15 seconds and computes
aggregate CPU time over that interval. Neither process had children.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Idle CPU, percentage of one core | 0.00% | 0.00% | Below process-timer resolution |
| Post-warmup sampled peak/settled RSS | 192,663,552 bytes | 191,758,336 bytes | -905,216 bytes / -0.47% |
| Isolated development preview executable | 83,176,272 bytes | 83,183,560 bytes | +7,288 bytes / +0.009% |

The small RSS difference is noise from one short paired sample; no memory or
speed improvement is claimed. Startup peak, frame/startup latency, sustained
load, leak soaks and GPU memory remain unmeasured. Native screenshots were
inspected at 1120x760/100% dark and 760x520/150% light/dark, including scrolling
to the encoding card and the last Add Friend action. The reply/edit regression
test also verifies that context controls leave the message input inside the
viewport. All 417 UI library tests pass (five benchmark tests ignored), and
strict UI Clippy across all targets passes.

The default workspace-check/package commands hit the inherited read-only debug
cache. Retrying the workspace check and standard release package in an owned
target directory reaches the native dependency build and fails because
`glib-sys` cannot find `glib-2.0.pc`.
The environment also lacks GTK4/WebKit6 development/runtime dependencies.
Standard release executable, installed-package and compressed-distribution
sizes remain unavailable; complete workspace/packaging validation is a CI
requirement. The development preview sizes above must not be treated as shipped
package sizes.

## Linux OpenH264 isolation verification — October 5, 2026

The upstream sync and PR #3 merge preparation exposed an Ubuntu GStreamer
2.4/OpenH264 2.6 ABI collision. The corrected native linkage passes the exact
crash reproduction and all 92 voice tests with each of the older Ubuntu and
newer Debian plugins (four intentionally ignored per suite). The current voice
crate, build script and C shim were compiled afresh using exact cached Rust
dependency artifacts and real native runtime libraries extracted into scratch.
[Build hashes and reproduction details](pr-evidence/openh264-isolation/README.md)
describe this local verification separately from a workspace Cargo build.

The fresh full Linux FFmpeg recipe passes all 20 encoder configuration checks;
both 330-frame software streams decode and are byte-identical to the previous
output. Executable-relative portable library staging and the Debian private
SONAME closure check pass. These are correctness checks. CPU/RSS, physical GPU
performance and standard release executable/installed/compressed package deltas
were not measured for this linkage fix. The current `cargo xtask check` attempt
still fails at missing `glib-2.0.pc`; native development prerequisites and full
application packaging remain CI requirements.


## Hardware codec detection — October 5, 2026

Base `a08d5a7b6fef26959166708b690378bfd1e3da6c`; raw current samples and limits in
[hardware capability evidence](pr-evidence/video-hardware-capabilities/README.md).
Debian 13 x86_64, kernel 6.18.44, five CPUs, 18,882,699,264 bytes RAM, Rust 1.98.1,
Xvfb/Mesa with WGPU GL. The isolated native UI uses synthetic --demo data,
1120×760 dark at 100%, six-second warmup and 15 one-second RSS samples after the
same settings scroll interaction. One pair only; opt-level 1/debug 0 previews,
baseline incremental defaults and after incremental disabled. These are auxiliary
observations; standard release/package comparisons remain blocked by missing
`glib-2.0.pc`, and no performance improvement is claimed.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Auxiliary preview settled/peak RSS | 148.043 MiB | 146.668 MiB | −1.375 MiB (no improvement claim)|
| Auxiliary preview idle CPU, one core | 0.000% | 0.067% | +0.067 percentage points (single short sample)|
| Auxiliary preview executable | 83,183,560 bytes | 84,268,632 bytes | +1,085,072 bytes; not the desktop package|
| Full release executable / installed / compressed package |Unavailable|Unavailable|Missing native development metadata|

A headless harness of the actual detector and real FFmpeg path ran one warmup
and five measured scans with synthetic pictures, without capture or a network
session. This host has no usable GPU devices: all nine backend/codec pairs were
unavailable. Median elapsed was 264.909 ms. Polling parent/child RSS every 1 ms gave
median sampled peaks 8.047 MiB parent and 4.957 MiB helper; short allocations may be
missed. There was at most one simultaneous helper, nine observed per scan and
none left after completion. UI previews had no children. This does not predict
positive GPU-driver session startup, encode throughput or GPU memory usage.
Full-desktop idle CPU/RSS, release/package deltas and frame/startup latency remain
unmeasured. Each backend/codec check has a 10-second subprocess deadline; a complete
Linux/Windows scan has at most nine sequential checks and a fixed 13-entry queue.

## Codebase bug hunt — October 5, 2026

Baseline `8a369cc3c71c18d97065dde8197c16d90970ed0a`; see
[reproduction and raw samples](pr-evidence/codebase-bug-hunt/README.md).
Debian 13 x86_64, kernel 6.18.44, five visible CPUs, 18,882,699,264 bytes RAM
and Rust 1.98.1. Standard release `replay-bench` binaries were rebuilt from each
revision. Each ran once for warmup and five measured times with alternating pair
order, without concurrent compilation or preview processes. Both replay 100,000
synthetic events and retain 331,992–332,477 estimated timeline bytes / 500 records.

The isolated native Appearance preview uses actual UI code with `ui/demo`,
opt-level 1/debug 0, Xvfb, eframe/wgpu and requested GL software rendering
(`WGPU_BACKEND=gl`, `LIBGL_ALWAYS_SOFTWARE=1`; exact adapter not instrumented).
Both use 1120 × 760 dark at 100% zoom, a six-second warmup including 100 scroll
events, then 15 one-second process RSS samples. Aggregate idle CPU is the
percentage of one core over that interval. Neither preview had children. The
new synthetic fixture requests Serif after session reset; baseline silently
retains Inter, while the fixed registry applies Serif.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Release reducer median, five runs | 83.116483 ms | 88.286294 ms | +5.169811 ms / +6.22% |
| Release replay executable | 1,510,296 bytes | 1,509,592 bytes | −704 bytes / −0.047% |
| Auxiliary UI sampled peak/settled RSS | 161,026,048 bytes | 161,624,064 bytes | +598,016 bytes / +0.37% |
| Auxiliary UI idle CPU, one core | 0.00% | 0.00% | Below process-timer resolution |
| Auxiliary UI executable | 89,526,776 bytes | 89,530,560 bytes | +3,784 bytes / +0.0042% |
| Full desktop release / installed / compressed package | Unavailable | Unavailable | Missing `glib-2.0.pc` |

Reducer ranges overlap widely: 80.75–123.18 ms before and 82.21–142.91 ms after.
The single short UI pair and five replay samples do not establish a performance
improvement or a statistically significant regression. Preview binary sizes
must not be treated as shipped desktop/package sizes. Frame/startup latency,
GPU memory, sustained-load/leak soaks and physical media-device cost remain
unmeasured. Actual voice tests pass with two Linux OpenH264 runtime versions;
those checks measure correctness, not encoding performance.

The thumbnail fix admits at most two jobs and 32 MiB of source bytes before
copying/spawning, retaining permits through actual decoder completion. Failed
placeholder attempts cap at 2,048; per-user video progress tables cap at 16;
tracked credential-removal IDs cap at eight. These are resource ceilings, not
process RSS or proof that all application leaks are absent. Standard workspace
and package checks were attempted and remain blocked at missing GLib development
metadata; the license-policy command also lacks `cargo-deny`.

### October 6, 2026 — driver discovery and optional encoder tests

Baseline is upstream feature-PR head `16a09a986b73c7bf6abc3a80559ecdefb919c84a`.
The change replaces automatic synthetic encodes with vendor driver codec queries,
adds a separate Test encoder action, and restarts cancelled automatic discovery
on reopening. It does not change active-stream encoding or capture presets.
[Raw measurements and build identities](pr-evidence/video-driver-detection/README.md)
record freshly rebuilt before/after source with pinned Rust 1.98.1 and real cached
dependencies. No account, camera, microphone or screen capture was accessed.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| No-GPU full scan median, one warmup/five alternating runs each | 262.166580 ms | 265.353937 ms | +3.187357 ms / +1.22% |
| Auxiliary settings helper sampled peak/settled RSS | 152,633,344 bytes | 152,551,424 bytes | −81,920 bytes / −0.054% |
| Auxiliary settings helper idle CPU, one core | 0.00% | 0.067% | Timer-resolution scale |
| Auxiliary settings helper executable | 88,546,592 bytes | 88,564,208 bytes | +17,616 bytes / +0.020% |
| Full desktop release / installed / compressed package | Unavailable | Unavailable | Local package build blocked by missing `glib-2.0.pc` |

The UI workload uses synthetic advertised support with tests unrun, the default
Inter font, 1120 × 760 dark at 100%, six seconds of warmup/scrolling and fifteen
one-second RSS/CPU samples. It runs an opt-level-1/debug-0 native helper, without
children or driver calls. The single pair does not establish a memory/CPU
improvement or regression and does not measure the shipped release package.

The separate headless detector workload uses real driver queries/encoder checks
and the same FFmpeg 7.1.5/OpenH264 2.6 prefix. This host has no usable GPU; all nine
vendor/codec paths were unavailable. The parent is sampled every millisecond;
all nine child PIDs were observed on each run, with at most one helper at a time
and no surviving helpers. Both parent and child RSS observations are sampled,
not memory ceilings. Similar scan times here mostly reflect process startup;
positive GPU query cost, encoding throughput, GPU memory and live interoperability
remain unmeasured.

Driver queries never initialize encoders or submit frames. SDK/API errors remain
Unknown. Only clicking Test encoder submits at most eight synthetic pictures per
camera/screen preset; a test failure does not change driver-reported support.
Every helper has a ten-second deadline, shared by both presets for a test.
Cancelling discovery drops stale messages and restarts on reopening; cancelling
a test leaves it waiting for an explicit new click. Full local workspace checks
also stop at missing GLib development metadata. Native platform CI remains the
standard build validation for this change.

## 8K presets and camera frame-rate controls — October 6, 2026

Baseline `623ef63514c1b68c516bc4b047503cead6b23511`, compared with the task's 8K
output presets and 15/30/60 fps camera controls. One matched auxiliary native UI
helper pair on Debian 13, Linux 6.18.44, Intel Xeon Platinum 8573C (five exposed
logical CPUs), 18,440,136 KiB configured RAM, pinned Rust 1.98.1, opt-level 1/debug 0,
Mesa Gallium 25.0.7/LLVM 19 software OpenGL. Actual UI/model/core crates use locked
cached dependencies; this helper excludes production GTK/native desktop adapters.
Both runs use default Inter, dark appearance, 1120×760 at 100%, Experimental/H.265,
synthetic driver rows, default camera settings, and no capture or encoding.

| Auxiliary native helper metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Idle CPU, one logical core | 0.000% | 0.000% | +0.000 percentage points |
| Sampled peak and settled RSS | 162,140,160 bytes | 163,909,632 bytes | +1,769,472 / +1.091% |
| Helper executable | 84,238,472 bytes | 84,296,376 bytes | +57,904 / +0.069% |

Six-second warmup, 100 wheel-down events in the settings body, pointer moved out,
then fifteen one-second `/proc` RSS samples per revision through psutil. CPU uses
process user+system time deltas, not total host CPU. No children were observed;
compiler work was stopped during each measurement. Peak and final RSS matched in
both runs. This single short pair shows a 1.688 MiB settings-rendering RSS
difference and no measurable idle CPU difference at the timer's resolution; it
establishes no general performance improvement or regression. Native warnings
about unavailable font caches/X11 SHM were the same nonfatal helper limitations.
Raw samples, executable hashes and reproduction are in
[`pr-evidence/video-8k-resolution`](pr-evidence/video-8k-resolution/README.md).

The standard `cargo xtask package` attempt stops at missing `glib-2.0.pc`; standard
executable, complete installed-package and compressed-distribution deltas for this
addition remain unmeasured locally. Earlier package tables describe earlier stages.
The helper sizes above are not package measurements. Physical 8K/60 fps capture,
GPU throughput/memory, full-frame/startup timings and sustained leak behavior remain
unmeasured.

Higher selected presets deliberately increase worker/native surface memory: an
8K packed BGRA picture is 132,710,400 bytes, RGB 99,532,800 bytes and I420 49,766,400
bytes, before separately owned codec/capture surfaces. Camera native byte budgets
follow negotiated geometry with at most 4096 padding bytes per row, within a global
150,405,120-byte ceiling. Encoded camera caps still start at 128 KiB for 480p and
scale only by resolution to at most 2 MiB; selected frame rate does not enlarge
that cap. Camera UI previews remain at most 640×480 RGB, screen previews 640×360
RGBA. These are component bounds, not a total RSS cap or measured active-media cost.


## Renderer GPU encoding and supported reordering — October 6, 2026

Compared source `79e336066beff5ea6c414268308f1f817c50c2f4` (camera GOP 1)
with Experimental camera Main profile/GOP 30 at 15 fps. Both software-only C
processes use the same pinned FFmpeg 7.1.5/OpenH264 2.6.0 libraries; hardware
presets, look-ahead and physical GPU selection are not exercised by this workload.
The [reproducer](pr-evidence/video-gpu-routing/measure-encoder.py) compiles each
revision's actual encoder shim with GCC `-O2`. Each run uses 30 warmup pictures
then 300 measured moving-gradient 640×480 limited-range BT.601 I420 pictures at
600 kbps. Elapsed time includes pixel preparation and bitstream writes. One
warmup run is discarded, then five alternating runs per revision are measured.
Both final 330-picture streams decode without errors with system FFmpeg.

Environment: Linux 6.18.44 x86-64, Intel Xeon Platinum 8573C, five visible CPUs
(four-core cgroup quota), 16 GiB memory ceiling. Process RSS is sampled every
10 ms. No renderer, GPU, camera, microphone, capture, account or network session
runs in this benchmark. No Cargo/native build ran during the final samples.

| Software-only workload / median | Before | After | Delta |
| --- | ---: | ---: | ---: |
| 300 camera pictures, elapsed | 1,115.033 ms | 546.520 ms | -568.513 ms / -50.99% |
| Encoded payload bytes for 300 pictures | 4,537,700 | 225,825 | -4,311,875 / -95.02% |
| Keyframes for 300 pictures | 300 | 10 | -290 |
| Sampled peak C-process RSS | 8,753,152 bytes | 8,740,864 bytes | -12,288 bytes / -0.14% |
| Actual Linux libavcodec shared library | 1,217,929 bytes | 1,258,993 bytes | +41,064 bytes |
| Actual Linux libavutil shared library | 1,489,977 bytes | 1,588,281 bytes | +98,304 bytes |

Timings vary (before 1,086.959–1,185.478 ms; after 466.037–632.130 ms).
The lower payload/time is specific to replacing every-picture keyframes in this
simple synthetic camera stream; it is not a hardware quality, bandwidth or
end-to-end latency guarantee. The small RSS difference is noise. Shared-library
sizes compare the original pinned prefix with the compiled GPU-binding patches;
they exclude source archives, headers, licenses and the rest of the application.
[Raw samples](pr-evidence/video-gpu-routing/measurements.json) and
[library hashes/sizes](pr-evidence/video-gpu-routing/native-library-sizes.json)
record the actual artifacts.

Native driver fixtures verify same-model GPU targeting, missing-device rejection,
metadata correction and resource release without physical hardware. The pinned
FFmpeg patches compile on Linux; their corresponding-source patch reproduces
byte-for-byte and reverses cleanly. Encoder fixtures exercise sixteen pending
inputs, reordered packet PTS, P5/look-ahead options and the 48-picture ceiling.
They do not prove physical hardware encoding or HEVC/AV1 receiver interoperability.
NVENC/QSV look-ahead and reordered output increase buffering and driver memory;
those costs, sustained leak behavior and actual Windows/macOS execution remain
unmeasured here. Standard executable/installed/compressed package sizes and native
demo CPU/RSS could not be measured: the current workspace check and standard
package commands stop at missing `glib-2.0.pc` development metadata. No new
full-desktop or whole-package performance claim is made.

## PR 567 review corrections — October 7, 2026

Compared original PR head `5b54b63fe3684309dc15d7a7ec7d53c96d70d504`
with reviewed runtime `737756718e5a675413bc0b33cf97087c429cc927`, which also
merges main `38d919d7`. These are the review corrections **plus upstream changes**,
not an isolated encoder optimization or the entire PR compared with main.
Host: Windows 11 Home 10.0.26200, Ryzen 7 7800X3D/16 logical CPUs,
33,410,678,784 bytes RAM, Rust 1.98.1. Other builds ran concurrently.

Both standard `cargo xtask package` builds passed with voice, normal fat LTO and
no demo/developer-session features, using the same locally built pinned
FFmpeg 7.1.5/OpenH264 2.6.0 prefix. All three packaged DLL hashes match between
builds. NSIS is unavailable, so no local installer executable was generated.
The installed metric sums regular-file bytes; ZIP uses Python `zipfile`, sorted
relative paths, DEFLATE level 6 and no enclosing directory. Every entry CRC was
verified. Packages were measured before this evidence-only appendix.

| Metric | Original PR | Review plus main | Delta |
| --- | ---: | ---: | ---: |
| Standard executable | 85,376,000 B | 85,985,792 B | +609,792 B / +0.7142% |
| Installed files (239 each) | 122,857,425 B | 123,467,254 B | +609,829 B / +0.4964% |
| Compressed ZIP | 78,476,318 B | 78,737,079 B | +260,761 B / +0.3323% |

The Windows MFT regression uses the exact original wait methods and corrected
helper with a synthetic NeedInput event followed by an empty event queue.
One warmup per path and five alternating optimized calls in one process gave
baseline samples 251.4037, 250.8659, 251.1911, 250.2568 and 250.5980 ms
(median **250.8659 ms**, then failure). The corrected path returns pending output
while retaining input credit; samples were 200, 200, 200, 400 and 200 ns. Those
small values are near timer/optimized-harness overhead, not a useful speedup
ratio. This demonstrates removal of the artificial 250 ms timeout, not real
MFT/GPU encoding latency. The original regression fails and the corrected
regression passes. Reproduce with
`python docs/pr-evidence/pr567-review/measure_mft_wait.py`; outputs stay in
`target/pr567-review/mft-event-flow`.

Both packaged executables also passed the real **RTX 5070 Ti NVENC** driver
queries and bounded synthetic encoder helpers for H.264, H.265 and AV1. NVIDIA
Windows driver version: 32.0.15.9186. An explicitly enumerated DXGI LUID selected
the same physical GPU; no software fallback was allowed. Each codec produced
validated keyframe/configuration output at camera 640×480/15 fps and screen
1280×720/30 fps. Query exit codes were all 1 (Available), test codes all 3 (both
presets), with empty stderr and no timeout. The helper exits before GUI,
credentials, network or capture initialization. These checks do not independently
decode GPU output, exercise renderer selection, establish sustained throughput,
or validate 8K/60 fps or Discord interoperability. The workspace tests separately
encode and independently decode software H.264, including 1440p and forced IDRs.

To repeat the GPU checks, build the included `adapter_ids.cpp` in a Microsoft
Developer PowerShell using `cl /nologo /EHsc /Fe:target/adapter_ids.exe
/Fo:target/adapter_ids.obj docs/pr-evidence/pr567-review/adapter_ids.cpp /link
dxgi.lib`, then run `target/adapter_ids.exe`. LUIDs are session-local; use the
fresh NVIDIA key in this Python snippet, run from the packaged worktree:

```python
import subprocess
adapter = "REPLACE_WITH_FRESH_NVIDIA_ADAPTER_KEY"
for operation in ("--query-video-codec", "--test-video-encoder"):
    for codec in ("h264", "h265", "av1"):
        result = subprocess.run(
            ["dist/serein.exe", operation, "nvenc", codec, "--adapter", adapter],
            stdin=subprocess.DEVNULL, timeout=10,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        print(operation, codec, result.returncode)
```

The changed workspace check passed: 1,349 tests, 29 ignored, strict Clippy,
formatting, policy and production compilation. No process CPU/RSS, frame-time,
physical camera/audio, AMF/QSV/VideoToolbox or live-call measurement is claimed.
The advisory scan still reports inherited unmaintained `ttf-parser 0.25.1`
(RUSTSEC-2026-0192), reproduced on main. Raw samples, helper exit codes and artifact
hashes are in [the review record](pr-evidence/pr567-review/results.json).

## Runtime bug hunt — October 7, 2026

Compared baseline `8003ae068679fecb67cd02c4bc5e6fbedf37ffee` with this task's
client-core search corrections. The measured after build uses the working tree
before adding this documentation; the source blob IDs and binary hashes are in
[the raw measurements](pr-evidence/runtime-bug-hunt/measurements.json).
Both actual `replay-bench` binaries use Rust 1.98.1, the pinned lockfile, normal
release fat LTO and no optional features. The baseline uses a detached worktree;
both builds share the same writable Cargo cache and target directory, with the
baseline binary copied aside before building the changed sources.

Host: Linux x86-64, Intel Xeon Platinum 8573C, five exposed logical CPUs and
17 GiB exposed RAM. After one warmup per binary, five measured runs per binary
use alternating pair order. No task compiler or native fixture process runs
during sampling; variation from the shared host remains. The existing workload
applies 100,000 synthetic message events, checks the 500-record / 4 MiB timeline
bounds and verifies logout releases retained timeline data.

| Metric / method | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Reducer elapsed median (five runs) | 99.675228 ms | 95.818134 ms | −3.857094 ms (−3.87%); noisy |
| Reducer elapsed sample range | 94.165187–106.522434 ms | 87.880852–107.966072 ms | Overlapping ranges |
| Retained timeline estimate | 331,992–332,477 B / 500 records | 331,992–332,477 B / 500 records | Unchanged |
| Release replay executable | 1,511,512 B | 1,516,760 B | +5,248 B (+0.3472%) |
| Standard voice-enabled app, installed and compressed package | Unmeasured | Unmeasured | Missing `glib-2.0.pc` prevents both builds |

This workload does not exercise permission changes with an open search or
desktop credential/cache failure handling. Its timing is a general reducer
regression check, with no performance improvement claim. The executable size
is for the reducer tool, not the installed application. Native UI CPU/RSS,
startup, search latency and application-wide leak detection are unmeasured.
The full workspace check and standard package attempt stop at missing GLib
development metadata. Native before/after screenshots for search retirement
and account cleanup status are consequently unavailable.

Offline native encoder query/configuration fixtures also pass with AddressSanitizer,
UndefinedBehaviorSanitizer and leak detection enabled. Both `CC` and `CXX`
receive sanitizer flags; the AMF C ABI mock has no C++ RTTI, so only its C++
vptr check is excluded with `-fno-sanitize=vptr`. This covers the compiled
query/shim fixtures and their synthetic resource counters, not the full app,
physical vendor drivers or the separately built FFmpeg libraries. No live
account, microphone, camera or screen capture was used.

## PR #567 upstream conflict resolution — October 7, 2026

Compared the verified release reducer from feature head
`11abe5a3700cfe0ec734808931d3791922d98b12` with the merged working tree including
upstream `20230cb91b0117fe4c888d70867fa600704dcad4`. The subsequent upstream
documentation cleanup `a76e030d749ef558e687bbff6dc592a7dddec244` changes none of
the measured runtime sources. Both use Rust 1.98.1, normal
fat LTO and no optional features; the merged build uses the updated upstream
lockfile. Separate binary paths preserve the baseline. Complete reducer source
blob IDs, executable hashes, warmups and samples are in
`docs/pr-evidence/pr567-conflict-resolution/measurements.json`.

Linux / Intel Xeon Platinum 8573C, five exposed logical CPUs, 17 GiB exposed RAM;
one warmup and five measured runs per binary, with alternating pair order and no
active task compiler during sampling. The workload replays 100,000 synthetic
events and verifies the bounded 500-record timeline and logout release.

| Metric | Baseline | Merged | Delta / method |
| --- | ---: | ---: | --- |
| Reducer median | 125.050355 ms | 144.784515 ms | +19.734160 ms (+15.78%); wide, overlapping sample ranges |
| Reducer sample range | 83.038196–158.760550 ms | 122.432188–168.732948 ms | Five samples each; shared-host scheduling noise |
| Retained timeline estimate | 331,992–332,477 B / 500 records | 331,992–332,477 B / 500 records | Unchanged; not process RSS |
| Release reducer executable | 1,516,760 B | 1,527,472 B | +10,712 B (+0.71%); not the application package |
| Standard voice-enabled app / installed / compressed package | Unmeasured | Unmeasured | Both full check and package attempts stop at missing `glib-2.0.pc` |

The measured median increased; the large timing spread prevents attributing that
change to the merge. No performance improvement is claimed. This reducer does
not exercise native audio, upload previews, renderer GPU routing or UI latency.
Full native GUI screenshots, process CPU/RSS, physical hardware encoding and
live service compatibility remain unverified. Offline audio and upload regressions
exercise their actual bounded production helpers without opening devices.
