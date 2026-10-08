# Internal AMF split acceptance

Baseline: `6451d97c49ddc6d69203fe20e3d1a28708b4da6e`. Evidence was collected
from the local working tree before committing. [Measurements](measurements.json)
record source/library hashes, configuration and component sizes;
[check excerpts](checks.log) record results and local build limits.

The bundled HEVC/AV1 AMF wrapper exposes a read-only acceptance bit. It is set
only when the selected codec advertises exactly two engines and its split
property write succeeds. The bridge reads it after successful initialization;
a retry without split remains declined. The existing diagnostic reporter counts
live accepted/declined camera and screen sessions independently, removes their
registrations on teardown, and snapshots counts into its bounded queue. There
is no new user control, status message, encoder session or per-frame driver query.

To inspect acceptance on Windows, close the previous instance and deliberately
attach stderr to the intended locally built executable:

```powershell
$env:SEREIN_VOICE_DIAGNOSTICS = "1"
Start-Process .\dist\serein.exe -RedirectStandardError "$PWD\split-debug.log" -Wait
```

With one active AMF 4K H.265 screen share, `StreamSend` includes
`amf_split_requested=1 amf_split_accepted=1` when the property and initialization
succeed. A declined request/retry reports `1` and `0`; no request reports `0`
and `0`. Camera counts appear under `Transport`. This is acceptance metadata.
Diagnostics never start capture or join a call. No live session was used here.

The exact LGPL FFmpeg GPU patch reconstructs and reverses cleanly against the
pinned 7.1.5 archive. Both AMF wrappers were compiled with the same diagnostic
configuration as the baseline: all ten outgoing encoders, assembly/Vulkan/VA
disabled, system OpenH264/oneVPL, without production OpenH264 namespace isolation.
This checks real option tables and wrapper code, not a complete native package.
The final generated source hashes are recorded; temporary build sources were
removed after installation to recover local disk space.

Reproducible focused checks, after building the native libraries:

```sh
python3 crates/discord-voice/tests/native/test_video_encoding.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_video_queries.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_ffmpeg_gpu_patch.py --archive ffmpeg-7.1.5.tar.xz
cargo run --locked -p discord-voice --example voice_diagnostics
```

The full current voice source was freshly compiled with recorded cached Rust
dependencies, a fresh C/C++ bridge and the rebuilt diagnostic FFmpeg libraries:
144 tests passed, 4 ignored; strict Clippy passed. The device-free diagnostic
example passed acceptance/off/rejection, fallback/lifetime snapshots and output
bounds. AMF split request and encoding-shim fixtures passed AddressSanitizer,
UndefinedBehaviorSanitizer and LeakSanitizer. The broader query sanitizer run
rejected the existing C-built AMFFactory mock's C++ vptr; normal query tests pass.

Workspace formatting passes. Full `cargo xtask check` was blocked by exhausted
scratch disk while compiling xtask prerequisites; `cargo xtask package` could
not acquire the non-writable cached workspace build lock. Whole-app package
sizes, CPU/RSS and native Windows/physical-GPU acceptance remain unmeasured.
