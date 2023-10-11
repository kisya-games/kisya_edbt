{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      utils,
      rust-overlay,
      ...
    }:
    utils.lib.eachSystem [ "aarch64-linux" "x86_64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Bevy's runtime stack: the egui example and the tests open a real app,
        # so the windowing/audio/GPU libs must be present.
        runtimeLibs = with pkgs; [
          alsa-lib
          libudev-zero
          vulkan-loader
          libxkbcommon
          libX11
          libXcursor
          libXrandr
          libXi
          freetype
          fontconfig
          expat
          libGL
          wayland
          stdenv.cc.cc.lib
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = runtimeLibs;
          nativeBuildInputs = with pkgs; [
            (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
            pkg-config
            taplo
            rust-analyzer-unwrapped
          ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
        };
      }
    );
}
