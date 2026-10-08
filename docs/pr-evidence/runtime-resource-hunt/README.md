# Resource cleanup bug hunt — October 8, 2026

Baseline: `98158b4106588df7a14bcc1d9ecc90d71ac1f931`, branch
`fix/pr567-upstream-conflicts`, PR #567. The working tree was clean at the start.
The parent default branch was fetched without changing this task branch.
After sources are identified by SHA-256 in [measurements.json](measurements.json).
All inputs are offline/synthetic. No account, capture device, physical GPU or
live installer was used. Not applicable — no visible UI change.

## Confirmed problems and fixes

- **AMF capability metadata:** unexpected property types, including values
  populated before a property error, may own interfaces or strings. The new
  split helper released its capability object but never cleared either AMF
  variant. The fixture's allocated interface reproduced a missing release;
  both variants now clear on every exit. HEVC and AV1 have eight additional
  ownership/error cases, for 28 AMF split cases total. This does not show that
  an ordinary physical driver returns these unexpected values.
- **Updater shutdown:** cleanup ran when a worker could not send its result,
  but a successfully queued result was dropped without cleanup if the app
  closed before polling. Both staged files and a prepared child process could
  survive. A result guard retains cleanup ownership until `sync` accepts it.
  Destruction cleans files or kills/reaps the helper off the UI thread.
  Queued files and an inert `sleep` helper fail their cleanup checks against
  baseline production code with the new tests backported. Both pass after;
  normal ownership transfer and completion after receiver closure also pass.
- **Registered games:** lowercase Unicode expansion could return a path
  exceeding the 256-byte limit, even though the input met it. Such an accepted
  path then failed validation during reload. Normalization now checks its
  output length. The regression fails before and passes after, including a
  255-byte accepted path that remains valid after normalization/sanitization.
- **Stale packaging fixtures:** the synthetic FFmpeg source omitted the new
  AMF patch targets; the xtask fixture omitted the Symbols2 font license
  added upstream. Both failed before and pass with complete fixture inputs.
  Production packaging behavior is unchanged by these fixture repairs.

Review also traced capture restarts, encoder fallback/GPU binding, receive-side
packet bounds, media pacing, background tasks and cache cleanup. These checks
and the unit suites are a scoped review, not proof that the entire app is free
of bugs or leaks.

## Verification and reproduction

Debian 13, Linux x86_64, pinned Rust 1.98.1. Rust unit/check builds use the locked
workspace, no incremental compilation and debug information disabled. The
native diagnostic build uses pinned FFmpeg 7.1.5 and the same SDK headers,
compiler and configuration as the preceding split-feature diagnostic build.
Both changed AMF wrappers were rebuilt after the helper change.

```sh
cargo test --lib --locked -p model -p client-core -p session-cache \
  -p local-store -p discord-protocol -p ui
cargo clippy --lib --locked -p model -p client-core -p session-cache \
  -p local-store -p discord-protocol -p ui -- -D warnings
python3 docs/pr-evidence/runtime-resource-hunt/updater-scope.py test
python3 docs/pr-evidence/runtime-resource-hunt/updater-scope.py clippy
python3 crates/discord-voice/tests/native/test_video_encoding.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_video_queries.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_ffmpeg_gpu_patch.py --archive /path/to/ffmpeg-7.1.5.tar.xz
python3 -m unittest discover -s packaging/ffmpeg -p 'test_*.py'
cargo fmt --all -- --check
cargo xtask policy
node tests/xtask-workspace.cjs
node tests/xtask-package.cjs
```

Set writable `CARGO_HOME`/`CARGO_TARGET_DIR` and cache the pinned dependencies for
offline commands. The updater harness compiles the actual updater, delta and
install modules and actual egui/UI dependencies. Only eframe's rendering
boundary is replaced by an egui re-export. Its shared dependency versions all
match entries in the production lockfile; its generated lock adds only the
harness package and eframe stub. It cannot establish full desktop integration.

Results: 779 workspace unit tests pass, 11 ignored; eight scoped updater tests
pass. Strict Clippy passes for those scopes. Native encoder/query tests pass
normally and with ASan/UBSan/LeakSanitizer. For the C++ AMF C-ABI mock only, vptr
checking is excluded; other sanitizer checks remain enabled. The pinned patch
reproduces byte-for-byte, reverses cleanly and rejects duplicate application.
Packaging passes 19 tests, with three platform/native-prefix checks skipped.
Formatting, Python syntax, persistence policy and both xtask fixtures pass.

`cargo xtask check` and `cargo xtask package` were attempted; both stop at missing
`glib-2.0.pc` / GLib >=2.70. The pinned cargo-deny checker is unavailable. Full
desktop/transport, Windows/macOS runtime, license and current package checks
remain unverified here. Physical split-encoding/interoperability testing still
needs supported hardware; the owner's earlier vendor testing predates it.

## Relevant resource comparison

| Component measurement | Baseline | After |
| --- | ---: | ---: |
| Releases of one returned owned AMF interface | 0 | 1 |
| Queued staged directories left after updater closure (one fixture) | 1 | 0 |
| Queued inert helper exits within the 3-second test bound | No | Yes |
| Optimized diagnostic libavcodec installed bytes | 761,696 | 761,696 |
| Optimized diagnostic libavutil installed bytes | 846,680 | 846,680 |

The AMF ownership fixture covers both codecs and success/error returns of an
unexpected type. Updater rows are one deterministic reproduction each, not
statistical timing measurements. The helper test kills its synthetic process
after a baseline timeout, so the reproduction itself leaves no running helper.
Raw failures and successful summaries are in [checks.log](checks.log).

The baseline installed diagnostic libraries predate this hunt; their generated
AMF helper was verified byte-identical to the starting commit. The same
configured source tree was rebuilt for after. Library hashes and configuration
are recorded in the JSON. Source patch round-trip checks use the pinned pristine
archive, independent of this diagnostic build. No package-size, idle CPU/RSS,
frame latency or hardware-throughput improvement is claimed: the complete
native app/package cannot be built in this environment.
