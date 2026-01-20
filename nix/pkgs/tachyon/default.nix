{
  craneLib,
  cargoArtifacts,
  pkgs,
  ...
}: let
  src = craneLib.cleanCargoSource (craneLib.path ../../../.);
  commonArgs = {
    inherit src;

    strictDeps = true;

    buildInputs = [
      pkgs.luajit
    ];

    nativeBuildInputs = [
      pkgs.pkg-config
    ];
  };
in
  craneLib.buildPackage (commonArgs
    // {
      inherit cargoArtifacts;
      pname = "tachyon";
      doCheck = false;
      CARGO_PROFILE = "release";
    })
