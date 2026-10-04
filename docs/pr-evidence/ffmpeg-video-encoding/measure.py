"""Offline Linux camera/native-idle measurements; requires psutil and xdotool.

Start an isolated Xvfb :88 before idle mode. Both paths must name release builds.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import statistics
import subprocess
import time

import psutil


def identity(path):
    data = Path(path).read_bytes()
    return {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def camera(path):
    command = [str(path), "--exact", "camera::synthetic_camera_encode_workload", "--ignored", "--nocapture"]
    child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    process = psutil.Process(child.pid)
    peak = samples = 0
    while child.poll() is None:
        try:
            peak = max(peak, process.memory_info().rss)
            samples += 1
        except psutil.NoSuchProcess:
            break
        time.sleep(0.01)
    output, _ = child.communicate(timeout=10)
    if child.returncode:
        raise RuntimeError(output[-3000:])
    match = re.search(r"camera_encode_ms=([0-9.]+) packets=(\d+) bytes=(\d+)", output)
    if not match:
        raise RuntimeError(output[-3000:])
    elapsed, packets, encoded = match.groups()
    return {"encode_300_frames_ms": float(elapsed), "packets": int(packets),
            "encoded_bytes": int(encoded), "sampled_peak_rss_bytes": peak, "rss_samples": samples}


def preview(path, phase, output_dir):
    command = [str(path), "--demo", "--interactive", "--page=appearance", "--width=1120", "--height=760"]
    env = dict(os.environ, DISPLAY=":88", VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/lvp_icd.json", WGPU_BACKEND="vulkan")
    output = output_dir / f"serein-{phase}-idle.log"
    with output.open("w") as log:
        child = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            process = psutil.Process(child.pid)
            time.sleep(8)
            if child.poll() is not None:
                raise RuntimeError(output.read_text()[-3000:])
            windows = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--pid", str(child.pid)], env=env, text=True, timeout=5).splitlines()
            subprocess.run(["xdotool", "mousemove", "--window", windows[0], "1100", "740"], env=env, check=True, timeout=5)
            time.sleep(3)
            libraries = sorted({entry.path for entry in process.memory_maps() if "vulkan" in entry.path or "lvp" in entry.path})
            if not any("vulkan_lvp" in name for name in libraries):
                raise RuntimeError(f"Expected lavapipe library: {libraries}")
            process.cpu_percent(None)
            samples = []
            for _ in range(20):
                cpu = process.cpu_percent(interval=1)
                samples.append({"cpu_percent_one_core": cpu, "rss_bytes": process.memory_info().rss,
                                "children": [{"name": c.name(), "rss_bytes": c.memory_info().rss} for c in process.children(recursive=True)]})
            return {"command": command, "warmup_seconds": 8, "settle_seconds": 3,
                    "sample_interval_seconds": 1, "renderer_libraries": libraries, "samples": samples,
                    "cpu_mean_percent_one_core": statistics.mean(s["cpu_percent_one_core"] for s in samples),
                    "cpu_median_percent_one_core": statistics.median(s["cpu_percent_one_core"] for s in samples),
                    "sampled_peak_rss_bytes": max(s["rss_bytes"] for s in samples),
                    "settled_rss_bytes": statistics.median(s["rss_bytes"] for s in samples[-5:])}
        finally:
            if child.poll() is None:
                child.send_signal(signal.SIGINT)
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()


parser = argparse.ArgumentParser()
parser.add_argument("kind", choices=["camera", "idle"])
parser.add_argument("baseline", type=Path)
parser.add_argument("after", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
result = {"baseline_binary": identity(args.baseline), "after_binary": identity(args.after)}
if args.kind == "camera":
    result["warmups"] = {"baseline": camera(args.baseline), "after": camera(args.after)}
    result["baseline_samples"] = []
    result["after_samples"] = []
    for _ in range(5):
        result["baseline_samples"].append(camera(args.baseline))
        result["after_samples"].append(camera(args.after))
    for phase in ["baseline", "after"]:
        result[phase + "_median_ms"] = statistics.median(s["encode_300_frames_ms"] for s in result[phase + "_samples"])
else:
    for phase in ["baseline", "after"]:
        result[phase] = preview(getattr(args, phase), phase, args.output.resolve().parent)
args.output.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({key: value for key, value in result.items() if key.endswith("median_ms")}, indent=2))
