fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No system protoc in CI or the dev container, so use the vendored binary.
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["proto/basket.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/basket.proto");
    Ok(())
}
