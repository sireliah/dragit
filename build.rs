use prost_build;

fn build_proto() {
    prost_build::compile_protos(&["src/p2p/discovery/host.proto"], &["src/"]).unwrap();
    prost_build::compile_protos(&["src/p2p/transfer/metadata.proto"], &["src/"]).unwrap();
}

#[cfg(target_os = "windows")]
fn main() {
    build_proto();
    autovcpkg::configure(&["gtk", "cairo"]);
    autovcpkg::lib_fixup(&[("gtk-3.0.lib", "gtk-3.lib"), ("gdk-3.0.lib", "gdk-3.lib")]);
}

#[cfg(not(target_os = "windows"))]
fn main() {
    build_proto();
}
