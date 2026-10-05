//! Session-local hardware checks, isolated from native startup and capture.
use model::voice_settings::{
	HardwareBackend, HardwareSupport, ProbeResult, VideoBackend, VideoCapabilities, VideoCodec,
};
use std::{
	ffi::OsString,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
		mpsc,
	},
	thread::JoinHandle,
	time::{Duration, Instant},
};

const PROBE_ARGUMENT: &str = "--probe-video-encoder";
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const UNKNOWN: HardwareSupport = HardwareSupport {
	camera: ProbeResult::Failed,
	screen: ProbeResult::Failed,
};

fn request(
	mut args: impl Iterator<Item = OsString>,
) -> Option<Result<(HardwareBackend, VideoCodec), ()>> {
	if args.next()?.to_str()? != PROBE_ARGUMENT {
		return None;
	}
	let backend = args.next().and_then(|arg| {
		HardwareBackend::ALL
			.into_iter()
			.find(|backend| Some(backend.key()) == arg.to_str())
	});
	let codec = args.next().and_then(|arg| {
		VideoCodec::ALL
			.into_iter()
			.find(|codec| Some(codec.key()) == arg.to_str())
	});
	Some(match (backend, codec, args.next()) {
		(Some(backend), Some(codec), None) => Ok((backend, codec)),
		_ => Err(()),
	})
}

/// Exit before GUI, credentials, network clients or device discovery initialize.
pub fn probe_command() -> Option<i32> {
	request(std::env::args_os().skip(1)).map(|request| match request {
		Ok((backend, codec))
			if discord_voice::video_capabilities::backends().contains(&backend) =>
		{
			let support = discord_voice::video_capabilities::probe(backend, codec);
			i32::from(support.camera == ProbeResult::Available)
				| (i32::from(support.screen == ProbeResult::Available) << 1)
		}
		_ => 64,
	})
}

fn decode_status(code: Option<i32>) -> HardwareSupport {
	let Some(code @ 0..=3) = code else {
		return UNKNOWN;
	};
	let available = |bit| {
		if code & bit != 0 {
			ProbeResult::Available
		} else {
			ProbeResult::Unavailable
		}
	};
	HardwareSupport {
		camera: available(1),
		screen: available(2),
	}
}

struct ProbeChild(Child);
impl Drop for ProbeChild {
	fn drop(&mut self) {
		// Reap on success, timeout, cancellation and every error path.
		let _ = self.0.kill();
		let _ = self.0.wait();
	}
}

fn wait_for_probe(command: &mut Command, stop: &AtomicBool, timeout: Duration) -> HardwareSupport {
	if stop.load(Ordering::Acquire) {
		return UNKNOWN;
	}
	// Native driver diagnostics cannot fill pipes or enter application logs.
	command
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	let Ok(child) = command.spawn() else {
		return UNKNOWN;
	};
	let mut child = ProbeChild(child);
	let started = Instant::now();
	loop {
		if stop.load(Ordering::Acquire) {
			return UNKNOWN;
		}
		match child.0.try_wait() {
			Ok(Some(status)) => return decode_status(status.code()),
			Err(_) => return UNKNOWN,
			Ok(None) => {}
		}
		if started.elapsed() >= timeout {
			return HardwareSupport {
				camera: ProbeResult::TimedOut,
				screen: ProbeResult::TimedOut,
			};
		}
		std::thread::sleep(POLL_INTERVAL);
	}
}

fn initial_report() -> VideoCapabilities {
	let unavailable = HardwareSupport {
		camera: ProbeResult::Unavailable,
		screen: ProbeResult::Unavailable,
	};
	let mut report = VideoCapabilities {
		support: [[unavailable; 4]; 3],
	};
	for &backend in discord_voice::video_capabilities::backends() {
		for codec in VideoCodec::ALL {
			report.set(backend, codec, HardwareSupport::default());
		}
	}
	report
}

fn finish_pending(report: &mut VideoCapabilities) {
	for codec in &mut report.support {
		for support in codec {
			for result in [&mut support.camera, &mut support.screen] {
				if *result == ProbeResult::Pending {
					*result = ProbeResult::Failed;
				}
			}
		}
	}
}

enum Update {
	Support(HardwareBackend, VideoCodec, HardwareSupport),
	Finished,
}

fn scan(
	executable: &Path,
	stop: &AtomicBool,
	send: &mpsc::SyncSender<Update>,
	wake: &eframe::egui::Context,
) {
	for &backend in discord_voice::video_capabilities::backends() {
		for codec in VideoCodec::ALL {
			if stop.load(Ordering::Acquire) {
				return;
			}
			let mut command = Command::new(executable);
			command.args([PROBE_ARGUMENT, backend.key(), codec.key()]);
			let support = wait_for_probe(&mut command, stop, PROBE_TIMEOUT);
			if send
				.try_send(Update::Support(backend, codec, support))
				.is_err()
			{
				return;
			}
			wake.request_repaint();
		}
	}
	let _ = send.try_send(Update::Finished);
	wake.request_repaint();
}

#[derive(Default)]
pub struct Detector {
	receive: Option<mpsc::Receiver<Update>>,
	thread: Option<JoinHandle<()>>,
	stop: Option<Arc<AtomicBool>>,
}

impl Detector {
	pub fn cancel(&self) {
		if let Some(stop) = &self.stop {
			stop.store(true, Ordering::Release);
		}
	}

	pub fn poll(&mut self, demo: bool, ui: &mut ui::MessagingUi, ctx: &eframe::egui::Context) {
		if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
			if let Some(thread) = self.thread.take() {
				let _ = thread.join();
			}
			self.stop = None;
		}
		let mut finished = false;
		if let Some(receive) = &self.receive {
			// At most twelve backend/codec updates and one terminal message.
			for _ in 0..13 {
				match receive.try_recv() {
					Ok(Update::Support(backend, codec, support)) => {
						if let Some(report) = &mut ui.video_capabilities {
							report.set(backend, codec, support);
						}
					}
					Ok(Update::Finished) | Err(mpsc::TryRecvError::Disconnected) => {
						finished = true;
						break;
					}
					Err(mpsc::TryRecvError::Empty) => break,
				}
			}
		}
		if finished {
			self.receive = None;
			ui.video_capabilities_loading = false;
			if let Some(report) = &mut ui.video_capabilities {
				finish_pending(report);
			}
		}
		if demo {
			// Offline demos never initialize GPU drivers; previews supply explicit fixtures.
			self.cancel();
			if ui.video_capabilities.is_none() {
				let mut report = initial_report();
				finish_pending(&mut report);
				ui.video_capabilities = Some(report);
			}
			ui.video_capabilities_refresh = false;
			ui.video_capabilities_loading = false;
			return;
		}
		let visible =
			ui.voice_settings_open() && ui.video_settings.backend == VideoBackend::Experimental;
		if !visible {
			self.cancel();
			ui.video_capabilities_refresh = false;
			return;
		}
		if !ui.video_capabilities_refresh || self.thread.is_some() || self.receive.is_some() {
			return;
		}
		ui.video_capabilities_refresh = false;
		let mut report = initial_report();
		let executable = std::env::current_exe();
		let stop = Arc::new(AtomicBool::new(false));
		// The matrix has twelve entries, so this fixed queue fits a complete scan plus EOF.
		let (send, receive) = mpsc::sync_channel(13);
		let worker_stop = stop.clone();
		let wake = ctx.clone();
		let thread = executable.ok().and_then(|executable: PathBuf| {
			std::thread::Builder::new()
				.name("video-capabilities".into())
				.spawn(move || {
					scan(&executable, &worker_stop, &send, &wake);
				})
				.ok()
		});
		if let Some(thread) = thread {
			self.receive = Some(receive);
			self.thread = Some(thread);
			self.stop = Some(stop);
			ui.video_capabilities_loading = true;
		} else {
			finish_pending(&mut report);
			ui.video_capabilities_loading = false;
		}
		ui.video_capabilities = Some(report);
	}
}

impl Drop for Detector {
	fn drop(&mut self) {
		self.cancel();
		if let Some(thread) = self.thread.take() {
			// Only application teardown waits; the worker kills/reaps an outstanding child.
			let _ = thread.join();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn offline_demo_settles_without_starting_a_probe_or_losing_fixtures() {
		let mut detector = Detector::default();
		let mut ui = ui::MessagingUi::default();
		let ctx = eframe::egui::Context::default();
		ui.video_capabilities_refresh = true;
		detector.poll(true, &mut ui, &ctx);
		assert!(ui.video_capabilities.is_some());
		assert!(!ui.video_capabilities_refresh && !ui.video_capabilities_loading);
		assert!(detector.thread.is_none() && detector.receive.is_none());
		assert!(
			!ui.video_capabilities
				.unwrap()
				.confirmed_unavailable(VideoCodec::H265)
		);
		let fixture = VideoCapabilities {
			support: [[HardwareSupport {
				camera: ProbeResult::Available,
				screen: ProbeResult::Unavailable,
			}; 4]; 3],
		};
		ui.video_capabilities = Some(fixture);
		detector.poll(true, &mut ui, &ctx);
		assert_eq!(ui.video_capabilities, Some(fixture));
	}

	#[test]
	fn session_teardown_cancels_without_waiting_for_worker_completion() {
		let stop = Arc::new(AtomicBool::new(false));
		let detector = Detector {
			receive: None,
			thread: None,
			stop: Some(stop.clone()),
		};
		detector.cancel();
		assert!(stop.load(Ordering::Acquire));
	}

	#[test]
	fn helper_argument_parser_never_accepts_capture_or_extra_arguments() {
		let args = |values: &[&str]| {
			values
				.iter()
				.map(OsString::from)
				.collect::<Vec<_>>()
				.into_iter()
		};
		assert!(request(args(&["--demo"])).is_none());
		assert_eq!(
			request(args(&[PROBE_ARGUMENT, "amf", "av1"])),
			Some(Ok((HardwareBackend::Amf, VideoCodec::Av1)))
		);
		for values in [
			vec![PROBE_ARGUMENT],
			vec![PROBE_ARGUMENT, "software", "h264"],
			vec![PROBE_ARGUMENT, "nvenc", "vp9"],
			vec![PROBE_ARGUMENT, "qsv", "h264", "extra"],
		] {
			assert_eq!(request(args(&values)), Some(Err(())));
		}
	}

	#[test]
	fn exit_status_preserves_each_profile_and_does_not_turn_crashes_into_support() {
		assert_eq!(
			decode_status(Some(1)),
			HardwareSupport {
				camera: ProbeResult::Available,
				screen: ProbeResult::Unavailable
			}
		);
		assert_eq!(
			decode_status(Some(2)),
			HardwareSupport {
				camera: ProbeResult::Unavailable,
				screen: ProbeResult::Available
			}
		);
		assert_eq!(
			decode_status(Some(3)),
			HardwareSupport {
				camera: ProbeResult::Available,
				screen: ProbeResult::Available
			}
		);
		for status in [None, Some(64), Some(70), Some(255)] {
			assert_eq!(decode_status(status), UNKNOWN);
		}
	}

	#[test]
	fn interrupted_scan_keeps_known_results_and_marks_unfinished_checks_unknown() {
		let mut report = VideoCapabilities::default();
		report.set(
			HardwareBackend::Amf,
			VideoCodec::H265,
			HardwareSupport {
				camera: ProbeResult::Available,
				screen: ProbeResult::Unavailable,
			},
		);
		finish_pending(&mut report);
		assert_eq!(
			report.codec(VideoCodec::H265)[HardwareBackend::Amf.index()].camera,
			ProbeResult::Available
		);
		assert_eq!(
			report.codec(VideoCodec::Av1)[HardwareBackend::Qsv.index()],
			UNKNOWN
		);
		assert!(!report.confirmed_unavailable(VideoCodec::Av1));
	}

	#[cfg(unix)]
	#[test]
	fn helper_deadline_and_cancellation_kill_and_reap_processes() {
		let stop = AtomicBool::new(false);
		let started = Instant::now();
		let mut command = Command::new("/bin/sh");
		command.args(["-c", "exec sleep 60"]);
		assert_eq!(
			wait_for_probe(&mut command, &stop, Duration::from_millis(40)).camera,
			ProbeResult::TimedOut
		);
		assert!(started.elapsed() < Duration::from_secs(2));
		let stop = Arc::new(AtomicBool::new(false));
		let worker_stop = stop.clone();
		let worker = std::thread::spawn(move || {
			std::thread::sleep(Duration::from_millis(40));
			worker_stop.store(true, Ordering::Release);
		});
		assert_eq!(
			wait_for_probe(&mut command, &stop, Duration::from_secs(3)),
			UNKNOWN
		);
		worker.join().unwrap();
	}

	#[cfg(unix)]
	#[test]
	fn missing_executable_and_invalid_helper_exit_remain_unknown() {
		let stop = AtomicBool::new(false);
		assert_eq!(
			wait_for_probe(
				&mut Command::new("/serein-missing-probe-helper"),
				&stop,
				Duration::from_secs(1)
			),
			UNKNOWN
		);
		let mut command = Command::new("/bin/sh");
		command.args(["-c", "exit 64"]);
		assert_eq!(
			wait_for_probe(&mut command, &stop, Duration::from_secs(1)),
			UNKNOWN
		);
	}
}
