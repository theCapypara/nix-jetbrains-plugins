Nix Jetbrains Plugins
=====================

This repository contains derivations for ALL plugins from the Jetbrains Marketplace.

It is regularly updated to include all current plugins in their latest compatible version.

If any derivations fail to build or plugins are missing, please open an issue. 
We asume that plugins are not re-released with the same version number, so if a plugin does this for any reason, 
they might break and need manual fixing in this repository.

The plugins exported by this Flake are indexed by their IDE, version and then plugin ID. 
You can find the plugin IDs at the bottom of Marketplace pages.

The plugin list is only updated for IDEs from the current and previous year, for other IDEs the list may be stale.

## How to setup

### With Flakes

#### Inputs:

```nix
inputs.nix-jebrains-plugins.url = "github:theCapypara/nix-jebrains-plugins";
```

#### Usage:
```nix
let
  pluginList = [
    nix-jebrains-plugins.plugins."${system}".idea-ultimate."2024.3"."com.intellij.plugins.watcher"
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
      url = "https://github.com/theCapypara/nix-jebrains-plugins";
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
    pkgs.jetbrains.plugins.addPlugins pkgs.jetbrains.idea-ultimate pluginList
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
  environment.systemPackages = with nix-jebrains-plugins.lib."${system}"; [
    # Adds the latest IDEA Ultimate version with the latest compatible version of "com.intellij.plugins.watcher".
    buildIdeWithPlugins pkgs.jetbrains "idea-ultimate" ["com.intellij.plugins.watcher"]
  ];
}
```
