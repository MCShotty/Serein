fn main() {
	println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
	println!("cargo:rerun-if-changed=src/video_encode_ffmpeg.c");
	println!("cargo:rerun-if-changed=src/video_encode_ffmpeg.h");
	let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
	if target == "linux" {
		// Test executables also link static OpenH264 from hashed Rust rlibs.
		// They must not export it into host GStreamer's different native ABI.
		println!("cargo:rustc-link-arg=-Wl,--exclude-libs,ALL");
		println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/lib");
		println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/serein");
	} else if target == "macos" {
		println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
	}
	let prefix = std::env::var_os("FFMPEG_DIR").map(std::path::PathBuf::from);
	let mut native = cc::Build::new();
	native.file("src/video_encode_ffmpeg.c").std("c11");
	if let Some(prefix) = &prefix {
		assert!(
			prefix.join("include/libavcodec/avcodec.h").is_file(),
			"Build the LGPL FFmpeg libraries with python3 scripts/build-ffmpeg.py, then set FFMPEG_DIR to its prefix"
		);
		native.include(prefix.join("include"));
		native.compile("serein_avc");
		println!(
			"cargo:rustc-link-search=native={}",
			prefix.join("lib").display()
		);
		println!("cargo:rustc-link-lib=dylib=avcodec-serein");
		println!("cargo:rustc-link-lib=dylib=avutil-serein");
		if matches!(target.as_str(), "linux" | "macos") {
			println!(
				"cargo:rustc-link-arg=-Wl,-rpath,{}",
				prefix.join("lib").display()
			);
		}
	} else {
		let codec = pkg_config::Config::new()
			.atleast_version("61")
			.cargo_metadata(false)
			.probe("libavcodec-serein")
			.expect("FFmpeg 7+ development libraries required; see scripts/build-ffmpeg.py");
		for include in &codec.include_paths {
			native.include(include);
		}
		native.compile("serein_avc");
		pkg_config::Config::new()
			.atleast_version("61")
			.probe("libavcodec-serein")
			.unwrap();
		pkg_config::Config::new()
			.atleast_version("59")
			.probe("libavutil-serein")
			.unwrap();
	}
	if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
		// Dependency link-args do not propagate to this crate's test executables.
		println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
	}
}
