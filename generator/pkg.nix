{
  rustPlatform,
  cargo,
  rustc,
  pkg-config,
  openssl,
}:
rustPlatform.buildRustPackage {
  pname = "nix-jebrains-plugins-generator";
  version = "0.1.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
    cargo
    rustc
    openssl
  ];

  buildInputs = [
    openssl
  ];

  meta = {
    mainProgram = "nix-jebrains-plugins-generator";
  };
}
