fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = if std::path::Path::new("../proto/packet.proto").exists() {
        "../proto/packet.proto"
    } else {
        // Fallback or maybe we should just fail if it's missing there
        "proto/packet.proto" 
    };

    println!("cargo:rerun-if-changed={}", proto_file);
    tonic_build::compile_protos(proto_file)?;
    Ok(())
}
