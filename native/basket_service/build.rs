fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Vendored protoc keeps the build self-contained: no system protobuf-compiler required.
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    println!("cargo:rerun-if-changed=proto/basket.proto");
    tonic_prost_build::configure().compile_protos(&["proto/basket.proto"], &["proto"])?;

    Ok(())
}
