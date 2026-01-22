{
  description = "Nix testing utilities";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    inputs@{
      self,
      flake-utils,
      nixpkgs,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
        };

        nix-tests-rust = pkgs.rustPlatform.buildRustPackage {
          pname = "nix-tests";
          version = "0.1.0";
          src = ./.;
          doCheck = false;
          cargoLock.lockFile = ./Cargo.lock;
        };

        nix-tests = pkgs.writeShellApplication {
          name = "nix-tests";
          text = ''
            export NIX_TESTS_LIB_PATH="${self}/lib/tests.nix"
            ${nix-tests-rust}/bin/nix-tests "$@"
          '';
        };
      in
      {
        packages.default = nix-tests;
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.clippy
            pkgs.rustfmt
            pkgs.ripgrep
          ];

          shellHook = ''
            export NIX_TESTS_LIB_PATH="$PWD/lib/tests.nix"
          '';
        };
      }
    )
    // {
      overlays.default = final: prev: {
        nix-tests = self.packages.${prev.system}.default;
      };
    };
}
