{
  # Snowfall Lib provides a customized `lib` instance with access to your flake's library
  # as well as the libraries available from your flake's inputs.
  # You also have access to your flake's inputs.

  # The namespace used for your flake, defaulting to "internal" if not set.

  # All other arguments come from NixPkgs. You can use `pkgs` to pull packages or helpers
  # programmatically or you may add the named attributes as arguments here.
  pkgs,
  ...
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "sinh-x-pomodoro";
  version = "1.5.4";
  src = ../..;
  cargoSha256 = "sha256-G1qKx4HlgCGx9H3hW2Fs9jnC787z7MnwvABqpgT18OA=";
  buildInputs = with pkgs; [
    cargo
    openssl
    pkg-config
    rustc
    rustfmt
  ];

  nativeBuildInputs = [
    pkgs.pkg-config
    pkgs.openssl
  ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.openssl ];

  buildPhase = ''
    cargo build --release
  '';

  installPhase = ''
    mkdir -p $out/bin
    cp target/release/pomodoro $out/bin
  '';
}
