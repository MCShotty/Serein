# Historical resource cleanup — October 8, 2026

Historical evidence from the original combined PR, separated by scope. The recorded revision labels, hashes and measurements are unchanged; these results do not validate the new branch heads.

Baseline: `98158b4106588df7a14bcc1d9ecc90d71ac1f931`. After source SHA-256 labels, original measurements and test logs are in [measurements.json](measurements.json) and [checks.log](checks.log). Debian 13 x86_64, pinned Rust 1.98.1, synthetic offline inputs; no account, capture device, physical GPU or live installer was used.

AMF metadata variants may own interfaces or strings even after a property error. Both now clear on every exit. The baseline fixture reproduced zero releases; the fix releases one owned interface. HEVC/AV1 have 28 split ownership/error cases. The FFmpeg source fixture was also updated with the AMF patch targets.

Reproduce the native query/encoding tests and `test_ffmpeg_gpu_patch.py` under `crates/discord-voice/tests/native`, followed by the packaging/ffmpeg tests. ASan/UBSan/LeakSanitizer checks passed historically; the AMF C-ABI mock excludes vptr only. Optimized codec/util sizes are 761,696 / 846,680 bytes; source/library hashes and configuration are retained in the JSON. Full desktop/package checks stopped at missing GLib development metadata. Physical split use remains unverified.
