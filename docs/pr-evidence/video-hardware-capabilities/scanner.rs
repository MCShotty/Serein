#[path = "/workspace/Serein/apps/desktop/src/video_capabilities.rs"]
mod video_capabilities;
fn main() {
    if let Some(code) = video_capabilities::probe_command() { std::process::exit(code); }
    let mut detector = video_capabilities::Detector::default();
    let mut ui = ui::MessagingUi::default();
    ui.preview_settings("voice");
    ui.video_settings.backend = model::voice_settings::VideoBackend::Experimental;
    ui.video_capabilities_refresh = true;
    let ctx = eframe::egui::Context::default();
    let start = std::time::Instant::now();
    loop {
        detector.poll(false, &mut ui, &ctx);
        if !ui.video_capabilities_loading { break; }
        assert!(start.elapsed().as_secs() < 95, "scan exceeded fixed per-helper deadlines");
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    println!("Elapsed: {:?}", start.elapsed());
    let report = ui.video_capabilities.expect("completed report");
    for codec in model::voice_settings::VideoCodec::ALL {
        for &backend in discord_voice::video_capabilities::backends() {
            println!("{} {} {:?}", backend.key(), codec.key(), report.codec(codec)[backend.index()]);
        }
    }
}
