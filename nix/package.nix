{ rustPlatform, lib, makeWrapper, mutagen, tmux }:

rustPlatform.buildRustPackage {
  pname = "jeru";
  version = "0.1.0";

  src = lib.cleanSource ../.;

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [ makeWrapper ];

  postInstall = ''
    wrapProgram $out/bin/jeru \
      --prefix PATH : ${lib.makeBinPath [ mutagen tmux ]}
  '';

  meta = {
    description = "Project scaffolding tool";
    mainProgram = "jeru";
    license = lib.licenses.mit;
  };
}
