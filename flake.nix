{
  description = "Rust cli pomodoro";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        defaultPackage = pkgs.rustPlatform.buildRustPackage rec {
          pname = "rust_cli_pomodoro";
          version = "1.4.5-rc.2";
          src = ./.;
          cargoSha256 = "sha256-YvAKA3bNXQJj0BTovshlom20nr1FonDAS6XoaJpcwtM=";
          buildInputs = [pkgs.openssl];
          nativeBuildInputs = [pkgs.cargo pkgs.rustc pkgs.pkg-config pkgs.openssl];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [pkgs.openssl];

          buildPhase = ''
            cargo build --release
          '';

          installPhase = ''
            mkdir -p $out/bin
            cp target/release/pomodoro $out/bin
            cp target/release/daemon $out/bin
          '';
        };

        devShell = pkgs.mkShell {
          buildInputs = [pkgs.openssl];
          nativeBuildInputs = [pkgs.cargo pkgs.rustc pkgs.pkg-config pkgs.openssl];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [pkgs.openssl];
        };

        apps.rust_cli_pomodoro = {
          type = "app";
          program = "${self.defaultPackage.${system}}/bin/pomodoro";
        };
      }
    );
}
