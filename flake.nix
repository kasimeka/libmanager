{
  nixConfig.bash-prompt-prefix = ''\[\e[0;31m\](rust) \e[0m'';

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    inputs.flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [inputs.rust-overlay.overlays.default];
        };
        rust-toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in {
        devShells.default = pkgs.mkShell {
          packages =
            (with pkgs; [openssl pkg-config cargo-hack])
            ++ [
              (rust-toolchain.override
                {extensions = ["rust-src" "rust-analyzer" "clippy"];})
            ];
        };
      }
    );
}
