					// Explicit synthetic capabilities fixture, never GPU detection evidence.
					use model::voice_settings::{HardwareBackend, HardwareSupport, ProbeResult, VideoCapabilities};
					let mode = args.iter().find_map(|arg| arg.strip_prefix("--capabilities=")).unwrap_or("unknown");
					let mut report = VideoCapabilities { support: [[HardwareSupport { camera: ProbeResult::Unavailable, screen: ProbeResult::Unavailable }; 4]; 3] };
					match mode {
						"hardware" => {
							for codec in [VideoCodec::H264, VideoCodec::H265] {
								report.set(HardwareBackend::Nvenc, codec, HardwareSupport { camera: ProbeResult::Available, screen: ProbeResult::Available });
								report.set(HardwareBackend::Amf, codec, HardwareSupport { camera: ProbeResult::Unavailable, screen: ProbeResult::Available });
							}
							report.set(HardwareBackend::Amf, VideoCodec::Av1, HardwareSupport { camera: ProbeResult::Available, screen: ProbeResult::Available });
						},
						"none" => {},
						"checking" => { messaging.video_capabilities_loading = true; },
						"unknown" => { report.support = [[HardwareSupport { camera: ProbeResult::Failed, screen: ProbeResult::TimedOut }; 4]; 3]; },
						_ => return Err("Invalid synthetic capability mode".into()),
					}
					messaging.video_capabilities = Some(report);
