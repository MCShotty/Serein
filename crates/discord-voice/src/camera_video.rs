//! Outgoing camera RTP. Unofficial Discord video signaling; live interoperability is unverified.
//! Signaling reference: https://github.com/dank074/Discord-video-stream/blob/master/src/client/voice/BaseMediaConnection.ts
//! Uses the same bounded codec packetization as Go Live.
use crate::crypto::Encryption;
use model::voice_settings::{VideoCodec, VideoSettings};
use serde_json::{Value, json};
use tokio::time::Instant;

pub const MAX_FRAME_BYTES: usize = crate::camera::MAX_ENCODED_BYTES;
const MAX_PACKETS: usize = crate::video::MAX_FRAGMENTS;
const MAX_WIRE_BYTES: usize = MAX_PACKETS * 1200;

pub struct Frame {
	pub generation: u64,
	pub timestamp: u32,
	pub codec: VideoCodec,
	pub data: Vec<u8>,
}

pub(crate) struct Sender {
	codec: VideoCodec,
	ssrc: u32,
	rtx: u32,
	sequence: u16,
	pub negotiated: bool,
	pub generation: u64,
	pub announced: bool,
	pacer: crate::video::Pacer,
	dimensions: (u32, u32),
	bitrate: u32,
	frame_rate: u32,
	max_dave_bytes: usize,
}
impl Default for Sender {
	fn default() -> Self {
		Self::new(VideoSettings::default())
	}
}
impl Sender {
	pub fn new(settings: VideoSettings) -> Self {
		Self {
			codec: settings.codec,
			ssrc: 0,
			rtx: 0,
			sequence: 0,
			negotiated: false,
			generation: 0,
			announced: false,
			pacer: crate::video::Pacer::new(),
			dimensions: settings.camera_resolution.camera_dimensions(),
			bitrate: crate::camera::bit_rate(
				settings.camera_resolution,
				settings.camera_frame_rate,
			),
			frame_rate: settings.camera_frame_rate.fps(),
			max_dave_bytes: crate::camera::encoded_limit(settings.camera_resolution) + 64 * 1024,
		}
	}
	pub fn configure(&mut self, data: &Value, audio: u32) {
		// Request one stream and accept only that exact assignment, never guessed SSRCs.
		let Some(stream) = data["streams"]
			.as_array()
			.filter(|s| s.len() <= 4)
			.and_then(|s| s.iter().find(|s| s["type"] == "video" && s["rid"] == "100"))
		else {
			return;
		};
		let ssrc = stream["ssrc"]
			.as_u64()
			.and_then(|s| u32::try_from(s).ok())
			.unwrap_or(0);
		let rtx = stream["rtx_ssrc"]
			.as_u64()
			.and_then(|s| u32::try_from(s).ok())
			.unwrap_or(0);
		if ssrc != 0 && rtx != 0 && ssrc != audio && rtx != audio && ssrc != rtx {
			self.ssrc = ssrc;
			self.rtx = rtx;
		}
	}
	pub fn available(&self) -> bool {
		self.negotiated && self.ssrc != 0
	}
	pub fn announcement(&self, audio: u32, enabled: bool) -> Value {
		json!({"op":12,"d":{"audio_ssrc":audio,"video_ssrc":if enabled {self.ssrc} else {0},"rtx_ssrc":if enabled {self.rtx} else {0},"streams":if enabled {vec![json!({"type":"video","rid":"100","ssrc":self.ssrc,"rtx_ssrc":self.rtx,"active":true,"quality":100,"max_bitrate":self.bitrate,"max_framerate":self.frame_rate,"max_resolution":{"type":"fixed","width":self.dimensions.0,"height":self.dimensions.1}})]}else{vec![]}}})
	}
	pub fn clear(&mut self) {
		self.pacer = crate::video::Pacer::new();
	}
	pub fn is_empty(&self) -> bool {
		self.pacer.is_empty()
	}
	pub fn deadline(&self) -> Instant {
		self.pacer.deadline
	}
	pub fn stale(&self, now: Instant) -> bool {
		self.pacer.stale(now)
	}
	pub fn next_batch(
		&mut self,
		now: Instant,
		encryption: &mut Encryption,
	) -> Result<Vec<Vec<u8>>, &'static str> {
		self.pacer
			.next_batch(now, self.bitrate)
			.map(|packet| encryption.seal(&packet.header, &packet.payload))
			.collect()
	}

	pub fn packetize(
		&mut self,
		frame: &[u8],
		timestamp: u32,
		now: Instant,
	) -> Result<(), &'static str> {
		if frame.len() > self.max_dave_bytes || !self.pacer.is_empty() {
			return Err("Camera frame exceeds the media budget");
		}
		let mut sequence = self.sequence;
		let packets = crate::video::packetize_for_codec(
			frame,
			self.codec,
			&mut sequence,
			timestamp,
			self.ssrc,
			true,
		)?;
		if packets.len() > MAX_PACKETS {
			return Err("Camera packet queue exceeds its budget");
		}
		let mut bytes = 0;
		for packet in &packets {
			// RTP header, XChaCha tag and nonce trailer; each datagram stays <=1200 bytes.
			bytes += packet.header.len() + packet.payload.len() + 20;
			if bytes > MAX_WIRE_BYTES {
				return Err("Camera packet bytes exceed their budget");
			}
		}
		self.sequence = sequence;
		self.pacer.queue(packets, now);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn camera_packetization_is_bounded_and_marks_only_the_last_fragment() {
		let mut sender = Sender::default();
		let mut crypto = Encryption::new(&[7; 32]);
		let mut frame = vec![0, 0, 0, 1, 0x65];
		frame.extend(vec![9; 2400]);
		let now = Instant::now();
		sender.packetize(&frame, 6000, now).unwrap();
		let packets = sender.next_batch(now, &mut crypto).unwrap();
		assert_eq!(packets.len(), 3);
		for (i, packet) in packets.iter().enumerate() {
			assert!(packet.len() <= 1200);
			assert_eq!(packet[1] & 0x80 != 0, i == 2);
			assert_eq!(&packet[4..8], &6000u32.to_be_bytes());
		}
		sender.clear();
		let over_budget = sender.max_dave_bytes + 1;
		assert!(sender.packetize(&vec![0; over_budget], 0, now).is_err());
		assert!(sender.packetize(&[0, 0, 1], 0, now).is_err());
	}
	#[test]
	fn failed_packet_budget_does_not_queue_a_partial_camera_frame() {
		let mut sender = Sender::default();
		let mut frame = Vec::new();
		for _ in 0..MAX_PACKETS + 1 {
			frame.extend([0, 0, 0, 1, 0x65, 7]);
		}
		assert!(sender.packetize(&frame, 0, Instant::now()).is_err());
		assert!(sender.is_empty());
		assert_eq!(sender.sequence, 0);
	}
	#[test]
	fn camera_packetizer_enforces_the_selected_frame_budget() {
		use model::voice_settings::VideoResolution;
		for resolution in [
			VideoResolution::P480,
			VideoResolution::P720,
			VideoResolution::P4320,
		] {
			let mut sender = Sender::new(VideoSettings {
				camera_resolution: resolution,
				..VideoSettings::default()
			});
			let limit = crate::camera::encoded_limit(resolution) + 64 * 1024;
			let mut frame = vec![0, 0, 0, 1, 0x65];
			frame.resize(limit + 1, 9);
			assert_eq!(
				sender.packetize(&frame, 0, Instant::now()),
				Err("Camera frame exceeds the media budget")
			);
			assert!(sender.is_empty());
			frame.pop();
			assert!(sender.packetize(&frame, 0, Instant::now()).is_ok());
		}
	}
	#[test]
	fn camera_announces_the_selected_resolution_and_rate() {
		use model::voice_settings::{VideoFrameRate, VideoResolution};
		for resolution in VideoResolution::ALL {
			for frame_rate in VideoFrameRate::ALL {
				let sender = Sender::new(VideoSettings {
					camera_resolution: resolution,
					camera_frame_rate: frame_rate,
					..VideoSettings::default()
				});
				let announcement = sender.announcement(1, true);
				let stream = &announcement["d"]["streams"][0];
				let (width, height) = resolution.camera_dimensions();
				assert_eq!(stream["max_resolution"]["width"], width);
				assert_eq!(stream["max_resolution"]["height"], height);
				assert_eq!(
					stream["max_bitrate"],
					crate::camera::bit_rate(resolution, frame_rate)
				);
				assert_eq!(stream["max_framerate"], frame_rate.fps());
			}
		}
	}

	#[test]
	fn high_resolution_camera_is_paced_above_the_old_fixed_packet_rate() {
		use model::voice_settings::VideoResolution;
		let mut sender = Sender::new(VideoSettings {
			camera_resolution: VideoResolution::P4320,
			..VideoSettings::default()
		});
		let now = Instant::now();
		let mut crypto = Encryption::new(&[7; 32]);
		let mut frame = vec![0, 0, 0, 1, 0x65];
		frame.extend(vec![9; 300_000]);
		sender.packetize(&frame, 6000, now).unwrap();
		assert!(sender.next_batch(now, &mut crypto).unwrap().len() <= 4);
		let packets = sender
			.next_batch(now + std::time::Duration::from_millis(2), &mut crypto)
			.unwrap();
		assert!(packets.len() > 1);
		assert!(packets.iter().all(|packet| packet.len() <= 1200));
		sender.clear();
		assert!(sender.is_empty());
		assert!(
			sender
				.next_batch(now + std::time::Duration::from_secs(1), &mut crypto)
				.unwrap()
				.is_empty()
		);
	}
}
