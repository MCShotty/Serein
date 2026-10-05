# Linux OpenH264 ABI isolation

Ubuntu CI for PR #3 aborted in the native voice tests with `SIGABRT` and
`stack smashing detected`. The older Ubuntu Noble GStreamer plugin uses
OpenH264 2.4.1. Its unversioned `WelsCreateSVCEncoder` import instead bound to
the executable's static OpenH264 2.6.0 implementation. `GetDefaultParams`
then wrote the larger 2.6 parameter structure into the plugin's 2.4 stack
allocation. The system's newer Debian plugin had concealed this ABI mismatch.

The fix hides native static archive symbols in the desktop and voice-test
executables. The separate FFmpeg shared codec has seven prefixed public APIs,
hidden internal symbols and a private `libopenh264-serein.so.8` SONAME. Debian,
RPM, Arch and portable staging use that filename and retain the applied source
patch. Windows and macOS use their existing linking rules.

The official Ubuntu fixture URLs and checksums are in `verification.json`.
They were extracted into a task directory, without installing system packages.
The unchanged cached executable reproduced SIGABRT; a freshly built current
voice test executable passes the same exact test. Loader binding traces show
that the fixed plugin calls its own `libopenh264.so.7`, and the executable
exports no `Wels` symbols. The shared codec exports only the seven prefixed
APIs and their version node. The persisted Python regression also checks both
load orders with a synthetic host using ABI-major 8.

Local native development metadata is missing. The current test crate and its
actual build script/C shim were compiled afresh with pinned Rust 1.98.1,
reusing the exact cached dependency archives selected by Cargo fingerprints.
Real Debian GTK/WebKit/JavaScriptCore/Graphene runtime libraries were extracted
into scratch for linking. This is a current-source voice test, not a successful
workspace Cargo rebuild or a standard application package. Source, dependency
and produced-artifact hashes are retained in `voice-build.json`.

With the normal native development prerequisites installed, rebuild into fresh
directories and run the workspace check and encoder/package regressions:

```sh
python3 scripts/build-ffmpeg.py --prefix /tmp/serein-ffmpeg/prefix \
  --work-dir /tmp/serein-ffmpeg/work --jobs 4
export FFMPEG_DIR=/tmp/serein-ffmpeg/prefix
cargo xtask check
python3 -m unittest discover -s packaging/ffmpeg -p 'test_*.py'
python3 -m unittest discover -s packaging/linux -p 'test_*.py'
```

To exercise the older ABI explicitly, extract the two Ubuntu packages from
`verification.json` with `dpkg-deb -x`. Put only their `libgstopenh264.so` in a
separate plugin directory. Run the built voice test executable with that
directory in `GST_PLUGIN_PATH_1_0`, the extracted `libopenh264.so.7` directory
in `LD_LIBRARY_PATH`, a fresh `GST_REGISTRY_1_0` path, and these arguments:

```text
video_backend::native::tests::compact_i420_chroma_strides_encode_an_854_pixel_picture
--exact --nocapture --test-threads=1
```

Keep the normal host base/good/bad/libav plugins available. `LD_DEBUG=bindings`
records which library supplies `WelsCreateSVCEncoder`. Then run all voice tests
under each plugin version. These checks use synthetic frames and local test
transports; they do not capture devices or access a Discord account.

The fresh full Linux encoder recipe compiles all ten expected encoders. The C
ABI harness passes twenty codec/backend/profile configurations and decodes
both 330-frame software streams, whose bytes match the previous output.
Portable staging runs the same harness using only executable-relative library
lookup. Physical GPU encoding and release CPU/RSS/package comparisons remain
unmeasured. `cargo xtask check` still stops at missing `glib-2.0.pc` locally.
