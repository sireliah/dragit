use prost_build;

fn build_proto() {
    prost_build::compile_protos(&["src/p2p/discovery/host.proto"], &["src/"]).unwrap();
    prost_build::compile_protos(&["src/p2p/transfer/metadata.proto"], &["src/"]).unwrap();
}

#[cfg(target_os = "windows")]
fn main() {
    build_proto();
    println!("cargo:rustc-cdylib-link-arg=src/dragit.res");
}

#[cfg(not(target_os = "windows"))]
fn main() {
    build_proto();
}
