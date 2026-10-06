"""Prepare real native offline UI at the task baseline or changed revision.

The baseline-only adjustment exposes its existing synthetic screen picker. It
changes the example launcher, never the baseline production UI/capture code.
"""
from pathlib import Path
import subprocess
import sys

here = Path(__file__).resolve().parent
root = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else here.parents[2]
out = Path(sys.argv[1]).resolve()
subprocess.run([sys.executable, str(here.parent / 'video-driver-detection/prepare-preview.py'),
                str(out), str(root)], check=True)
source = (out / 'main.rs').read_text()
if '"screen-share"' not in source:
    replacements = {
        '\t\t\t| "friends"': '\t\t\t| "friends"\n\t\t\t| "screen-share"',
        '\t\t\t} else if page == "friends" {':
            '\t\t\t} else if page == "screen-share" {\n'
            '\t\t\t\ttest_support::call_demo_state()\n'
            '\t\t\t} else if page == "friends" {',
        '\t\t\t} else if page == "forum-settings" {':
            '\t\t\t} else if page == "screen-share" {\n'
            '\t\t\t\tmessaging.preview_screen_share(&state);\n'
            '\t\t\t} else if page == "forum-settings" {',
    }
    for before, after in replacements.items():
        assert source.count(before) == 1, before
        source = source.replace(before, after)
    (out / 'main.rs').write_text(source)
