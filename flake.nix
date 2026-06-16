{
  description = "jeru - project scaffolding tool";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      overlay = final: prev: {
        jeru = final.callPackage ./nix/package.nix { };
      };
    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        packages.default = pkgs.callPackage ./nix/package.nix { };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          buildInputs = [ pkgs.rust-analyzer pkgs.clippy ];
        };
      })
    // {
      overlays.default = overlay;

      homeManagerModules.jeru = import ./nix/hm-module.nix;
      homeManagerModules.default = self.homeManagerModules.jeru;
    };
}
