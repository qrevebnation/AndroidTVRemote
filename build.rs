use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=proto/polo.proto");
    println!("cargo:rerun-if-changed=proto/remotemessage.proto");

    prost_build::Config::new()
        .compile_protos(
            &["proto/polo.proto", "proto/remotemessage.proto"],
            &["proto/"],
        )
        .expect("Failed to compile protobuf files");

    if Path::new("ui/appwindow.slint").exists() {
        println!("cargo:rerun-if-changed=ui/appwindow.slint");
        slint_build::compile("ui/appwindow.slint").expect("Failed to compile Slint template");
    }
}
