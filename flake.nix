{
  description = "Nix testing utilities";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    inputs@{
      self,
      flake-utils,
      nixpkgs,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        nix-tests = pkgs.writeShellApplication {
          name = "nix-tests";
          runtimeInputs = with pkgs; [
            coreutils
            gawk
            gnugrep
            gnused
            ripgrep
          ];
          text = ''
            NIX_TESTS_LIB_PATH="${self}/lib/tests.nix"
            ${builtins.readFile ./scripts/nix-tests.sh}
          '';
        };
      in
      {
        packages.default = nix-tests;
        devShells.default = pkgs.mkShell {
          packages = [ nix-tests ];
        };
      }
    )
    // {
      overlays.default = final: prev: {
        nix-tests = self.packages.${prev.system}.default;
      };
    };
}
