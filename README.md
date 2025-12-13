Nix Jetbrains Plugins
=====================

This repository contains derivations for ALL plugins from the Jetbrains Marketplace.

It is regularly updated to include all current plugins in their latest compatible version.

If any derivations fail to build or plugins are missing, please open an issue.

The plugins exported by this Flake are indexed by their IDE, version and then plugin ID. 
You can find the plugin IDs at the bottom of Marketplace pages.

The plugin list is only updated for IDEs from the current year, as well as the last minor 
release line from the previous year, for other IDEs the list may be stale.

Supported IDEs:
- IntelliJ IDEA (`jetbrains.idea`, `jetbrains.idea-oss`*)
- PhpStorm (`jetbrains.phpstorm`)
- WebStorm (`jetbrains.webstorm`)
- PyCharm (`jetbrains.pycharm`, `jetbrains.pycharm-oss`*)
- RubyMine (`jetbrains.ruby-mine`)
- CLion (`jetbrains.clion`)
- GoLand (`jetbrains.goland`)
- DataGrip (`jetbrains.datagrip`)
- DataSpell (`jetbrains.dataspell`)
- Rider (`jetbrains.rider`)
- Android Studio (`android-studio`)
- RustRover (`jetbrains.rust-rover`)
- Mps (`jetbrains.mps`)
- Android Studio

Supported legacy IDEs:
- PyCharm Community & Professional (`pycharm-community`, `pycharm-professional`)
- IntelliJ IDEA Community & Ultimate (`idea-community`, `idea-ultimate`)
- Aqua (`jetbrains.aqua`)
- Writerside (`jetbrains.writerside`)

## How to setup

### With Flakes

#### Inputs:

```nix
inputs.nix-jetbrains-plugins.url = "github:theCapypara/nix-jetbrains-plugins";
```

#### Usage:
```nix
let
  pluginList = [
    nix-jetbrains-plugins.plugins."${system}".idea-ultimate."2024.3"."com.intellij.plugins.watcher"
  ];
in {
  # ... see "How to use"
}
```

### Without flakes

```nix
let
  system = builtins.currentSystem;
  plugins =
    (import (builtins.fetchGit {
      url = "https://github.com/theCapypara/nix-jetbrains-plugins";
      ref = "refs/heads/main";
      rev = "<latest commit hash>";
    })).plugins."${system}";
  pluginList = [
      plugins.idea-ultimate."2024.3"."com.intellij.plugins.watcher"
  ];
in {
  # ... see "How to use"
}
```

## How to use

The plugins can be used with ``jetbrains.plugins.addPlugins``:

```nix
{
  environment.systemPackages = [
    # See "How to setup" for definition of `pluginList`.
    (pkgs.jetbrains.plugins.addPlugins pkgs.jetbrains.idea-ultimate pluginList)
  ];
}
```

## Convenience functions (`lib`)
The flake exports some convenience functions that can be used to make adding plugins to your IDEs
easier.

These functions are only compatible and tested with the latest stable nixpkgs version.

### `buildIdeWithPlugins`

Using this function you can build an IDE using a set of named plugins from this Flake. The function
will automatically figure out what IDE and version the plugin needs to be for.

#### Arguments:

1. `pkgs.jetbrains` from nixpkgs.
2. The `pkgs.jetbrains` key of the IDE to build or download.
3. A list of plugin IDs to install.

#### Example:

```nix
{
  environment.systemPackages = with nix-jetbrains-plugins.lib."${system}"; [
    # Adds the latest IDEA Ultimate version with the latest compatible version of "com.intellij.plugins.watcher".
    (buildIdeWithPlugins pkgs.jetbrains "idea-ultimate" ["com.intellij.plugins.watcher"])
  ];
}
```
