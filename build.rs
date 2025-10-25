use std::path::Path;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	// If someone runs `rustc build.rs` directly, Cargo's build-dependencies are not available.
	if std::env::var("CARGO_MANIFEST_DIR").is_err() {
		eprintln!("This build script must be run by Cargo (use `cargo build`), not `rustc build.rs`.");
		std::process::exit(1);
	}

	// If the PROTOC env var is set, prost/tonic will use that. Otherwise ensure `protoc` is on PATH.
	if std::env::var_os("PROTOC").is_none() {
		match std::process::Command::new("protoc").arg("--version").output() {
			Ok(output) if output.status.success() => {
				// protoc present, continue
			}
			_ => {
				eprintln!(
					"protoc was not found. Install the protobuf compiler or set the PROTOC env var.\n\
					- On Debian/Ubuntu: sudo apt-get install protobuf-compiler\n\
					- Or add a vendored protoc build-dependency (e.g. protoc-bin-vendored) and set PROTOC\n\
					See: https://docs.rs/prost-build/#sourcing-protoc"
				);
				std::process::exit(1);
			}
		}
	}

	// compile your proto(s)
	for p in ["./protos/encryption.proto", "./protos/election.proto"].iter() {
		tonic_prost_build::compile_protos(p)?;
	}

	// Ensure Cargo rebuilds when proto changes.
	println!("cargo:rerun-if-changed=./protos/encryption.proto");
	println!("cargo:rerun-if-changed=./protos/election.proto");
	println!("cargo:rerun-if-changed=./protos");

	// Copy the generated files from OUT_DIR to a stable location inside src/.
	// This prevents IDEs / rust-analyzer from choking when OUT_DIR is not set.
	let out_dir = std::env::var("OUT_DIR")?;
	let dest_dir = Path::new("src").join("generated");
	fs::create_dir_all(&dest_dir)?;

	for fname in &["encryption.rs", "election.rs"] {
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
