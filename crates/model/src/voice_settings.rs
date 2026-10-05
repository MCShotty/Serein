//! Device-local voice and video settings. Profiles preserve custom microphone settings.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoBackend {
	#[default]
	Stable,
	Experimental,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoCodec {
	#[default]
	H264,
	H265,
	Av1,
}

impl VideoCodec {
	pub const ALL: [Self; 3] = [Self::H264, Self::H265, Self::Av1];

	pub const fn index(self) -> usize {
		match self {
			Self::H264 => 0,
			Self::H265 => 1,
			Self::Av1 => 2,
		}
	}

	pub const fn key(self) -> &'static str {
		match self {
			Self::H264 => "h264",
			Self::H265 => "h265",
			Self::Av1 => "av1",
		}
	}
}

/// Hardware-only FFmpeg engines. Capability results are session-local, never preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardwareBackend {
	Nvenc,
	Amf,
	Qsv,
	VideoToolbox,
}

impl HardwareBackend {
	pub const ALL: [Self; 4] = [Self::Nvenc, Self::Amf, Self::Qsv, Self::VideoToolbox];

	pub const fn index(self) -> usize {
		match self {
			Self::Nvenc => 0,
			Self::Amf => 1,
			Self::Qsv => 2,
			Self::VideoToolbox => 3,
		}
	}

	pub const fn key(self) -> &'static str {
		match self {
			Self::Nvenc => "nvenc",
			Self::Amf => "amf",
			Self::Qsv => "qsv",
			Self::VideoToolbox => "videotoolbox",
		}
	}

	pub const fn name(self) -> &'static str {
		match self {
			Self::Nvenc => "NVIDIA NVENC",
			Self::Amf => "AMD AMF",
			Self::Qsv => "Intel Quick Sync",
			Self::VideoToolbox => "Apple VideoToolbox",
		}
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProbeResult {
	#[default]
	Pending,
	Available,
	Unavailable,
	Failed,
	TimedOut,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HardwareSupport {
	pub camera: ProbeResult,
	pub screen: ProbeResult,
}

/// Driver-advertised codec support, queried without submitting frames to an encoder.
/// A fixed codec/backend matrix bounds session-local state independently of driver output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DriverCapabilities {
	pub support: [[ProbeResult; 4]; 3],
}

impl DriverCapabilities {
	pub fn codec(&self, codec: VideoCodec) -> &[ProbeResult; 4] {
		&self.support[codec.index()]
	}

	pub fn set(&mut self, backend: HardwareBackend, codec: VideoCodec, value: ProbeResult) {
		self.support[codec.index()][backend.index()] = value;
	}

	/// Only complete negative driver reports disable a codec. Query failures stay unknown.
	pub fn confirmed_unavailable(&self, codec: VideoCodec) -> bool {
		self.codec(codec)
			.iter()
			.all(|result| *result == ProbeResult::Unavailable)
	}
}

/// Results of explicit synthetic encoder tests; distinct from driver-advertised support.
/// The fixed codec/backend matrix bounds state independently of driver output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VideoCapabilities {
	pub support: [[HardwareSupport; 4]; 3],
}

impl VideoCapabilities {
	pub fn codec(&self, codec: VideoCodec) -> &[HardwareSupport; 4] {
		&self.support[codec.index()]
	}

	pub fn set(&mut self, backend: HardwareBackend, codec: VideoCodec, value: HardwareSupport) {
		self.support[codec.index()][backend.index()] = value;
	}

	/// A complete negative test matrix; this does not override driver-reported support.
	pub fn confirmed_unavailable(&self, codec: VideoCodec) -> bool {
		self.codec(codec).iter().all(|support| {
			support.camera == ProbeResult::Unavailable && support.screen == ProbeResult::Unavailable
		})
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
	pub backend: VideoBackend,
	pub codec: VideoCodec,
}
impl VideoSettings {
	pub fn is_valid(self) -> bool {
		self.backend == VideoBackend::Experimental || self.codec == VideoCodec::H264
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputProfile {
	#[default]
	VoiceIsolation,
	Studio,
	Custom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseSuppression {
	Off,
	#[default]
	RnNoise,
	WebRtc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Processing {
	pub suppression: NoiseSuppression,
	/// WebRTC suppression strength, from low (0) through very high (3).
	pub suppression_level: u8,
	pub echo_cancellation: bool,
	pub automatic_gain: bool,
	/// None is an open microphone; otherwise a dBFS threshold with a short release hold.
	pub sensitivity_db: Option<i16>,
}
impl Default for Processing {
	fn default() -> Self {
		Self {
			suppression: NoiseSuppression::RnNoise,
			suppression_level: 2,
			echo_cancellation: true,
			automatic_gain: true,
			sensitivity_db: Some(-55),
		}
	}
}
impl Processing {
	pub fn is_valid(self) -> bool {
		self.suppression_level <= 3 && self.sensitivity_db.is_none_or(|db| (-80..=0).contains(&db))
	}
	pub fn studio() -> Self {
		Self {
			suppression: NoiseSuppression::Off,
			suppression_level: 0,
			echo_cancellation: false,
			automatic_gain: false,
			sensitivity_db: None,
		}
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceProcessing {
	pub profile: InputProfile,
	pub custom: Processing,
}
impl VoiceProcessing {
	pub fn effective(self) -> Processing {
		match self.profile {
			InputProfile::VoiceIsolation => Processing::default(),
			InputProfile::Studio => Processing::studio(),
			InputProfile::Custom => self.custom,
		}
	}
	pub fn from_legacy(noise_suppression: bool) -> Self {
		Self {
			profile: InputProfile::Custom,
			custom: Processing {
				suppression: if noise_suppression {
					NoiseSuppression::RnNoise
				} else {
					NoiseSuppression::Off
				},
				echo_cancellation: true,
				..Processing::studio()
			},
		}
	}
	/// Editing a preset starts from its visible values, rather than hidden custom values.
	pub fn edit(&mut self) -> &mut Processing {
		if self.profile != InputProfile::Custom {
			self.custom = self.effective();
		}
		self.profile = InputProfile::Custom;
		&mut self.custom
	}
}
