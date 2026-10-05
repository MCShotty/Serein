"""Compare actual headless detectors on a no-GPU host; not encoder throughput."""
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time

import psutil

before_binary, after_binary, output = sys.argv[1:]
output = Path(output)
env = dict(os.environ, LD_LIBRARY_PATH='/workspace/scratch/ffmpeg-codecs-final-prefix/lib')
results = {'before': [], 'after': []}
for run in range(6):
    for revision, binary in [('before', before_binary), ('after', after_binary)]:
        logfile = output.with_name(f'scan-{revision}-{run}.log')
        with logfile.open('w') as log:
            started = time.monotonic()
            process = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
            parent = psutil.Process(process.pid)
            parent_peak = 0
            child_peak = 0
            maximum_children = 0
            observed_children = {}
            while process.poll() is None:
                if time.monotonic() - started >= 95:
                    process.kill()
                    process.wait()
                    raise RuntimeError('Detector exceeded its fixed helper deadlines')
                try:
                    parent_peak = max(parent_peak, parent.memory_info().rss)
                    children = parent.children(recursive=True)
                    maximum_children = max(maximum_children, len(children))
                    for child in children:
                        try:
                            observed_children[child.pid] = child.create_time()
                            child_peak = max(child_peak, child.memory_info().rss)
                        except psutil.Error:
                            pass
                except psutil.Error:
                    pass
                time.sleep(0.001)
            elapsed = time.monotonic() - started
            assert process.returncode == 0, logfile.read_text()
            assert maximum_children <= 1, 'Detector overlapped helper processes'
            remaining = []
            for pid, created in observed_children.items():
                try:
                    child = psutil.Process(pid)
                    if child.create_time() == created:
                        remaining.append(pid)
                except psutil.Error:
                    pass
            assert not remaining, 'Observed helper remained alive after detector exit'
        results[revision].append({
            'run': run, 'warmup': run == 0, 'elapsed_s': elapsed,
            'parent_peak_rss_bytes': parent_peak, 'child_peak_rss_bytes': child_peak,
            'maximum_simultaneous_children': maximum_children,
            'children_observed': len(observed_children),
            'observed_children_remaining_after_exit': remaining,
            'output': logfile.read_text(),
        })
for revision in ['before', 'after']:
    samples = results[revision]
    binary = before_binary if revision == 'before' else after_binary
    results[revision] = {
        'executable_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
        'runs': samples,
        'median_elapsed_s': statistics.median(sample['elapsed_s'] for sample in samples[1:]),
        'median_parent_peak_rss_bytes': statistics.median(
            sample['parent_peak_rss_bytes'] for sample in samples[1:]),
        'median_child_peak_rss_bytes': statistics.median(
            sample['child_peak_rss_bytes'] for sample in samples[1:]),
    }
results['method'] = (
    'One warmup and five measured fresh processes per revision, sequential alternating before/after. '
    'Actual Detector and fresh actual Rust/native voice libraries; baseline synthetic encoder probes '
    'versus driver queries after. RSS/child observations polled every 1 ms; short allocations/helpers '
    'may be missed. No GPU device nodes or vendor drivers. Each scan starts nine serial helpers. '
    'Wrapper asserts all pairs Unavailable, and after asserts no encoder-test report was started. '
    'Elapsed includes process launch and polling. This measures fast failure/negative discovery, '
    'not successful hardware throughput, live call behavior, or production scan performance.')
output.write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps({revision: {key: value for key, value in results[revision].items()
                             if key != 'runs'} for revision in ['before', 'after']}))
