# Serein HDR surface patch

This copies only egui-wgpu from egui revision
`8f6d3d6ed99cb24d2e14c43951803d2868db40b1` (0.36.2). The rest of the GUI stack remains
at that revision. Upstream MIT and Apache-2.0 license texts are retained unchanged.
The standalone manifest expands upstream workspace dependencies without upgrading them.

The patch selects linear extended-sRGB HDR presentation only when that exact
format/color-space pair is advertised on an opaque surface. It polls current display
capabilities, retains an SDR fallback, and preserves textures and callbacks while
switching render pipelines. Ordinary UI colors still map to SDR reference white.
Screenshots use the ordinary SDR path. Media callbacks receive current output format,
reference white and display headroom; tone mapping remains the media renderer's job.
