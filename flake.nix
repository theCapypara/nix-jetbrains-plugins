{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    flake-utils.inputs.systems.follows = "systems";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
      flake-utils,
    }:
    flake-utils.lib.eachSystem (import systems) (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
      in
      rec {
        plugins = pkgs.callPackage ./plugins.nix { };

        packages = {
          _nix-jebrains-plugins-generator = pkgs.callPackage ./generator/pkg.nix { };
        };

        devShells = {
          default = pkgs.callPackage ./dev.nix { };
        };

        lib = {
          # Using this function you can build an IDE using a set of named plugins from this Flake. The function
          # will automatically figure out what IDE and version the plugin needs to be for.
          # See README.
          buildIdeWithPlugins =
            jetbrains: ide-or-name: plugin-ids:
            let
              ide = if builtins.typeOf ide-or-name == "string" then jetbrains."${ide-or-name}" else ide-or-name;
            in
            jetbrains.plugins.addPlugins ide (
              builtins.map (p: plugins."${ide.pname}"."${ide.version}"."${p}") plugin-ids
            );
        };
      }
    );
}
