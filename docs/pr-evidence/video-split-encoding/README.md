# Two-engine split encoding evidence

Baseline: `00e39d6d6ed90b313a1c98a7daadb2cdd2f9d49e`. Changed source hashes,
compiler/configuration, library hashes/sizes and all timing samples are in
[measurements.json](measurements.json).

Both optimized diagnostic FFmpeg 7.1.5 builds enable all ten outgoing encoders
with pinned NVENC 12.2.72.0, AMF 1.4.36 and oneVPL 2.14.0 headers. They link
Debian's OpenH264 `2.6.0+dfsg-2` and VPL `1:2.14.0-1+b1`, disable assembly,
Vulkan and VA, and omit the standard recipe's private OpenH264 namespace/GPU
memory integration. This checks the changed wrappers and real option tables,
not complete native packaging.

Passed against the changed diagnostic prefix:

```sh
python3 crates/discord-voice/tests/native/test_video_encoding.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_video_queries.py --prefix "$FFMPEG_DIR"
python3 crates/discord-voice/tests/native/test_ffmpeg_gpu_patch.py --archive ffmpeg-7.1.5.tar.xz
CC='cc -O1 -g -fsanitize=address,undefined -fno-omit-frame-pointer' \
  ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 UBSAN_OPTIONS=halt_on_error=1 \
  python3 crates/discord-voice/tests/native/test_video_encoding.py --prefix "$FFMPEG_DIR"
```

The AMF eligibility fixture also passes ASan/UBSan/LeakSanitizer: twenty codec
count, old-runtime and error cases without encoder initialization. The encoding
fixture covers 1080p, the 1440p boundary, portrait capture, 4K/8K, mandatory wide
AV1 tiling, superblock-area alignment and rejected split requests. It verifies
cleanup before retrying the same GPU/codec/quality. Existing query/GPU-binding,
delayed-output and 48-picture bounds pass. The LGPL patch reconstructs source
byte-for-byte, reverses cleanly and rejects duplicate application. Workspace
formatting and Python syntax pass.

`cargo xtask check` and `cargo xtask package` stop at missing `glib-2.0.pc` /
GLib >=2.70. No physical GPU, capture, account or network session was used. The
owner's earlier vendor/native-client tests precede this new split path.

## Matched setup measurement

[presets.c](presets.c) allocates/configures/frees 60,000 contexts across HEVC/AV1
NVENC, AMF and QSV at 2560x1440 without opening codecs or loading drivers. AMF's
hint is an option-table fixture on Linux, not an active DX11 session. Compile
each production shim against its matching diagnostic prefix using
`cc -O2 -std=c11 -Wall -Wextra -Werror`, include the voice source directory and
define `SHIM` to the shim's absolute path. Define `SPLIT_ENABLED=1` only for the
changed revision. Link `avcodec-serein` / `avutil-serein` with that prefix's rpath.

One warmup, then five alternating samples each; no builds during sampling.
Median setup time: 1301.148 -> 1371.227 ms (+5.39%), with overlapping ranges
1181.369-1388.280 / 1246.890-1481.352 ms. This is noisy setup cost, not frame
latency. Diagnostic libavcodec grows 757,600 -> 761,696 bytes (+4096);
libavutil remains 846,680 bytes. The unshipped benchmark executable grows
22,584 -> 26,712 bytes. No performance improvement is claimed. Native idle
CPU/RSS, complete app/package sizes, engine activity, hardware throughput/quality
and split-path interoperability remain unmeasured.
