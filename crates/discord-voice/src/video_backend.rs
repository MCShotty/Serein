//! Worker-owned video encoding selection. Stable retains the native H.264 encoders;
//! Experimental uses FFmpeg and the explicitly selected codec.
use crate::video_encode::{Config, Profile};
use model::voice_settings::{VideoBackend, VideoCodec};
use openh264::{
	OpenH264API,
	encoder::{
		BitRate, Complexity, EncoderConfig, FrameRate, FrameType, IntraFramePeriod,
		Profile as H264Profile, RateControlMode, UsageType,
	},
	formats::YUVSlices,
};

#[cfg(target_os = "linux")]
#[path = "video_encode_linux.rs"]
mod native;
#[cfg(target_os = "windows")]
#[path = "video_encode_windows.rs"]
mod native;
#[cfg(target_os = "macos")]
#[path = "video_encode_macos.rs"]
mod native;

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceFormat {
	Bgra,
	#[cfg_attr(not(test), allow(dead_code))]
	Rgb,
}
#[cfg(target_os = "macos")]
impl SourceFormat {
	fn bytes_per_pixel(self) -> usize {
		match self {
			Self::Bgra => 4,
			Self::Rgb => 3,
		}
	}
}

enum Implementation {
	Stable(Box<Stable>),
	Experimental(crate::video_encode::Encoder),
}

pub(crate) struct Encoder {
	implementation: Implementation,
}

impl Encoder {
	#[cfg(test)]
	pub(crate) fn software(config: Config) -> Result<Self, &'static str> {
		picture_bytes(config)?;
		if config.codec != VideoCodec::H264 {
			return Err("Stable video encoding supports H.264 only");
		}
		Ok(Self {
			implementation: Implementation::Stable(Box::new(Stable {
				hardware: None,
				software: Some(software(config)?),
				config,
				#[cfg(target_os = "macos")]
				bgra: Vec::new(),
				pending: 0,
				produced_output: false,
			})),
		})
	}

	pub(crate) fn new(config: Config, backend: VideoBackend) -> Result<Self, &'static str> {
		picture_bytes(config)?;
		let implementation = match backend {
			VideoBackend::Stable => {
				if config.codec != VideoCodec::H264 {
					return Err("Stable video encoding supports H.264 only");
				}
				Implementation::Stable(Box::new(Stable::new(config)?))
			}
			VideoBackend::Experimental => {
				Implementation::Experimental(crate::video_encode::Encoder::new(config)?)
			}
		};
		Ok(Self { implementation })
	}

	pub(crate) fn encode(
		&mut self,
		picture: &[u8],
		force: bool,
	) -> Result<(Vec<u8>, bool), &'static str> {
		match &mut self.implementation {
			Implementation::Stable(encoder) => encoder.encode(picture, force),
			Implementation::Experimental(encoder) => encoder.encode(picture, force),
		}
	}

	pub(crate) fn reconfigure(&mut self, config: Config) -> Result<(), &'static str> {
		picture_bytes(config)?;
		match &mut self.implementation {
			Implementation::Stable(encoder) => {
				if config.codec != VideoCodec::H264 {
					return Err("Stable video encoding supports H.264 only");
				}
				encoder.reconfigure(config)
			}
			Implementation::Experimental(encoder) => encoder.reconfigure(config),
		}
	}

	pub(crate) fn hardware(&self) -> bool {
		match &self.implementation {
			Implementation::Stable(encoder) => encoder.hardware.is_some(),
			Implementation::Experimental(encoder) => encoder.hardware(),
		}
	}

	#[cfg(target_os = "linux")]
	pub(crate) fn label(&self) -> &'static str {
		match &self.implementation {
			Implementation::Stable(encoder) => encoder
				.hardware
				.as_ref()
				.map_or("H.264 · Stable software encoding", |hardware| {
					hardware.label()
				}),
			Implementation::Experimental(encoder) => encoder.label(),
		}
	}
}

fn picture_bytes(config: Config) -> Result<usize, &'static str> {
	if config.width == 0
		|| config.height == 0
		|| config.width > 1920
		|| config.height > 1080
		|| !config.width.is_multiple_of(2)
		|| !config.height.is_multiple_of(2)
		|| !(1..=60).contains(&config.fps)
		|| !(1_000..=50_000_000).contains(&config.bit_rate)
		|| config.max_bytes == 0
		|| config.max_bytes > 2 * 1024 * 1024
	{
		return Err("Invalid video encoder settings");
	}
	Ok(config.width as usize * config.height as usize * 3 / 2)
}

struct Stable {
	hardware: Option<native::Encoder>,
	software: Option<openh264::encoder::Encoder>,
	config: Config,
	#[cfg(target_os = "macos")]
	bgra: Vec<u8>,
	pending: u8,
	produced_output: bool,
}

impl Stable {
	fn new(config: Config) -> Result<Self, &'static str> {
		#[cfg(target_os = "macos")]
		let hardware = native::Encoder::new(config, SourceFormat::Bgra).ok();
		#[cfg(not(target_os = "macos"))]
		let hardware = native::Encoder::new(config).ok();
		let software = hardware.is_none().then(|| software(config)).transpose()?;
		Ok(Self {
			hardware,
			software,
			config,
			#[cfg(target_os = "macos")]
			bgra: Vec::new(),
			pending: 0,
			produced_output: false,
		})
	}

	fn reconfigure(&mut self, config: Config) -> Result<(), &'static str> {
		let old = self.config;
		// Use the original native live rate setter when geometry/profile are unchanged.
		if config.width == old.width
			&& config.height == old.height
			&& config.fps == old.fps
			&& config.profile == old.profile
			&& config.max_bytes == old.max_bytes
			&& self
				.hardware
				.as_mut()
				.is_some_and(|encoder| encoder.set_bitrate(config.bit_rate).is_ok())
		{
			self.config = config;
			return Ok(());
		}
		let was_hardware = self.hardware.is_some();
		// Native Drop completes its platform teardown before another session is requested.
		self.hardware = None;
		self.software = None;
		self.config = config;
		self.pending = 0;
		self.produced_output = false;
		if was_hardware {
			#[cfg(target_os = "macos")]
			let hardware = native::Encoder::new(config, SourceFormat::Bgra).ok();
			#[cfg(not(target_os = "macos"))]
			let hardware = native::Encoder::new(config).ok();
			self.hardware = hardware;
		}
		if self.hardware.is_none() {
			self.software = Some(software(config)?);
		}
		Ok(())
	}

	fn encode(&mut self, picture: &[u8], force: bool) -> Result<(Vec<u8>, bool), &'static str> {
		if picture.len() != picture_bytes(self.config)? {
			return Err("Invalid video encoder picture");
		}
		let force = force || self.config.profile == Profile::Baseline || !self.produced_output;
		if let Some(hardware) = self.hardware.as_mut() {
			#[cfg(target_os = "linux")]
			let encoded = hardware.encode(picture, force);
			#[cfg(target_os = "windows")]
			let encoded = {
				let (y, chroma) =
					picture.split_at(self.config.width as usize * self.config.height as usize);
				let (u, v) = chroma.split_at(y.len() / 4);
				hardware.encode(y, u, v, force)
			};
			#[cfg(target_os = "macos")]
			let encoded = {
				i420_to_bgra(picture, self.config, &mut self.bgra)?;
				hardware.encode(
					&self.bgra,
					(self.config.width as usize, self.config.height as usize),
					force,
				)
			};
			if let Ok(frame) = encoded {
				if frame.0.is_empty() {
					self.pending = self.pending.saturating_add(1);
					if self.pending < 4 {
						return Ok(frame);
					}
				} else if validate_h264(&frame.0, frame.1, self.config, !self.produced_output)
					.is_ok()
				{
					self.pending = self.pending.saturating_sub(1);
					self.produced_output = true;
					return Ok(frame);
				}
			}
			// A failed platform encoder stays excluded through every bitrate change in
			// this stream. The first software picture must reset the receiver with an IDR.
			self.hardware = None;
			self.software = Some(software(self.config)?);
			self.pending = 0;
			self.produced_output = false;
		}
		let (width, height) = (self.config.width as usize, self.config.height as usize);
		let (y, chroma) = picture.split_at(width * height);
		let (u, v) = chroma.split_at(width * height / 4);
		let software = self
			.software
			.as_mut()
			.ok_or("Stable video encoder stopped")?;
		if force || !self.produced_output {
			software.force_intra_frame();
		}
		let encoded = software
			.encode(&YUVSlices::new(
				(y, u, v),
				(width, height),
				(width, width / 2, width / 2),
			))
			.map_err(|_| "Stable video encoding failed")?;
		let mut length = 0usize;
		for index in 0..encoded.num_layers() {
			let layer = encoded
				.layer(index)
				.ok_or("Stable encoder returned an invalid layer")?;
			for index in 0..layer.nal_count() {
				length = length
					.checked_add(
						layer
							.nal_unit(index)
							.ok_or("Stable encoder returned an invalid NAL")?
							.len(),
					)
					.filter(|length| *length <= self.config.max_bytes)
					.ok_or("Stable encoded video frame exceeds its limit")?;
			}
		}
		if length == 0 {
			return Ok((Vec::new(), false));
		}
		let mut frame = Vec::with_capacity(length);
		encoded.write_vec(&mut frame);
		let keyframe = matches!(encoded.frame_type(), FrameType::IDR);
		validate_h264(&frame, keyframe, self.config, !self.produced_output)?;
		self.produced_output = true;
		Ok((frame, keyframe))
	}
}

fn validate_h264(
	frame: &[u8],
	keyframe: bool,
	config: Config,
	initial: bool,
) -> Result<(), &'static str> {
	if frame.is_empty() || frame.len() > config.max_bytes {
		return Err("Stable encoded video frame exceeds its limit");
	}
	crate::video::validate_source(frame).map_err(|_| "Stable encoder returned invalid H.264")?;
	if (initial || config.profile == Profile::Baseline)
		&& (!keyframe
			|| !crate::video_receive::is_keyframe(frame)
			|| !crate::video_receive::has_parameter_sets(frame))
	{
		return Err("Stable initial or camera frame is not independently decodable");
	}
	Ok(())
}

fn software(config: Config) -> Result<openh264::encoder::Encoder, &'static str> {
	let camera = config.profile == Profile::Baseline;
	let threads = if camera {
		1
	} else {
		std::thread::available_parallelism().map_or(2, |count| count.get().clamp(2, 8) as u16)
	};
	openh264::encoder::Encoder::with_api_config(
		OpenH264API::from_source(),
		EncoderConfig::new()
			.bitrate(BitRate::from_bps(config.bit_rate))
			.max_frame_rate(FrameRate::from_hz(config.fps as f32))
			.profile(if camera {
				H264Profile::Baseline
			} else {
				H264Profile::Main
			})
			.usage_type(if camera {
				UsageType::CameraVideoRealTime
			} else {
				UsageType::ScreenContentRealTime
			})
			.rate_control_mode(RateControlMode::Bitrate)
			.complexity(if cfg!(target_os = "windows") || camera {
				Complexity::Low
			} else {
				Complexity::Medium
			})
			.num_threads(threads)
			.intra_frame_period(IntraFramePeriod::from_num_frames(if camera {
				1
			} else {
				config.fps * 2
			}))
			.debug(false),
	)
	.map_err(|_| "Stable software video encoder is unavailable")
}

#[cfg(any(target_os = "windows", test))]
fn i420_to_nv12(y: &[u8], u: &[u8], v: &[u8], output: &mut [u8]) -> Result<(), &'static str> {
	if u.len() != v.len() || y.len() != u.len() * 4 || output.len() != y.len() + u.len() + v.len() {
		return Err("Invalid video encoder color planes");
	}
	output[..y.len()].copy_from_slice(y);
	for (pair, (&u, &v)) in output[y.len()..]
		.as_chunks_mut::<2>()
		.0
		.iter_mut()
		.zip(u.iter().zip(v))
	{
		pair.copy_from_slice(&[u, v]);
	}
	Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn i420_to_bgra(picture: &[u8], config: Config, output: &mut Vec<u8>) -> Result<(), &'static str> {
	if picture.len() != picture_bytes(config)? {
		return Err("Invalid video encoder color planes");
	}
	let (width, height) = (config.width as usize, config.height as usize);
	let (y, chroma) = picture.split_at(width * height);
	let (u, v) = chroma.split_at(width * height / 4);
	output.resize(width * height * 4, 0);
	for (index, pixel) in output.as_chunks_mut::<4>().0.iter_mut().enumerate() {
		let chroma = index / width / 2 * (width / 2) + index % width / 2;
		let (c, d, e) = (
			i32::from(y[index]) - 16,
			i32::from(u[chroma]) - 128,
			i32::from(v[chroma]) - 128,
		);
		let byte = |value: i32| ((value + 128) >> 8).clamp(0, 255) as u8;
		*pixel = [
			byte(298 * c + 516 * d),
			byte(298 * c - 100 * d - 208 * e),
			byte(298 * c + 409 * e),
			255,
		];
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use openh264::formats::YUVSource;

	const CAMERA: Config = Config {
		width: 640,
		height: 480,
		fps: 15,
		bit_rate: 600_000,
		max_bytes: 128 * 1024,
		profile: Profile::Baseline,
		codec: VideoCodec::H264,
	};

	fn software_only(config: Config) -> Encoder {
		Encoder::software(config).unwrap()
	}

	#[test]
	fn stable_camera_frames_are_bounded_and_independently_decodable() {
		let mut encoder = software_only(CAMERA);
		let mut picture = vec![128; picture_bytes(CAMERA).unwrap()];
		for luma in [16, 126, 235] {
			picture[..640 * 480].fill(luma);
			let (frame, keyframe) = encoder.encode(&picture, false).unwrap();
			assert!(keyframe && frame.len() <= CAMERA.max_bytes);
			let mut decoder = openh264::decoder::Decoder::new().unwrap();
			assert_eq!(
				decoder.decode(&frame).unwrap().unwrap().dimensions(),
				(640, 480)
			);
		}
		encoder
			.reconfigure(Config {
				bit_rate: 450_000,
				..CAMERA
			})
			.unwrap();
		assert!(
			!encoder.hardware(),
			"software fallback must survive rate changes"
		);
		let (frame, keyframe) = encoder.encode(&picture, false).unwrap();
		assert!(keyframe && crate::video_receive::has_parameter_sets(&frame));
	}

	#[test]
	fn stable_screen_uses_delta_frames_and_can_restart_at_a_forced_idr() {
		let config = Config {
			profile: Profile::Main,
			..CAMERA
		};
		let mut encoder = software_only(config);
		let mut decoder = openh264::decoder::Decoder::new().unwrap();
		let mut picture = vec![128; picture_bytes(config).unwrap()];
		for index in 0..8 {
			// Preserve Stable's scene-change detection: vary one pixel, not the whole scene.
			picture[0] = 128 + index;
			let force = index == 0 || index == 5;
			let (frame, keyframe) = encoder.encode(&picture, force).unwrap();
			assert_eq!(keyframe, force);
			assert_eq!(
				decoder.decode(&frame).unwrap().unwrap().dimensions(),
				(640, 480)
			);
			if force {
				let mut fresh = openh264::decoder::Decoder::new().unwrap();
				assert_eq!(
					fresh.decode(&frame).unwrap().unwrap().dimensions(),
					(640, 480)
				);
			}
		}
	}

	#[test]
	fn stable_rejects_other_codecs_and_malformed_input_without_losing_its_encoder() {
		let mut encoder = software_only(CAMERA);
		for codec in [VideoCodec::H265, VideoCodec::Av1] {
			let config = Config { codec, ..CAMERA };
			assert!(matches!(
				Encoder::new(config, VideoBackend::Stable),
				Err("Stable video encoding supports H.264 only")
			));
			assert_eq!(
				encoder.reconfigure(config),
				Err("Stable video encoding supports H.264 only")
			);
		}
		for length in [0, 640 * 480 * 3 / 2 - 1, 640 * 480 * 3 / 2 + 1] {
			assert!(encoder.encode(&vec![0; length], true).is_err());
		}
		let picture = vec![128; picture_bytes(CAMERA).unwrap()];
		assert!(encoder.encode(&picture, true).unwrap().1);
		let mut capped = software_only(Config {
			max_bytes: 1,
			..CAMERA
		});
		assert!(capped.encode(&picture, true).is_err());
	}

	#[test]
	fn native_color_adapters_preserve_bt601_limited_range_and_plane_order() {
		let config = Config {
			width: 2,
			height: 2,
			..CAMERA
		};
		let mut bgra = Vec::new();
		for (picture, pixel) in [
			([16, 16, 16, 16, 128, 128], [0, 0, 0, 255]),
			([235, 235, 235, 235, 128, 128], [255, 255, 255, 255]),
			([81, 81, 81, 81, 90, 240], [0, 0, 255, 255]),
		] {
			i420_to_bgra(&picture, config, &mut bgra).unwrap();
			assert!(
				bgra.as_chunks::<4>()
					.0
					.iter()
					.all(|output| *output == pixel)
			);
		}
		let mut nv12 = [0; 12];
		i420_to_nv12(&[1, 2, 3, 4, 5, 6, 7, 8], &[9, 10], &[11, 12], &mut nv12).unwrap();
		assert_eq!(nv12, [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 10, 12]);
		assert!(i420_to_nv12(&[1; 4], &[2], &[3, 4], &mut nv12).is_err());
		assert!(i420_to_bgra(&[128; 5], config, &mut bgra).is_err());
	}
}
