{
  description = "Development environment for Dragit";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        overrides = (builtins.fromTOML (builtins.readFile (self + "/rust-toolchain.toml")));
      in
      {
        devShells.default = pkgs.mkShell rec {
          nativeBuildInputs = with pkgs; [
            pkg-config
            protobuf
          ];

          buildInputs = with pkgs; [
            # Rust toolchain
            rustup

            # GTK3 and related libraries (mirrors the apt deps in release.yml)
            gtk3
            glib
            gdk-pixbuf
            pango
            atk
            cairo

            # D-Bus (required by zbus)
            dbus

            # Needed for pnet raw socket support
            libpcap
          ];

          RUSTC_VERSION = overrides.toolchain.channel;

          shellHook = ''
            export PATH=$PATH:''${CARGO_HOME:-~/.cargo}/bin
            export PATH=$PATH:''${RUSTUP_HOME:-~/.rustup}/toolchains/$RUSTC_VERSION-x86_64-unknown-linux-gnu/bin/
            rustup toolchain install $RUSTC_VERSION
            rustup default $RUSTC_VERSION
          '';

          # Make pkg-config able to find all the GTK libs
          PKG_CONFIG_PATH = with pkgs; lib.makeSearchPathOutput "dev" "lib/pkgconfig" [
            gtk3
            glib.dev
            gdk-pixbuf
            pango
            atk
            cairo
            dbus
          ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
          RUST_BACKTRACE = 1;
        };
      }
    );
}
