{
  description = "jeru - project scaffolding tool";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        packages.default = pkgs.callPackage ./nix/package.nix { };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          buildInputs = [ pkgs.rust-analyzer pkgs.clippy ];
        };
      });
}
