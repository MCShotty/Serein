//! Single-worker FFmpeg H.264 encoding for camera and screen sharing.
//! Only NVENC, VideoToolbox and the LGPL build's OpenH264 encoder are admitted.
#![allow(unsafe_code)] // Small, checked ABI to the owned libavcodec context in the C shim.

use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};

#[derive(Clone, Copy)]
pub(crate) struct Config {
	pub width: u32,
	pub height: u32,
	pub fps: u32,
	pub bit_rate: u32,
	pub max_bytes: usize,
	pub profile: Profile,
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
}

unsafe extern "C" {
	fn serein_avc_open(
		width: i32,
		height: i32,
		fps: i32,
		bitrate: i32,
		baseline: i32,
		backend: i32,
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
}

impl Encoder {
	pub(crate) fn new(config: Config) -> Result<Self, &'static str> {
		config.picture_bytes()?;
		#[cfg(any(target_os = "linux", target_os = "windows"))]
		if let Ok(encoder) = Self::open(config, Backend::Nvenc) {
			return Ok(encoder);
		}
		#[cfg(target_os = "macos")]
		if let Ok(encoder) = Self::open(config, Backend::VideoToolbox) {
			return Ok(encoder);
		}
		Self::software(config)
	}

	pub(crate) fn software(config: Config) -> Result<Self, &'static str> {
		Self::open(config, Backend::Software)
	}

	fn open(config: Config, backend: Backend) -> Result<Self, &'static str> {
		config.picture_bytes()?;
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
				config.max_bytes,
			)
		};
		let pointer = NonNull::new(pointer).ok_or("FFmpeg H.264 encoder is unavailable")?;
		Ok(Self {
			native: Some(Native(pointer, PhantomData)),
			config,
			backend,
			output: vec![0; config.max_bytes],
			in_flight: 0,
		})
	}

	pub(crate) fn hardware(&self) -> bool {
		self.backend != Backend::Software
	}
	#[cfg(target_os = "linux")]
	pub(crate) fn label(&self) -> &'static str {
		match self.backend {
			Backend::Software => "H.264 · FFmpeg software encoding",
			#[cfg(any(target_os = "linux", target_os = "windows"))]
			Backend::Nvenc => "H.264 · FFmpeg NVENC hardware encoding",
			#[cfg(target_os = "macos")]
			Backend::VideoToolbox => "H.264 · FFmpeg VideoToolbox hardware encoding",
		}
	}

	/// Encodes tightly packed I420. A rejected hardware frame switches permanently to
	/// FFmpeg software encoding and begins a fresh independently decodable sequence.
	pub(crate) fn encode(
		&mut self,
		picture: &[u8],
		force: bool,
	) -> Result<(Vec<u8>, bool), &'static str> {
		if picture.len() != self.config.picture_bytes()? {
			return Err("Invalid FFmpeg video encoder picture");
		}
		match self.encode_native(picture, force) {
			Ok(frame) => Ok(frame),
			Err(error) if self.hardware() => {
				// Release scarce hardware resources before opening the replacement.
				self.native = None;
				*self = Self::open(self.config, Backend::Software).map_err(|_| error)?;
				self.encode_native(picture, true)
			}
			Err(error) => Err(error),
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
		crate::video::validate_source(data).map_err(|_| "FFmpeg returned invalid H.264")?;
		let keyframe = keyframe != 0 && crate::video_receive::is_keyframe(data);
		if self.config.profile == Profile::Baseline
			&& (!keyframe || !crate::video_receive::has_parameter_sets(data))
		{
			return Err("FFmpeg camera frame was not independently decodable");
		}
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
	};
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
