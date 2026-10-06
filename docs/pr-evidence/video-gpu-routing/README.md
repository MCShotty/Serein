# Renderer GPU selection and encoding quality

Starting fork source: `79e336066beff5ea6c414268308f1f817c50c2f4`.
The parent-facing source at `5a8bb2e6118b1de697775c15abd3f9bd6dc29173`
has the same tree. This evidence describes the subsequent GPU selection,
quality settings and delayed-picture transport changes, rather than the entire
feature PR against upstream. There is no visible settings-layout change in this
addition, so new before/after screenshots are not applicable.

## Behavior and reproduction

1. In General, select Automatic, High performance or Power saving, then restart
   the application. The encoder uses the physical adapter actually chosen by
   the renderer, including when two cards have the same vendor and model.
   Changing a saved preference during a call does not change its adapter.
2. Select Experimental in Voice & Video. NVENC requests P5/HQ for H.264,
   H.265 and AV1, plus sixteen-picture lookahead when supported. QSV retains
   medium and CBR, requesting sixteen-picture external-BRC lookahead. AMD
   retains balanced AV1 and its highest H.265 quality preset, with optional
   preanalysis. Driver queries and explicit Test encoder target the same GPU.
3. H.264 remains free of B-frames for the receiver's SPS/DAVE compatibility
   contract. NVENC/QSV request up to two B-frames for H.265/AV1; VideoToolbox
   permits H.265 reordering. The bundled AMF HEVC/AV1 wrappers expose no
   B-frame encoding control. Unsupported advanced features are retried on the
   same GPU. Reordered modes leave the reference structure to the driver/preset
   instead of imposing a one-picture reference-buffer limit. H.264 can
   subsequently fall back to software; H.265/AV1 report
   failure without changing the selected codec.
4. A static screen continues feeding its retained picture at the selected
   cadence while output is pending. Camera preview remains available while
   lookahead fills. Dropped encoded pictures request a fresh prediction chain.
   Strong encryption/session resets discard queued and pending old pictures,
   including when an ordinary keyframe request is already waiting.

Physical-GPU encoding, native Windows/macOS execution and live Discord reception
are unverified. The tests below use synthetic/offline inputs and no account,
microphone, camera or user screen capture.

## Verification method

[verification.json](verification.json) records results and final source hashes.
The linked actual voice tests were rebuilt from current Rust sources, actual
`build.rs` and fresh C/C++ objects using the patched FFmpeg prefix. Cached Rust
dependency artifacts were selected by their exact Cargo fingerprints. Desktop
checks rebuilt a coherent set of current workspace library metadata before
checking its actual test target and strict Clippy. These are scoped checks;
desktop metadata checking is not a linked production desktop build or an
execution of all desktop tests.

The normal delivery commands were also attempted:

```sh
cargo xtask check
cargo xtask package
cargo fmt --all -- --check
cargo fmt --manifest-path fuzz/Cargo.toml --all -- --check
cargo xtask policy
node tests/xtask-package.cjs
python3 -m unittest packaging/flatpak/test_build.py
```

The full check and standard package stop at missing `glib-2.0.pc` development
metadata. Formatting, policy and offline packaging fixtures pass. No full local
workspace/package pass is claimed. New platform CI must validate this source.

Native driver and encode-boundary fixtures can be reproduced against a completed
prefix built by `scripts/build-ffmpeg.py`:

```sh
python3 crates/discord-voice/tests/native/test_video_queries.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_amf_query.py --include "$FFMPEG_DIR/include"
python3 crates/discord-voice/tests/native/test_video_encoding.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_ffmpeg_gpu_patch.py \
  --archive "$FFMPEG_DIR/share/serein-ffmpeg/source/ffmpeg-7.1.5.tar.xz"
```

They exercise exact device identities and resource release, including matching
GPU models, missing devices, QSV driver-corrected metadata, P5/lookahead options,
sixteen pending inputs, nonmonotonic presentation timestamps and the 48-picture
ceiling. Mocked driver/packet boundaries do not prove real hardware behavior.
The pinned FFmpeg libraries compile on Linux. Their generated corresponding-source
patch reproduces byte-for-byte, reverses cleanly and rejects duplicate application.

## Software camera benchmark

```sh
python3 docs/pr-evidence/video-gpu-routing/measure-encoder.py \
  --prefix "$FFMPEG_DIR" --output target/video-gpu-benchmark
ffmpeg -v error -xerror -i target/video-gpu-benchmark/before.h264 -f null -
ffmpeg -v error -xerror -i target/video-gpu-benchmark/after.h264 -f null -
```

The C benchmark compiles the actual baseline/current shim using GCC `-O2`, with
both processes linked to the same patched FFmpeg 7.1.5/OpenH264 2.6.0 libraries.
It compares baseline camera Baseline/GOP 1 with Experimental camera Main/GOP 30
at 640×480, 15 fps and a 600 kbps target. Each run generates thirty warmup and
three hundred measured moving-gradient limited-range BT.601 I420 pictures.
Elapsed time includes pixel preparation and bitstream writes. One alternating
warmup pair is discarded, followed by five measured alternating pairs.

Linux 6.18.44 x86-64, Xeon Platinum 8573C, CPU affinity 0–4, four-core cgroup
quota, 16 GiB memory ceiling. RSS is sampled from `/proc` every 10 ms and may
miss brief peaks. No Cargo/native compilation overlapped the recorded samples.
Both final 330-picture streams decode successfully with system FFmpeg.

| Metric / median of five runs | Baseline | After | Delta |
| --- | ---: | ---: | ---: |
| Elapsed for 300 measured pictures | 1,115.033 ms | 546.520 ms | −50.99% |
| Encoded payload for 300 pictures | 4,537,700 bytes | 225,825 bytes | −95.02% |
| Keyframes among 300 pictures | 300 | 10 | −290 |
| Sampled peak process RSS | 8,753,152 bytes | 8,740,864 bytes | −0.14% |

Timing ranges are 1,086.959–1,185.478 ms before and 466.037–632.130 ms after.
The time/payload reduction is specific to predictive encoding of this simple
synthetic camera stream. It is not a hardware quality, live bandwidth or latency
guarantee. The small RSS difference is noise. Input is generated without camera,
renderer, GPU, capture or transport. There are no helper children.
[measurements.json](measurements.json) retains every sample, compiler command,
source and bitstream hash; its environment/validation fields also record the
external checks performed after measurement.

## Component size and limits

[native-library-sizes.json](native-library-sizes.json) compares actual original
and patched Linux shared libraries: libavcodec grows 41,064 bytes, libavutil
98,304 bytes. These exclude archives, headers, notices and the rest of the
application; they are not a whole-package comparison.

Executable, installed-package and compressed-distribution sizes, native demo
CPU/RSS, hardware memory/latency and sustained leak behavior remain unmeasured
because the standard desktop build lacks GLib development metadata and this host
has no physical encoder GPU. Lookahead/reordering intentionally add buffering
and native surfaces. The 48-picture timestamp/input ceiling, 2 MiB access-unit
ceiling and selected capture geometry are component bounds, not a process RAM
cap. An 8K I420 picture alone occupies 49,766,400 bytes before separate native
capture/codec surfaces.
