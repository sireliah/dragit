#[cfg(target_os = "windows")]
fn main() {
    autovcpkg::configure(&["gtk", "cairo"]);
    autovcpkg::lib_fixup(&[("gtk-3.0.lib", "gtk-3.lib"), ("gdk-3.0.lib", "gdk-3.lib")]);
}

#[cfg(not(target_os = "windows"))]
fn main() {}
