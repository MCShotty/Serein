# Codebase bug hunt — October 5, 2026

Baseline: `8a369cc3c71c18d97065dde8197c16d90970ed0a`, the hardware-capability
branch. The original combined change was stacked on that branch.
All reproductions use synthetic data; no Discord session, credentials, microphone,
camera, screen capture or user conversation was accessed.

## Confirmed bugs and repairs

Historical evidence from the original combined PR, separated by scope. The recorded revision labels, hashes and measurements are unchanged; these results do not validate the new branch heads.

| Area | Trigger and repair |
| --- | --- |
| Stream negotiation | An initial null endpoint prematurely failed screen-stream allocation. Keep it pending within the original 30-second deadline and accept a subsequent valid endpoint. |
| Windows video | A zero-length native sample could reach a raw-slice operation with a null pointer. Validate length/capacity/pointer and return an empty vector without constructing that slice. |
| Remote video recovery | Another sender's active video hid a stalled participant. Track progress on bounded per-user source lifetimes and retire old progress on removal/full SSRC replacement. |
| Windows uninstall | Recursive removal of a custom installation root deleted unrelated files. Remove known FFmpeg payload files explicitly and only remove the root when empty. Owned `docs`, `licenses`, `source` and `ffmpeg-source` payload directories are still recursively removed. |

The original combined voice crate passed 102 tests (four ignored), with fresh native compilation and scoped strict Clippy. Both Linux OpenH264 runtime checks passed. These historical checks include fixes outside the video scope; current-head verification is recorded separately. No full desktop executable, physical capture or live-client check is claimed here.
