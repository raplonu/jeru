{ config, lib, pkgs, ... }:

let
  cfg = config.programs.jeru;
in
{
  options.programs.jeru = {
    enable = lib.mkEnableOption "jeru, a project scaffolding tool";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalExpression "pkgs.callPackage ./package.nix { }";
      description = "The jeru package to use.";
    };

    enableBashIntegration = lib.mkEnableOption "Bash completion for jeru" // {
      default = true;
    };

    enableZshIntegration = lib.mkEnableOption "Zsh completion for jeru" // {
      default = true;
    };

    enableFishIntegration = lib.mkEnableOption "Fish completion for jeru" // {
      default = true;
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    programs.bash.initExtra = lib.mkIf cfg.enableBashIntegration ''
      source <(${lib.getExe cfg.package} completions bash)
    '';

    programs.zsh.initContent = lib.mkIf cfg.enableZshIntegration ''
      source <(${lib.getExe cfg.package} completions zsh)
    '';

    programs.fish.interactiveShellInit = lib.mkIf cfg.enableFishIntegration ''
      ${lib.getExe cfg.package} completions fish | source
    '';
  };
}
