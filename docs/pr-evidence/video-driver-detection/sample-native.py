"""Sample one offline native helper, using an owned X11 display and synthetic data."""
import ctypes
import ctypes.util
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

import psutil

binary, label, output = sys.argv[1:]
args = [binary, '--demo', '--page=voice', '--video-backend=experimental',
        '--video-codec=h265', '--capabilities=hardware', '--interactive',
        '--width=1120', '--height=760']
x = ctypes.CDLL(ctypes.util.find_library('X11'))
xt = ctypes.CDLL(ctypes.util.find_library('Xtst'))
x.XOpenDisplay.restype = ctypes.c_void_p
x.XOpenDisplay.argtypes = [ctypes.c_char_p]
x.XFlush.argtypes = [ctypes.c_void_p]
x.XCloseDisplay.argtypes = [ctypes.c_void_p]
xt.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
                                   ctypes.c_int, ctypes.c_ulong]
xt.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int,
                                   ctypes.c_ulong]
display = x.XOpenDisplay(os.environ['DISPLAY'].encode())
assert display, 'An owned X11 DISPLAY is required'
with open(Path(output).with_suffix('.log'), 'w') as log:
    process = subprocess.Popen(args, stdout=log, stderr=log)
    try:
        native = psutil.Process(process.pid)
        time.sleep(3)
        xt.XTestFakeMotionEvent(display, 0, 850, 450, 0)
        for _ in range(100):
            xt.XTestFakeButtonEvent(display, 5, 1, 0)
            xt.XTestFakeButtonEvent(display, 5, 0, 0)
        xt.XTestFakeMotionEvent(display, 0, 1279, 959, 0)
        x.XFlush(display)
        time.sleep(3)
        initial = native.cpu_times()
        started = time.monotonic()
        samples = []
        for _ in range(15):
            time.sleep(1)
            samples.append({'elapsed_s': time.monotonic() - started,
                            'rss_bytes': native.memory_info().rss})
        final = native.cpu_times()
        elapsed = time.monotonic() - started
        libraries = sorted({entry.path for entry in native.memory_maps()
                            if any(name in entry.path for name in
                                   ['gallium', 'LLVM', 'libEGL', 'libGLX', 'libvulkan'])})
        result = {
            'revision': label, 'command': args, 'warmup_s': 6,
            'interaction': '100 wheel-down events in settings body, pointer moved out',
            'duration_s': elapsed, 'interval_s': 1,
            'process_cpu_one_core_percent': 100 * (
                (final.user + final.system) - (initial.user + initial.system)) / elapsed,
            'peak_rss_bytes': max(sample['rss_bytes'] for sample in samples),
            'settled_rss_bytes': samples[-1]['rss_bytes'],
            'children': [{'pid': child.pid, 'rss_bytes': child.memory_info().rss}
                         for child in native.children(recursive=True)],
            'executable_bytes': Path(binary).stat().st_size,
            'executable_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
            'renderer_libraries': libraries, 'samples': samples,
        }
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        x.XCloseDisplay(display)
Path(output).write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: value for key, value in result.items()
                  if key not in ['command', 'samples']}))
