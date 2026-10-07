"""Prepare an offline native UI preview with explicitly synthetic hardware results.

The same base preview runs against the before revision and current source. Driver
reports and optional encoder-test reports are separate fixtures, never evidence
of hardware on the machine running this helper.
"""
from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else Path(__file__).resolve().parents[3]
out = Path(sys.argv[1]).resolve()
subprocess.run([sys.executable, str(root / 'docs/pr-evidence/amd-quality-ui/prepare-preview.py'),
                str(out), str(root)], check=True)
source = (out / 'main.rs').read_text()
anchor = '\t\t\t\t\tif !messaging.video_settings.is_valid() {'
assert source.count(anchor) == 1
fixture = 'driver-fixture.rs' if 'pub struct DriverCapabilities' in (
    root / 'crates/model/src/voice_settings.rs').read_text() else 'before-fixture.rs'
source = source.replace(anchor, Path(__file__).with_name(fixture).read_text() + anchor)
(out / 'main.rs').write_text(source)
