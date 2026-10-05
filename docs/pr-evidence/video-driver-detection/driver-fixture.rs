					// Explicit synthetic driver/test results; no driver or encoder is invoked here.
					use model::voice_settings::{DriverCapabilities, HardwareBackend, HardwareSupport, ProbeResult, VideoCapabilities};
					let mode = args.iter().find_map(|arg| arg.strip_prefix("--capabilities=")).unwrap_or("unknown");
					let mut report = DriverCapabilities { support: [[ProbeResult::Unavailable; 4]; 3] };
					match mode {
						"hardware" => {
							for codec in [VideoCodec::H264, VideoCodec::H265] {
								report.set(HardwareBackend::Nvenc, codec, ProbeResult::Available);
								report.set(HardwareBackend::Amf, codec, ProbeResult::Available);
							}
							report.set(HardwareBackend::Amf, VideoCodec::Av1, ProbeResult::Available);
						},
						"none" => {},
						"checking" => {
							report = DriverCapabilities::default();
							messaging.video_capabilities_loading = true;
						},
						"unknown" => { report.support = [[ProbeResult::Failed; 4]; 3]; },
						_ => return Err("Invalid synthetic capability mode".into()),
					}
					messaging.video_capabilities = Some(report);
					let test = args.iter().find_map(|arg| arg.strip_prefix("--encoder-test=")).unwrap_or("not-tested");
					match test {
						"not-tested" => {},
						"passed" | "failed" => {
							let mut tests = VideoCapabilities::default();
							let codec = messaging.video_settings.codec;
							for backend in HardwareBackend::ALL {
								tests.set(backend, codec, HardwareSupport { camera: ProbeResult::Unavailable, screen: ProbeResult::Unavailable });
							}
							if test == "passed" {
								tests.set(HardwareBackend::Nvenc, codec, HardwareSupport { camera: ProbeResult::Available, screen: ProbeResult::Available });
								tests.set(HardwareBackend::Amf, codec, HardwareSupport { camera: ProbeResult::Unavailable, screen: ProbeResult::Available });
							}
							messaging.video_encoder_tests = Some(tests);
							messaging.video_encoder_tests_codec = Some(codec);
						},
						"running" => {
							messaging.video_encoder_tests_loading = true;
							messaging.video_encoder_tests_codec = Some(messaging.video_settings.codec);
						},
						_ => return Err("Invalid synthetic encoder-test mode".into()),
					}
