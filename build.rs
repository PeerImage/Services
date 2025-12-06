use std::path::Path;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	// If someone runs `rustc build.rs` directly, Cargo's build-dependencies are not available.
	if std::env::var("CARGO_MANIFEST_DIR").is_err() {
		eprintln!("This build script must be run by Cargo (use `cargo build`), not `rustc build.rs`.");
		std::process::exit(1);
	}

	// Use vendored protoc if PROTOC env var is not set
	if std::env::var_os("PROTOC").is_none() {
		let protoc_path = protoc_bin_vendored::protoc_bin_path()
			.map_err(|e| format!("Failed to get vendored protoc path: {}", e))?;
		// Safety: This is safe in build.rs as it runs in a single-threaded context before main compilation
		unsafe {
			std::env::set_var("PROTOC", &protoc_path);
		}
		println!("cargo:warning=Using vendored protoc from: {:?}", protoc_path);
	}

	// compile your proto(s)
	for p in ["./protos/encryption.proto", "./protos/election.proto", "./protos/directoryofservice.proto"].iter() {
		tonic_prost_build::compile_protos(p)?;
	}

	// Ensure Cargo rebuilds when proto changes.
	println!("cargo:rerun-if-changed=./protos/encryption.proto");
	println!("cargo:rerun-if-changed=./protos/election.proto");
	println!("cargo:rerun-if-changed=./protos/directoryofservice.proto");
	println!("cargo:rerun-if-changed=./protos");

	// Copy the generated files from OUT_DIR to a stable location inside src/.
	// This prevents IDEs / rust-analyzer from choking when OUT_DIR is not set.
	let out_dir = std::env::var("OUT_DIR")?;
	let dest_dir = Path::new("src").join("generated");
	fs::create_dir_all(&dest_dir)?;

	for fname in &["encryption.rs", "election.rs", "directoryofservice.rs"] {
		let generated_src = Path::new(&out_dir).join(fname);
		let dest = dest_dir.join(fname);
		// If the generated file exists in OUT_DIR, copy it; else ignore (build will fail later if necessary).
		if generated_src.exists() {
			fs::copy(&generated_src, &dest).map_err(|e| {
				format!(
					"failed to copy generated proto file from {:?} to {:?}: {}",
					generated_src, dest, e
				)
			})?;
		}
	}

	Ok(())
}
