"""Create an offline native UI preview with explicitly synthetic capability results."""
from pathlib import Path
import subprocess
import sys
root=Path(sys.argv[2]).resolve() if len(sys.argv)>2 else Path(__file__).resolve().parents[3]
out=Path(sys.argv[1]).resolve()
if 'pub struct DriverCapabilities' in (root/'crates/model/src/voice_settings.rs').read_text():
    # Newer checkouts split driver discovery from optional encoder tests. Keep
    # this entry point runnable without injecting the old report type.
    subprocess.run([sys.executable,str(root/'docs/pr-evidence/video-driver-detection/prepare-preview.py'),str(out),str(root)],check=True)
    raise SystemExit(0)
subprocess.run([sys.executable,str(root/'docs/pr-evidence/amd-quality-ui/prepare-preview.py'),str(out),str(root)],check=True)
# Leave the same baseline harness untouched when measured against the base revision.
if 'pub struct VideoCapabilities' in (root/'crates/model/src/voice_settings.rs').read_text():
    source=(out/'main.rs').read_text()
    anchor='\t\t\t\t\tif !messaging.video_settings.is_valid() {'
    assert source.count(anchor)==1
    fixture=Path(__file__).with_name('capability-fixture.rs').read_text()
    (out/'main.rs').write_text(source.replace(anchor,fixture+anchor))
