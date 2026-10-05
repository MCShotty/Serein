//! Outgoing camera RTP. Unofficial Discord video signaling; live interoperability is unverified.
//! Signaling reference: https://github.com/dank074/Discord-video-stream/blob/master/src/client/voice/BaseMediaConnection.ts
//! Uses the same bounded codec packetization as Go Live.
use crate::crypto::Encryption;
use model::voice_settings::VideoCodec;
use serde_json::{Value, json};
use std::collections::VecDeque;

pub const MAX_FRAME_BYTES: usize = 128 * 1024;
const MAX_PACKETS: usize = 256;

pub struct Frame {
	pub generation: u64,
	pub timestamp: u32,
	pub codec: VideoCodec,
	pub data: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct Sender {
	codec: VideoCodec,
	ssrc: u32,
	rtx: u32,
	sequence: u16,
	pub negotiated: bool,
	pub generation: u64,
	pub announced: bool,
	packets: VecDeque<Vec<u8>>,
}
impl Sender {
	pub fn new(codec: VideoCodec) -> Self {
		Self {
			codec,
			..Self::default()
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
		json!({"op":12,"d":{"audio_ssrc":audio,"video_ssrc":if enabled {self.ssrc} else {0},"rtx_ssrc":if enabled {self.rtx} else {0},"streams":if enabled {vec![json!({"type":"video","rid":"100","ssrc":self.ssrc,"rtx_ssrc":self.rtx,"active":true,"quality":100,"max_bitrate":600_000,"max_framerate":15,"max_resolution":{"type":"fixed","width":640,"height":480}})]}else{vec![]}}})
	}
	pub fn clear(&mut self) {
		self.packets.clear();
	}
	pub fn is_empty(&self) -> bool {
		self.packets.is_empty()
	}
	pub fn next(&mut self) -> Option<Vec<u8>> {
		self.packets.pop_front()
	}
	pub fn packetize(
		&mut self,
		frame: &[u8],
		timestamp: u32,
		encryption: &mut Encryption,
	) -> Result<(), &'static str> {
		if frame.len() > MAX_FRAME_BYTES + 1024 || !self.packets.is_empty() {
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
		let mut wire = VecDeque::with_capacity(packets.len());
		let mut bytes = 0;
		for packet in packets {
			let packet = encryption.seal(&packet.header, &packet.payload)?;
			bytes += packet.len();
			if bytes > MAX_FRAME_BYTES + 64 * 1024 {
				return Err("Camera packet bytes exceed their budget");
			}
			wire.push_back(packet);
		}
		self.sequence = sequence;
		self.packets = wire;
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
		sender.packetize(&frame, 6000, &mut crypto).unwrap();
		assert_eq!(sender.packets.len(), 3);
		for (i, packet) in sender.packets.iter().enumerate() {
			assert!(packet.len() <= 1200);
			assert_eq!(packet[1] & 0x80 != 0, i == 2);
			assert_eq!(&packet[4..8], &6000u32.to_be_bytes());
		}
		sender.clear();
		assert!(
			sender
				.packetize(&vec![0; MAX_FRAME_BYTES + 1025], 0, &mut crypto)
				.is_err()
		);
		assert!(sender.packetize(&[0, 0, 1], 0, &mut crypto).is_err());
	}
	#[test]
	fn failed_packet_budget_does_not_queue_a_partial_camera_frame() {
		let mut sender = Sender::default();
		let mut crypto = Encryption::new(&[7; 32]);
		let mut frame = Vec::new();
		for _ in 0..MAX_PACKETS + 1 {
			frame.extend([0, 0, 0, 1, 0x65, 7]);
		}
		assert!(sender.packetize(&frame, 0, &mut crypto).is_err());
		assert!(sender.is_empty());
		assert_eq!(sender.sequence, 0);
	}
}
