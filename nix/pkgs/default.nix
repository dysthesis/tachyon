{
  perSystem = {
    craneLib,
    pkgs,
    commonArgs,
    cargoArtifacts,
    ...
  }: let
    inherit (pkgs) callPackage;
  in {
    packages = rec {
      tachyon = callPackage ./tachyon {
        inherit
          craneLib
          pkgs
          commonArgs
          cargoArtifacts
          ;
      };
      default = tachyon;
    };
  };
}
