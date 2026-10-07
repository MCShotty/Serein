"""Synthetic MFT event-flow regression only; no COM, GPU, capture, or encoder access."""
from pathlib import Path
import hashlib
import json
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "target/pr567-review/mft-event-flow"
SOURCE = "crates/discord-voice/src/video_encode_windows.rs"
BASELINE = "5b54b63fe3684309dc15d7a7ec7d53c96d70d504"
OUT.mkdir(parents=True, exist_ok=True)


def block(text, marker):
    start = text.index(marker)
    opening = text.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


baseline = subprocess.check_output(["git", "show", f"{BASELINE}:{SOURCE}"], cwd=ROOT, text=True)
fixed = (ROOT / SOURCE).read_text(encoding="utf-8")
methods = block(baseline, "fn wait_for_output(") + "\n" + block(baseline, "fn wait_until(")
helper = block(fixed, "fn wait_for_events(\n")
test = block(fixed, "fn delayed_mft_output_preserves_input_credit_and_drains_output_first()")

# These stubs reproduce only the external event interface. The old methods and
# new helper/test below are extracted verbatim, not rewritten implementations.
# Event IDs match windows 0.61.3 MediaFoundation metadata: 601/602/1.
header = r'''#![allow(non_upper_case_globals, non_snake_case, dead_code)]
use std::{time::{Duration, Instant}, cell::Cell};
struct EventType(i32);
const METransformNeedInput: EventType = EventType(601);
const METransformHaveOutput: EventType = EventType(602);
const MEError: EventType = EventType(1);
const MF_EVENT_FLAG_NO_WAIT: u32 = 1;
const MF_E_NO_EVENTS_AVAILABLE: i32 = 1;
const FAILED: &str = "Windows hardware video encoding failed";
struct Event;
impl Event {
    unsafe fn GetStatus(&self) -> Result<Result<(), ()>, ()> { Ok(Ok(())) }
    unsafe fn GetType(&self) -> Result<u32, ()> { Ok(601) }
}
struct Missing;
impl Missing { fn code(&self) -> i32 { MF_E_NO_EVENTS_AVAILABLE } }
struct Events(Cell<bool>);
impl Events {
    unsafe fn GetEvent(&self, _: u32) -> Result<Event, Missing> {
        if self.0.replace(true) { Err(Missing) } else { Ok(Event) }
    }
}
struct Encoder { need_input: usize, have_output: usize, events: Events }
'''
baseline_test = r'''
#[test]
fn normal_mode_mft_can_request_second_input_before_first_output() {
    let mut encoder = Encoder { need_input: 0, have_output: 0, events: Events(Cell::new(false)) };
    assert!(encoder.wait_for_output().is_ok(), "baseline timed out despite available input credit");
}
'''
benchmark_main = r'''
fn measure(fixed: bool) -> u128 {
    if fixed {
        let (mut input, mut output) = (0, 0);
        let mut pending = true;
        let start = Instant::now();
        let result = wait_for_events(&mut input, &mut output, true, || {
            Ok(if std::mem::replace(&mut pending, false) { Some(601) } else { None })
        });
        let nanos = start.elapsed().as_nanos();
        assert_eq!(result, Ok(false));
        assert_eq!((input, output), (1, 0));
        nanos
    } else {
        let mut encoder = Encoder { need_input: 0, have_output: 0, events: Events(Cell::new(false)) };
        let start = Instant::now();
        let result = encoder.wait_for_output();
        let nanos = start.elapsed().as_nanos();
        assert_eq!(result, Err(FAILED));
        assert_eq!((encoder.need_input, encoder.have_output), (1, 0));
        nanos
    }
}
fn main() {
    // Warm each path once; then alternate five calls to each in this process.
    let _ = measure(false);
    let _ = measure(true);
    for index in 1..=5 {
        println!("baseline,{index},{}", measure(false));
        println!("fixed,{index},{}", measure(true));
    }
}
'''
sources = {
    "baseline-regression": header + "impl Encoder {\n" + methods + "\n}\n" + baseline_test,
    "fixed-regression": header + helper + "\n#[test]\n" + test,
    "benchmark": header + "impl Encoder {\n" + methods + "\n}\n" + helper + benchmark_main,
}
commands = []
results = {}
for name, content in sources.items():
    source = OUT / f"{name}.rs"
    binary = OUT / f"{name}.exe"
    source.write_text(content)
    command = ["rustc", "--edition", "2024", "-O"]
    if name != "benchmark":
        command += ["--test"]
    command += [str(source), "-o", str(binary)]
    commands.append(command)
    subprocess.run(command, cwd=ROOT, check=True)
    command = [str(binary)] + (["--nocapture"] if name != "benchmark" else [])
    commands.append(command)
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    (OUT / f"{name}.log").write_text(result.stdout + result.stderr)
    expected = 101 if name == "baseline-regression" else 0
    if result.returncode != expected:
        raise RuntimeError(f"Unexpected {name} exit {result.returncode}; expected {expected}")
    results[name] = {"exit_code": result.returncode, "expected_exit_code": expected}
    if name == "benchmark":
        values = {"baseline": [], "fixed": []}
        for line in result.stdout.splitlines():
            mode, _, nanos = line.split(",")
            values[mode].append(int(nanos))
        results["measurements"] = {
            mode: {"nanoseconds": samples, "median_nanoseconds": statistics.median(samples)}
            for mode, samples in values.items()
        }

report = {
    "scope": "Synthetic event-flow timeout reproduction; not GPU, encoding throughput, or live performance",
    "baseline_commit": BASELINE,
    "source": SOURCE,
    "fixed_source_sha256": hashlib.sha256(fixed.encode()).hexdigest(),
    "method": "Exact source method/helper extraction; mock NeedInput event then empty event queue; one warmup and five alternating measured calls per path in one optimized Rust process",
    "toolchain": subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True).strip(),
    "commands": commands,
    "results": results,
}
(OUT / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report["results"], indent=2))
print(f"Evidence: {OUT / 'results.json'}")
