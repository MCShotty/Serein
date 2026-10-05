//! Explicitly selected, memory-only screen capture and video encoding.
pub use client_core::screen::{Settings, Source, SourceId};
use model::voice_settings::{VideoCodec, VideoSettings};
#[cfg(target_os = "linux")]
#[path = "screen/audio_linux.rs"]
mod audio_linux;
#[cfg(all(test, not(target_os = "windows")))]
#[path = "screen/audio_windows.rs"]
mod audio_windows;
#[cfg(not(target_os = "linux"))]
#[path = "screen/capture.rs"]
mod capture;
#[path = "screen/convert.rs"]
mod convert;
#[cfg(target_os = "linux")]
#[path = "screen/gstreamer.rs"]
mod gstreamer;
#[cfg(target_os = "linux")]
#[path = "screen/linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
#[path = "screen/portal_linux.rs"]
mod portal_linux;

use std::sync::{
	Arc, Mutex,
	atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
	mpsc,
};

#[cfg(not(target_os = "linux"))]
use std::time::{Duration, Instant};

pub const MAX_RAW_BYTES: usize = 3840 * 2160 * 4;
pub const MAX_ENCODED_BYTES: usize = 2 * 1024 * 1024;

pub struct RawFrame {
	pub width: u32,
	pub height: u32,
	pub stride: usize,
	pub data: Vec<u8>,
}

pub struct EncodedFrame {
	pub codec: VideoCodec,
	pub data: Vec<u8>,
	pub timestamp: u32,
	pub keyframe: bool,
}

/// Longest system-audio chunk accepted from the OS: 100 ms of 48 kHz stereo.
pub const MAX_AUDIO_SAMPLES: usize = 4800 * 2;

#[derive(Debug)]
pub struct AudioChunk {
	pub samples: Vec<f32>,
	/// Capture generation; old buffers must not cross an encryption transition.
	pub epoch: u64,
}

pub struct Video {
	pub video_settings: VideoSettings,
	pub settings: Settings,
	pub frames: tokio::sync::mpsc::Receiver<EncodedFrame>,
	pub ready: Arc<AtomicBool>,
	pub keyframe: Arc<AtomicBool>,
	/// Transport feedback target in bits per second, bounded by the selected quality.
	pub bitrate: Arc<AtomicU32>,
	/// Interleaved 48 kHz stereo system audio, present only when the share requested it.
	pub audio: Option<tokio::sync::mpsc::Receiver<AudioChunk>>,
	pub audio_epoch: Arc<AtomicU64>,
}

/// Whether this platform can capture system audio with the screen.
pub fn audio_supported() -> bool {
	supported()
}

pub fn supported() -> bool {
	cfg!(any(
		target_os = "macos",
		target_os = "windows",
		target_os = "linux"
	))
}

pub fn sources() -> Result<Vec<Source>, &'static str> {
	#[cfg(target_os = "linux")]
	{
		let mut sources = vec![Source {
			id: SourceId::Portal,
			name: "Choose in the system picker".into(),
		}];
		if linux::x11_session() {
			sources.push(Source {
				id: SourceId::X11Desktop,
				name: "Entire X11 desktop · all monitors · no portal".into(),
			});
		}
		Ok(sources)
	}
	#[cfg(not(target_os = "linux"))]
	capture::sources()
}

pub struct Worker {
	stop: Arc<AtomicBool>,
	ready: Arc<AtomicBool>,
	preview: Arc<Mutex<Option<image::RgbaImage>>>,
	done: Option<mpsc::Receiver<Result<(), &'static str>>>,
	#[cfg(target_os = "linux")]
	status: Arc<Mutex<&'static str>>,
	#[cfg(target_os = "linux")]
	preview_visible: Arc<AtomicBool>,
}

impl Worker {
	pub fn start(
		settings: Settings,
		video_settings: VideoSettings,
		wake: impl Fn() + Send + 'static,
	) -> Result<(Self, Video), &'static str> {
		if !settings.valid() || !video_settings.is_valid() || !supported() {
			return Err("Screen sharing is unavailable for these settings or this platform");
		}
		let stop = Arc::new(AtomicBool::new(false));
		let ready = Arc::new(AtomicBool::new(false));
		let audio_epoch = Arc::new(AtomicU64::new(0));
		let worker_audio_epoch = audio_epoch.clone();
		let keyframe = Arc::new(AtomicBool::new(true));
		let bitrate = Arc::new(AtomicU32::new(settings.bit_rate()));
		let worker_bitrate = bitrate.clone();
		let preview = Arc::new(Mutex::new(None));
		let worker_preview = preview.clone();
		#[cfg(target_os = "linux")]
		let status = Arc::new(Mutex::new("Choose a screen or window in the system picker"));
		#[cfg(target_os = "linux")]
		let worker_status = status.clone();
		#[cfg(target_os = "linux")]
		let preview_visible = Arc::new(AtomicBool::new(true));
		#[cfg(target_os = "linux")]
		let worker_preview_visible = preview_visible.clone();
		// A few frames of slack absorbs send jitter without forcing keyframes on every hiccup.
		let (send, frames) = tokio::sync::mpsc::channel(3);
		let (complete, done) = mpsc::sync_channel(1);
		let (audio_send, audio) = if settings.audio && audio_supported() {
			let (send, receive) = tokio::sync::mpsc::channel(4);
			(Some(send), Some(receive))
		} else {
			(None, None)
		};
		let (worker_stop, worker_ready, worker_keyframe) =
			(stop.clone(), ready.clone(), keyframe.clone());
		std::thread::Builder::new()
			.name("screen-encoder".into())
			.spawn(move || {
				let finished_ready = worker_ready.clone();
				#[cfg(target_os = "linux")]
				let result = linux::run(
					settings,
					video_settings,
					worker_stop,
					worker_ready,
					worker_keyframe,
					worker_bitrate,
					send,
					audio_send,
					worker_audio_epoch,
					worker_preview,
					worker_status,
					worker_preview_visible,
					&wake,
				);
				#[cfg(not(target_os = "linux"))]
				let result = encode_loop(
					settings,
					video_settings,
					worker_stop,
					worker_ready,
					worker_keyframe,
					worker_bitrate,
					send,
					audio_send,
					worker_audio_epoch,
					worker_preview,
					&wake,
				);
				finished_ready.store(false, Ordering::Release);
				// The desktop only shows the latest status, which a later stop overwrites.
				// Name the cause once so a share that ends by itself is never a mystery.
				if std::env::var_os("SEREIN_VOICE_DIAGNOSTICS").is_some_and(|value| value == "1") {
					match &result {
						Ok(()) => eprintln!("[Serein voice Screen] capture_stopped=ok"),
						Err(reason) => eprintln!("[Serein voice Screen] capture_stopped={reason}"),
					}
				}
				let _ = complete.try_send(result);
				wake();
			})
			.map_err(|_| "Could not start screen capture worker")?;
		Ok((
			Self {
				stop,
				ready: ready.clone(),
				preview,
				done: Some(done),
				#[cfg(target_os = "linux")]
				status,
				#[cfg(target_os = "linux")]
				preview_visible,
			},
			Video {
				video_settings,
				settings,
				frames,
				ready,
				keyframe,
				bitrate,
				audio,
				audio_epoch,
			},
		))
	}

	pub fn set_preview_visible(&self, _visible: bool) {
		#[cfg(target_os = "linux")]
		self.preview_visible.store(_visible, Ordering::Release);
	}

	pub fn capture_status(&self) -> Option<&'static str> {
		#[cfg(target_os = "linux")]
		return self.status.try_lock().ok().map(|status| *status);
		#[cfg(not(target_os = "linux"))]
		None
	}

	pub fn result(&self) -> Option<Result<(), &'static str>> {
		self.done.as_ref()?.try_recv().ok()
	}

	/// One local RGBA image, bounded to 640 by 360 pixels and replaced at most ten times a second.
	pub fn take_preview(&self) -> Option<image::RgbaImage> {
		self.preview.try_lock().ok()?.take()
	}

	pub fn shutdown(mut self) -> mpsc::Receiver<Result<(), &'static str>> {
		self.done.take().expect("screen worker completion")
	}
}

impl Drop for Worker {
	fn drop(&mut self) {
		self.ready.store(false, Ordering::Release);
		self.stop.store(true, Ordering::Release);
	}
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::too_many_arguments)] // Media outputs of one explicitly started capture.
fn encode_loop(
	settings: Settings,
	video_settings: VideoSettings,
	stop: Arc<AtomicBool>,
	ready: Arc<AtomicBool>,
	keyframe: Arc<AtomicBool>,
	bitrate: Arc<AtomicU32>,
	send: tokio::sync::mpsc::Sender<EncodedFrame>,
	audio: Option<tokio::sync::mpsc::Sender<AudioChunk>>,
	audio_epoch: Arc<AtomicU64>,
	preview: Arc<Mutex<Option<image::RgbaImage>>>,
	wake: &impl Fn(),
) -> Result<(), &'static str> {
	if stop.load(Ordering::Acquire) || send.is_closed() {
		return Ok(());
	}
	let origin = Instant::now();
	let (raw_send, raw) = mpsc::sync_channel(1);
	let capture_stop = Arc::new(AtomicBool::new(false));
	#[cfg(target_os = "windows")]
	let raw_pending = Arc::new(AtomicBool::new(false));
	#[cfg(target_os = "windows")]
	let _native = capture::Capture::start(
		settings,
		raw_send,
		audio,
		capture_stop.clone(),
		ready.clone(),
		audio_epoch,
		raw_pending.clone(),
	)?;
	// The worker's stop also reaches a pending macOS picker, so call teardown closes it now.
	#[cfg(not(target_os = "windows"))]
	let _native = capture::Capture::start(
		settings,
		raw_send,
		audio,
		capture_stop.clone(),
		ready.clone(),
		audio_epoch,
		stop.clone(),
	)?;
	let mut encoding = None;
	let mut first_frame_deadline = Some(Instant::now() + Duration::from_secs(15));
	let mut next_frame = Instant::now();
	let mut next_preview = Instant::now();
	let mut latest_frame = None;

	while !stop.load(Ordering::Acquire) && !send.is_closed() {
		#[cfg(target_os = "windows")]
		if _native.failed() {
			return Err(
				"System audio capture stopped; check your output device or share without audio",
			);
		}
		if capture_stop.load(Ordering::Acquire) {
			return Err("The selected screen or window stopped sharing");
		}
		let frame = match raw.recv_timeout(Duration::from_millis(100)) {
			Ok(frame) => {
				#[cfg(target_os = "windows")]
				raw_pending.store(false, Ordering::Release);
				Some(frame)
			}
			Err(_) if stop.load(Ordering::Acquire) || send.is_closed() => break,
			Err(mpsc::RecvTimeoutError::Timeout)
				if first_frame_deadline.is_none_or(|deadline| Instant::now() < deadline) =>
			{
				None
			}
			Err(_) => {
				return Err(
					"No screen frames received; check screen recording permission and the selected source",
				);
			}
		};
		if stop.load(Ordering::Acquire) || send.is_closed() {
			break;
		}
		let now = Instant::now();
		if let Some(frame) = &frame {
			first_frame_deadline = None;
			if now >= next_preview {
				let image = preview_frame(frame)?;
				if let Ok(mut slot) = preview.try_lock() {
					*slot = Some(image);
				}
				next_preview = now + Duration::from_millis(100);
				wake();
			}
		}
		let encode = retain_screen_frame(
			&mut latest_frame,
			frame,
			ready.load(Ordering::Acquire),
			keyframe.load(Ordering::Acquire),
		)?;
		// Local capture remains available while alone; only secure media is encoded or queued.
		if !ready.load(Ordering::Acquire) {
			encoding = None;
			keyframe.store(true, Ordering::Release);
			continue;
		}
		if !encode {
			continue;
		}
		// Pace on an accumulating schedule with a little tolerance: capture timing jitter must
		// not skip every other frame, and a stalled encoder resumes from now instead of bursting.
		let interval = Duration::from_secs_f64(1.0 / f64::from(settings.fps));
		if now + Duration::from_millis(2) < next_frame {
			continue;
		}
		next_frame = (next_frame + interval).max(now + interval / 2);
		// Drop before encoding while the transport is behind, so the encoder never references
		// a picture the receiver did not get.
		if send.capacity() == 0 {
			continue;
		}
		let target = bitrate
			.load(Ordering::Acquire)
			.clamp(250_000, settings.bit_rate());
		if encoding.is_none() {
			encoding = Some(ScreenEncoder::new(settings, target, video_settings)?);
		}
		if encoding
			.as_mut()
			.expect("secure screen encoder")
			.set_bitrate(target)?
		{
			keyframe.store(true, Ordering::Release);
		}
		// Retain one source snapshot for keyframe requests on an unchanged desktop.
		let force_keyframe = keyframe.swap(false, Ordering::AcqRel);
		let (data, is_keyframe) = encoding.as_mut().expect("secure screen encoder").encode(
			latest_frame.as_ref().expect("latest screen frame"),
			force_keyframe,
		)?;
		if force_keyframe && (data.is_empty() || !is_keyframe) {
			keyframe.store(true, Ordering::Release);
		}
		if data.is_empty() {
			continue;
		}
		if !ready.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
			continue;
		}
		let frame = EncodedFrame {
			codec: video_settings.codec,
			data,
			timestamp: (origin.elapsed().as_micros() * 90 / 1000) as u32,
			keyframe: is_keyframe,
		};
		if send.try_send(frame).is_err() {
			keyframe.store(true, Ordering::Release);
		}
	}
	capture_stop.store(true, Ordering::Release);
	Ok(())
}

pub(super) fn retain_screen_frame(
	latest: &mut Option<RawFrame>,
	frame: Option<RawFrame>,
	ready: bool,
	keyframe: bool,
) -> Result<bool, &'static str> {
	let fresh = frame.is_some();
	if let Some(frame) = frame {
		validate_frame(&frame)?;
		*latest = Some(frame);
	}
	Ok(ready && latest.is_some() && (fresh || keyframe))
}

/// Selected encoder shared by every platform; native capture remains independent.
pub(super) struct ScreenEncoder {
	diagnostics: crate::diagnostics::EncoderRegistration,
	encoder: Option<crate::video_backend::Encoder>,
	video_settings: VideoSettings,
	i420: Vec<u8>,
	settings: Settings,
	bitrate: u32,
}

impl ScreenEncoder {
	pub(super) fn new(
		settings: Settings,
		bitrate: u32,
		video_settings: VideoSettings,
	) -> Result<Self, &'static str> {
		if !settings.valid() || !video_settings.is_valid() {
			return Err("Invalid screen encoder settings");
		}
		let encoder = crate::video_backend::Encoder::new(
			Self::config(settings, bitrate, video_settings.codec),
			video_settings.backend,
		)?;
		Ok(Self {
			diagnostics: crate::diagnostics::EncoderRegistration::new(true, encoder.hardware()),
			encoder: Some(encoder),
			video_settings,
			i420: Vec::new(),
			settings,
			bitrate,
		})
	}

	fn config(settings: Settings, bitrate: u32, codec: VideoCodec) -> crate::video_encode::Config {
		crate::video_encode::Config {
			width: settings.width,
			height: settings.height,
			fps: settings.fps,
			bit_rate: bitrate.clamp(250_000, settings.bit_rate()),
			max_bytes: MAX_ENCODED_BYTES,
			profile: crate::video_encode::Profile::Main,
			codec,
		}
	}

	#[cfg(target_os = "linux")]
	pub(super) fn label(&self) -> &'static str {
		self.encoder
			.as_ref()
			.map_or("FFmpeg encoder stopped", |encoder| encoder.label())
	}

	pub(super) fn set_bitrate(&mut self, bitrate: u32) -> Result<bool, &'static str> {
		let bitrate = bitrate.clamp(250_000, self.settings.bit_rate());
		// Rate changes restart at an IDR. Bound restart frequency with the sender's
		// existing 15% reduction / 25% recovery thresholds.
		if !software_rate_change(self.bitrate, bitrate) {
			return Ok(false);
		}
		self.diagnostics.set(None);
		let config = Self::config(self.settings, bitrate, self.video_settings.codec);
		let encoder = self.encoder.as_mut().ok_or("Screen encoder stopped")?;
		encoder.reconfigure(config)?;
		self.diagnostics.set(Some(encoder.hardware()));
		self.bitrate = bitrate;
		Ok(true)
	}

	pub(super) fn encode(
		&mut self,
		frame: &RawFrame,
		force: bool,
	) -> Result<(Vec<u8>, bool), &'static str> {
		let (width, height) = (self.settings.width as usize, self.settings.height as usize);
		self.i420.resize(width * height * 3 / 2, 0);
		convert::bgra_to_i420(frame, width, height, &mut self.i420)?;
		let encoder = self
			.encoder
			.as_mut()
			.ok_or("FFmpeg screen encoder stopped")?;
		let result = encoder.encode(&self.i420, force)?;
		self.diagnostics.set(Some(encoder.hardware()));
		Ok(result)
	}
}

/// Significant bitrate moves apply without restarting on every feedback tick.
pub(crate) fn software_rate_change(current: u32, target: u32) -> bool {
	u64::from(target) * 100 <= u64::from(current) * 85
		|| u64::from(target) * 100 >= u64::from(current) * 125
}

fn validate_frame(frame: &RawFrame) -> Result<(usize, usize), &'static str> {
	let row_bytes = (frame.width as usize)
		.checked_mul(4)
		.ok_or("Screen capture returned an unsupported frame size")?;
	let required = frame
		.stride
		.checked_mul(frame.height as usize)
		.ok_or("Screen capture returned an unsupported frame size")?;
	if frame.width == 0
		|| frame.height == 0
		|| frame.width > 3840
		|| frame.height > 2160
		|| frame.data.len() > MAX_RAW_BYTES
		|| frame.stride < row_bytes
		|| required > frame.data.len()
	{
		return Err("Screen capture returned an unsupported frame size");
	}

	Ok((row_bytes, required))
}

pub(super) fn preview_frame(frame: &RawFrame) -> Result<image::RgbaImage, &'static str> {
	validate_frame(frame)?;
	let scale = (640.0 / f64::from(frame.width))
		.min(360.0 / f64::from(frame.height))
		.min(1.0);
	let width = (f64::from(frame.width) * scale).round().max(1.0) as u32;
	let height = (f64::from(frame.height) * scale).round().max(1.0) as u32;
	// ponytail: nearest sampling keeps the ten-fps preview cheap; use filtered scaling if needed.
	Ok(image::RgbaImage::from_fn(width, height, |x, y| {
		let source_x = (x * frame.width / width) as usize;
		let source_y = (y * frame.height / height) as usize;
		let offset = source_y * frame.stride + source_x * 4;
		image::Rgba([
			frame.data[offset + 2],
			frame.data[offset + 1],
			frame.data[offset],
			255,
		])
	}))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn idle_screen_keyframe_uses_latest_snapshot_only_when_ready() {
		let frame = |value| RawFrame {
			width: 2,
			height: 2,
			stride: 8,
			data: vec![value; 16],
		};
		let mut latest = None;
		assert!(!retain_screen_frame(&mut latest, None, true, true).unwrap());
		assert!(retain_screen_frame(&mut latest, Some(frame(1)), true, false).unwrap());
		assert!(!retain_screen_frame(&mut latest, None, true, false).unwrap());
		assert!(retain_screen_frame(&mut latest, None, true, true).unwrap());
		// A security pause keeps tracking the source without encoding. Its newest snapshot
		// can be freshly encoded after readiness, even if no further capture event arrives.
		assert!(!retain_screen_frame(&mut latest, Some(frame(2)), false, true).unwrap());
		assert!(!retain_screen_frame(&mut latest, None, false, true).unwrap());
		assert!(retain_screen_frame(&mut latest, None, true, true).unwrap());
		assert_eq!(latest.as_ref().unwrap().data, vec![2; 16]);
		let mut oversized = frame(3);
		oversized.width = 3841;
		assert!(retain_screen_frame(&mut latest, Some(oversized), true, true).is_err());
		assert_eq!(latest.unwrap().data, vec![2; 16]);
	}

	#[test]
	fn local_preview_is_bounded_and_converts_padded_bgra_without_media_readiness() {
		let preview = preview_frame(&RawFrame {
			width: 1,
			height: 2,
			stride: 8,
			data: vec![1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 0, 9, 9, 9, 9],
		})
		.unwrap();
		assert_eq!(preview.as_raw(), &[3, 2, 1, 255, 6, 5, 4, 255]);
		let worker = Worker {
			stop: Arc::new(AtomicBool::new(false)),
			ready: Arc::new(AtomicBool::new(false)),
			preview: Arc::new(Mutex::new(Some(preview))),
			done: None,
			#[cfg(target_os = "linux")]
			status: Arc::new(Mutex::new("")),
			#[cfg(target_os = "linux")]
			preview_visible: Arc::new(AtomicBool::new(true)),
		};
		assert!(worker.take_preview().is_some());
		assert!(worker.take_preview().is_none());
		assert!(!worker.ready.load(Ordering::Acquire));
		for (width, height) in [(3840, 2160), (2, 2160), (3840, 2)] {
			let frame = RawFrame {
				width,
				height,
				stride: width as usize * 4,
				data: vec![0; width as usize * height as usize * 4],
			};
			let preview = preview_frame(&frame).unwrap();
			assert!(preview.width() > 0 && preview.width() <= 640);
			assert!(preview.height() > 0 && preview.height() <= 360);
			assert!(preview.as_raw().len() <= 640 * 360 * 4);
		}
		assert!(
			preview_frame(&RawFrame {
				width: 2,
				height: 2,
				stride: 4,
				data: vec![0; 8],
			})
			.is_err()
		);
	}

	#[test]
	fn synthetic_frame_is_bounded_and_encodes_a_keyframe() {
		let frame = RawFrame {
			width: 2,
			height: 2,
			stride: 12,
			data: vec![255; 24],
		};
		let mut pixels = vec![0; 1280 * 720 * 3 / 2];
		convert::bgra_to_i420(&frame, 1280, 720, &mut pixels).unwrap();
		assert_eq!(pixels.len(), 1280 * 720 * 3 / 2);

		let settings = Settings {
			source: SourceId::Display(1),
			width: 1280,
			height: 720,
			fps: 30,
			cursor: true,
			audio: false,
		};
		let video_settings = VideoSettings::default();
		let encoder = crate::video_backend::Encoder::software(ScreenEncoder::config(
			settings,
			settings.bit_rate(),
			video_settings.codec,
		))
		.unwrap();
		let mut encoder = ScreenEncoder {
			diagnostics: crate::diagnostics::EncoderRegistration::new(true, false),
			encoder: Some(encoder),
			video_settings,
			i420: Vec::new(),
			settings,
			bitrate: settings.bit_rate(),
		};
		let raw = frame;
		let (encoded, keyframe) = encoder.encode(&raw, true).unwrap();
		assert!(keyframe);
		assert!(!encoded.is_empty() && encoded.len() <= MAX_ENCODED_BYTES);
		assert!(crate::video_receive::is_keyframe(&encoded));
		assert!(crate::video_receive::has_parameter_sets(&encoded));
		use openh264::formats::YUVSource;
		let mut decoder = openh264::decoder::Decoder::new().unwrap();
		assert_eq!(
			decoder.decode(&encoded).unwrap().unwrap().dimensions(),
			(1280, 720)
		);
		assert!(encoder.set_bitrate(settings.bit_rate() / 2).unwrap());
		let (encoded, keyframe) = encoder.encode(&raw, true).unwrap();
		assert!(keyframe && crate::video_receive::has_parameter_sets(&encoded));

		assert!(
			convert::bgra_to_i420(
				&RawFrame {
					width: 2,
					height: 2,
					stride: 4,
					data: vec![0; 8],
				},
				1280,
				720,
				&mut pixels,
			)
			.is_err()
		);
	}
}
