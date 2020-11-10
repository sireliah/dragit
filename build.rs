//fn main() {
//println!(r"cargo:rustc-link-search=native=C:\msys64\home\vieviurka\vcpkg\installed\x64-windows\lib");
//}

use autovcpkg;
fn main() {
    autovcpkg::configure(&["gtk", "cairo"]);
    #[cfg(target_os = "windows")]
    // vcpkg generate gtk adn gdk libraries with 3.0, where gtk-rs et al. expect only 3, we duplicate and rename them so rust will be able to find and link correctly
    autovcpkg::lib_fixup(&[("gtk-3.0.lib", "gtk-3.lib"), ("gdk-3.0.lib", "gdk-3.lib")]);
}
