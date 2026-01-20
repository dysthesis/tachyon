{
  craneLib,
  cargoArtifacts,
  commonArgs,
  ...
}:
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    pname = "tachyon";
    CARGO_PROFILE = "release";
  }
)
