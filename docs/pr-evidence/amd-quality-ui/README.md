# Historical AMD quality / responsive UI evidence

Historical evidence from the original combined PR, separated by scope. The recorded revision labels, hashes and measurements are unchanged; these results do not validate the new branch heads.

Baseline: `be644d9064e0b53caf6db103037c8a48a5c7baa8`. The original synthetic native eframe/wgpu captures use the same fonts, viewport and zoom before/after. They are isolated development previews, not the shipped desktop app.

`before.png` / `after.png` show AMD encoder settings; `voice-bottom-after.png` shows video controls at the end of Voice & Video. On a configured native host, reproduce using each recorded revision’s `profile_preview --demo --page=voice --video-backend=experimental --video-codec=h265 --width=1120 --height=760`, with `--scroll=100000` for the bottom controls.

The normal-mode C fixture checks 20 FFmpeg option configurations and decodes both 330-frame H.264 streams. Physical AMF quality/throughput is unmeasured. See [measurements.json](measurements.json) for unchanged native configuration results.
