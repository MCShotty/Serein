//! Single-worker FFmpeg H.264, H.265 and AV1 encoding for camera and screen sharing.
//! Hardware backends preserve the selected codec; OpenH264 is the H.264 fallback.
#![allow(unsafe_code)] // Small, checked ABI to the owned libavcodec context in the C shim.

use model::voice_settings::VideoCodec;
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};

#[derive(Clone, Copy)]
pub(crate) struct Config {
	pub width: u32,
	pub height: u32,
	pub fps: u32,
	pub bit_rate: u32,
	pub max_bytes: usize,
	pub profile: Profile,
	pub codec: VideoCodec,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
	Baseline,
	Main,
}

impl Config {
	fn picture_bytes(self) -> Result<usize, &'static str> {
		if self.width == 0
			|| self.height == 0
			|| self.width > 1920
			|| self.height > 1080
			|| !self.width.is_multiple_of(2)
			|| !self.height.is_multiple_of(2)
			|| !(1..=60).contains(&self.fps)
			|| !(1_000..=50_000_000).contains(&self.bit_rate)
			|| self.max_bytes == 0
			|| self.max_bytes > 2 * 1024 * 1024
		{
			return Err("Invalid FFmpeg video encoder settings");
		}
		Ok(self.width as usize * self.height as usize * 3 / 2)
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
enum Backend {
	Software = 0,
	#[cfg(any(target_os = "linux", target_os = "windows"))]
	Nvenc = 1,
	#[cfg(target_os = "macos")]
	VideoToolbox = 2,
	#[cfg(any(target_os = "linux", target_os = "windows"))]
	Amf = 3,
	#[cfg(any(target_os = "linux", target_os = "windows"))]
	Qsv = 4,
}

const BACKENDS: &[Backend] = &[
	#[cfg(any(target_os = "linux", target_os = "windows"))]
	Backend::Nvenc,
	#[cfg(any(target_os = "linux", target_os = "windows"))]
	Backend::Amf,
	#[cfg(any(target_os = "linux", target_os = "windows"))]
	Backend::Qsv,
	#[cfg(target_os = "macos")]
	Backend::VideoToolbox,
	Backend::Software,
];

/// Keep failed GPUs out of this stream's remaining attempts, including after
/// a backend opens successfully but rejects its first actual picture.
fn try_backends<T>(
	codec: VideoCodec,
	after: Option<Backend>,
	mut open: impl FnMut(Backend) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
	let start = after.map_or(0, |backend| {
		BACKENDS
			.iter()
			.position(|candidate| *candidate == backend)
			.map_or(BACKENDS.len(), |index| index + 1)
	});
	let mut error = "FFmpeg H.264 encoder is unavailable";
	for backend in &BACKENDS[start..] {
		if *backend == Backend::Software && codec != VideoCodec::H264 {
			continue;
		}
		#[cfg(target_os = "macos")]
		if *backend == Backend::VideoToolbox && codec == VideoCodec::Av1 {
			continue;
		}
		match open(*backend) {
			Ok(encoder) => return Ok(encoder),
			Err(failure) => error = failure,
		}
	}
	Err(match codec {
		VideoCodec::H264 => error,
		VideoCodec::H265 => "FFmpeg H.265 requires a compatible hardware encoder",
		VideoCodec::Av1 => "FFmpeg AV1 requires a compatible hardware encoder",
	})
}

unsafe extern "C" {
	fn serein_avc_open(
		width: i32,
		height: i32,
		fps: i32,
		bitrate: i32,
		baseline: i32,
		backend: i32,
		codec: i32,
		max_bytes: usize,
	) -> *mut c_void;
	fn serein_avc_close(context: *mut c_void);
	fn serein_avc_encode(
		context: *mut c_void,
		picture: *const u8,
		length: usize,
		force: i32,
		output: *mut u8,
		capacity: usize,
		length_out: *mut usize,
		keyframe_out: *mut i32,
	) -> i32;
}

/// Owns one FFmpeg context, frame and packet on the capture worker's thread.
struct Native(NonNull<c_void>, PhantomData<Rc<()>>);
impl Drop for Native {
	fn drop(&mut self) {
		// SAFETY: This is the sole owner; the C shim frees all three allocations once.
		unsafe { serein_avc_close(self.0.as_ptr()) };
	}
}

pub(crate) struct Encoder {
	native: Option<Native>,
	config: Config,
	backend: Backend,
	output: Vec<u8>,
	in_flight: u8,
	produced_output: bool,
}

impl Encoder {
	pub(crate) fn new(config: Config) -> Result<Self, &'static str> {
		config.picture_bytes()?;
		try_backends(config.codec, None, |backend| Self::open(config, backend))
	}

	#[cfg(test)]
	pub(crate) fn software(config: Config) -> Result<Self, &'static str> {
		Self::open(config, Backend::Software)
	}

	/// Restart at the active backend when bitrate changes. Earlier failed GPUs
	/// stay excluded; a software fallback stays software until the stream stops.
	pub(crate) fn reconfigure(&mut self, config: Config) -> Result<(), &'static str> {
		config.picture_bytes()?;
		if config.codec != self.config.codec {
			return Err("Restart video to change its codec");
		}
		let active = self.backend;
		self.native = None;
		self.backend = Backend::Software;
		self.config = config;
		*self = Self::open(config, active).or_else(|_| {
			try_backends(config.codec, Some(active), |backend| {
				Self::open(config, backend)
			})
		})?;
		Ok(())
	}

	fn open(config: Config, backend: Backend) -> Result<Self, &'static str> {
		config.picture_bytes()?;
		if backend == Backend::Software && config.codec != VideoCodec::H264 {
			return Err("FFmpeg H.265/AV1 software encoding is not bundled");
		}
		// SAFETY: All scalar bounds are checked above; the returned allocation is uniquely
		// owned here and is never sent across workers. A failed open returns null.
		let pointer = unsafe {
			serein_avc_open(
				config.width as i32,
				config.height as i32,
				config.fps as i32,
				config.bit_rate as i32,
				i32::from(config.profile == Profile::Baseline),
				backend as i32,
				match config.codec {
					VideoCodec::H264 => 0,
					VideoCodec::H265 => 1,
					VideoCodec::Av1 => 2,
				},
				config.max_bytes,
			)
		};
		let pointer = NonNull::new(pointer).ok_or(match config.codec {
			VideoCodec::H264 => "FFmpeg H.264 encoder is unavailable",
			VideoCodec::H265 => "FFmpeg H.265 hardware encoder is unavailable",
			VideoCodec::Av1 => "FFmpeg AV1 hardware encoder is unavailable",
		})?;
		Ok(Self {
			native: Some(Native(pointer, PhantomData)),
			config,
			backend,
			output: vec![0; config.max_bytes],
			in_flight: 0,
			produced_output: false,
		})
	}

	pub(crate) fn hardware(&self) -> bool {
		self.backend != Backend::Software
	}
	#[cfg(target_os = "linux")]
	pub(crate) fn label(&self) -> &'static str {
		if self.config.codec != VideoCodec::H264 {
			return match (self.config.codec, self.backend) {
				(VideoCodec::H265, Backend::Nvenc) => "H.265 · FFmpeg NVENC hardware encoding",
				(VideoCodec::H265, Backend::Amf) => "H.265 · FFmpeg AMD AMF hardware encoding",
				(VideoCodec::H265, Backend::Qsv) => "H.265 · FFmpeg Intel QSV hardware encoding",
				(VideoCodec::Av1, Backend::Nvenc) => "AV1 · FFmpeg NVENC hardware encoding",
				(VideoCodec::Av1, Backend::Amf) => "AV1 · FFmpeg AMD AMF hardware encoding",
				(VideoCodec::Av1, Backend::Qsv) => "AV1 · FFmpeg Intel QSV hardware encoding",
				_ => "FFmpeg encoder unavailable",
			};
		}
		match self.backend {
			Backend::Software => "H.264 · FFmpeg software encoding",
			#[cfg(any(target_os = "linux", target_os = "windows"))]
			Backend::Nvenc => "H.264 · FFmpeg NVENC hardware encoding",
			#[cfg(any(target_os = "linux", target_os = "windows"))]
			Backend::Amf => "H.264 · FFmpeg AMD AMF hardware encoding",
			#[cfg(any(target_os = "linux", target_os = "windows"))]
			Backend::Qsv => "H.264 · FFmpeg Intel QSV hardware encoding",
			#[cfg(target_os = "macos")]
			Backend::VideoToolbox => "H.264 · FFmpeg VideoToolbox hardware encoding",
		}
	}

	/// Encodes tightly packed I420. A rejected GPU advances through the remaining
	/// hardware backends, then H.264 permanently uses software for this stream.
	pub(crate) fn encode(
		&mut self,
		picture: &[u8],
		force: bool,
	) -> Result<(Vec<u8>, bool), &'static str> {
		if picture.len() != self.config.picture_bytes()? {
			return Err("Invalid FFmpeg video encoder picture");
		}
		// FFmpeg 7.1's AMF wrapper does not forward forced picture types. A fresh
		// Main session produces an IDR with parameter sets on explicit requests.
		// Camera GOP 1 already guarantees this without restarting every picture.
		#[cfg(any(target_os = "linux", target_os = "windows"))]
		if force
			&& self.produced_output
			&& self.backend == Backend::Amf
			&& self.config.profile == Profile::Main
		{
			self.native = None;
			match Self::open(self.config, Backend::Amf) {
				Ok(replacement) => *self = replacement,
				Err(_) => {
					self.backend = Backend::Software;
					*self = try_backends(self.config.codec, Some(Backend::Amf), |backend| {
						Self::open(self.config, backend)
					})?;
				}
			}
		}
		let mut force = force;
		loop {
			match self.encode_native(picture, force) {
				Ok(frame) => return Ok(frame),
				Err(_) if self.hardware() => {
					let failed = self.backend;
					self.native = None;
					self.backend = Backend::Software;
					*self = try_backends(self.config.codec, Some(failed), |backend| {
						Self::open(self.config, backend)
					})?;
					force = true;
				}
				Err(error) => return Err(error),
			}
		}
	}

	fn encode_native(
		&mut self,
		picture: &[u8],
		force: bool,
	) -> Result<(Vec<u8>, bool), &'static str> {
		let native = self.native.as_ref().ok_or("FFmpeg video encoder stopped")?;
		let mut length = 0usize;
		let mut keyframe = 0i32;
		// SAFETY: The input and reusable output are valid for their exact lengths; output
		// scalars are initialized locals. The shim retains no borrowed Rust pointers.
		let result = unsafe {
			serein_avc_encode(
				native.0.as_ptr(),
				picture.as_ptr(),
				picture.len(),
				i32::from(force),
				self.output.as_mut_ptr(),
				self.output.len(),
				&mut length,
				&mut keyframe,
			)
		};
		self.in_flight = self.in_flight.saturating_add(1);
		if result == 0 && self.in_flight < 6 {
			return Ok((Vec::new(), false));
		}
		if result != 1 || length == 0 || length > self.config.max_bytes {
			return Err("FFmpeg video encoding failed or exceeded its frame limit");
		}
		self.in_flight = self.in_flight.saturating_sub(1);
		let data = &self.output[..length];
		crate::video::validate_source_for_codec(data, self.config.codec)
			.map_err(|_| "FFmpeg returned an invalid video bitstream")?;
		let keyframe =
			keyframe != 0 && crate::video::is_keyframe_for_codec(data, self.config.codec);
		if (self.config.profile == Profile::Baseline || !self.produced_output)
			&& (!keyframe || !crate::video::has_parameter_sets_for_codec(data, self.config.codec))
		{
			return Err("FFmpeg initial or camera frame was not independently decodable");
		}
		self.produced_output = true;
		Ok((data.to_vec(), keyframe))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	const CAMERA: Config = Config {
		width: 640,
		height: 480,
		fps: 15,
		bit_rate: 600_000,
		max_bytes: 128 * 1024,
		profile: Profile::Baseline,
		codec: VideoCodec::H264,
	};
	#[test]
	fn failed_backends_advance_once_without_revisiting_a_gpu() {
		let mut attempts = Vec::new();
		let selected = try_backends(VideoCodec::H264, None, |backend| {
			attempts.push(backend);
			if backend == Backend::Software {
				Ok(backend)
			} else {
				Err("unavailable GPU")
			}
		})
		.unwrap();
		assert!(selected == Backend::Software);
		assert!(attempts == BACKENDS);
		for (index, failed) in BACKENDS.iter().enumerate() {
			attempts.clear();
			let result: Result<(), _> = try_backends(VideoCodec::H264, Some(*failed), |backend| {
				attempts.push(backend);
				Err("failed to open")
			});
			assert!(result.is_err());
			assert!(attempts == BACKENDS[index + 1..]);
		}
	}
	#[cfg(any(target_os = "linux", target_os = "windows"))]
	#[test]
	fn hybrid_gpu_selection_continues_to_intel_after_amd_frame_failure() {
		let mut attempts = Vec::new();
		let amd = try_backends(VideoCodec::H264, None, |backend| {
			attempts.push(backend);
			if backend == Backend::Amf {
				Ok(backend)
			} else {
				Err("GPU unavailable")
			}
		})
		.unwrap();
		assert!(amd == Backend::Amf && attempts == [Backend::Nvenc, Backend::Amf]);
		attempts.clear();
		let intel = try_backends(VideoCodec::H264, Some(amd), |backend| {
			attempts.push(backend);
			Ok(backend)
		})
		.unwrap();
		assert!(intel == Backend::Qsv && attempts == [Backend::Qsv]);
		let software = try_backends(VideoCodec::H264, Some(intel), Ok).unwrap();
		assert!(software == Backend::Software);
		assert!(try_backends(VideoCodec::H264, Some(software), Ok).is_err());
	}
	#[test]
	fn newer_codecs_never_select_h264_software_fallback() {
		for codec in [VideoCodec::H265, VideoCodec::Av1] {
			let mut attempts = Vec::new();
			let result: Result<(), _> = try_backends(codec, None, |backend| {
				attempts.push(backend);
				Err("hardware unavailable")
			});
			assert!(result.is_err());
			assert!(!attempts.contains(&Backend::Software));
			assert!(Encoder::software(Config { codec, ..CAMERA }).is_err());
			#[cfg(target_os = "macos")]
			if codec == VideoCodec::Av1 {
				assert!(attempts.is_empty());
			}
		}
	}
	#[test]
	fn malformed_picture_keeps_existing_native_context_and_backend() {
		let mut encoder = Encoder::software(CAMERA).unwrap();
		let pointer = encoder.native.as_ref().unwrap().0;
		// Simulate each selected GPU using a real software allocation. Rejected
		// input must never reach native encoding or trigger device selection.
		for backend in BACKENDS {
			encoder.backend = *backend;
			assert!(encoder.encode(&[0; 3], true).is_err());
			assert!(encoder.backend == *backend);
			assert!(encoder.native.as_ref().unwrap().0 == pointer);
			assert!(!encoder.produced_output && encoder.in_flight == 0);
		}
		encoder.backend = Backend::Software;
		let (packet, keyframe) = encoder
			.encode(&vec![128; CAMERA.picture_bytes().unwrap()], true)
			.unwrap();
		assert!(keyframe && !packet.is_empty());
	}
	#[test]
	fn bitrate_restart_keeps_software_fallback_and_starts_with_an_idr() {
		let mut encoder = Encoder::software(CAMERA).unwrap();
		let picture = vec![128; CAMERA.picture_bytes().unwrap()];
		assert!(encoder.encode(&picture, true).unwrap().1);
		encoder
			.reconfigure(Config {
				bit_rate: 450_000,
				..CAMERA
			})
			.unwrap();
		assert!(encoder.backend == Backend::Software && !encoder.produced_output);
		let (packet, keyframe) = encoder.encode(&picture, false).unwrap();
		assert!(keyframe && crate::video_receive::has_parameter_sets(&packet));
	}
	#[test]
	fn ffmpeg_software_camera_is_bounded_and_independently_decodable() {
		use openh264::formats::YUVSource;
		let mut encoder = Encoder::open(CAMERA, Backend::Software).unwrap();
		for length in [0, 640 * 480 * 3 / 2 - 1, 640 * 480 * 3 / 2 + 1] {
			assert!(encoder.encode(&vec![0; length], true).is_err());
		}
		for luma in [16, 126, 235] {
			let mut picture = vec![128; 640 * 480 * 3 / 2];
			picture[..640 * 480].fill(luma);
			let (data, keyframe) = encoder.encode(&picture, true).unwrap();
			assert!(keyframe && data.len() <= CAMERA.max_bytes);
			let mut decoder = openh264::decoder::Decoder::new().unwrap();
			assert_eq!(
				decoder.decode(&data).unwrap().unwrap().dimensions(),
				(640, 480)
			);
		}
	}
	#[test]
	fn ffmpeg_rejects_unbounded_configuration_before_native_allocation() {
		for config in [
			Config { width: 0, ..CAMERA },
			Config {
				width: 1922,
				..CAMERA
			},
			Config {
				height: 481,
				..CAMERA
			},
			Config { fps: 61, ..CAMERA },
			Config {
				max_bytes: 2 * 1024 * 1024 + 1,
				..CAMERA
			},
		] {
			assert!(Encoder::new(config).is_err());
		}
	}

	#[test]
	fn ffmpeg_screen_delta_frames_and_forced_idr_decode() {
		use openh264::formats::YUVSource;
		let config = Config {
			profile: Profile::Main,
			..CAMERA
		};
		let mut encoder = Encoder::software(config).unwrap();
		let mut decoder = openh264::decoder::Decoder::new().unwrap();
		let mut picture = vec![128; config.picture_bytes().unwrap()];
		for index in 0..8 {
			picture[..640 * 480].fill(16 + index * 20);
			let forced = index == 0 || index == 5;
			let (data, keyframe) = encoder.encode(&picture, forced).unwrap();
			assert_eq!(keyframe, forced);
			assert_eq!(
				decoder.decode(&data).unwrap().unwrap().dimensions(),
				(640, 480)
			);
			if forced {
				let mut fresh = openh264::decoder::Decoder::new().unwrap();
				assert_eq!(
					fresh.decode(&data).unwrap().unwrap().dimensions(),
					(640, 480)
				);
			}
		}
		let mut capped = Encoder::software(Config {
			max_bytes: 1,
			..config
		})
		.unwrap();
		assert!(capped.encode(&picture, true).is_err());
	}
}
