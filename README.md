# steamer

A CLI-tool to automatically fetch and download SteamGridDB assets for your non-steam games.  
Also supports fetching official steam store assets with the `--official` flag.  
Downloads missing icon, grid, hero and logo for each game. Will preserve existing assets by default but can be overridden by the `--overwrite` flag.

Tested on linux, other platforms untested but should work.

## Installation

You can download the [latest release](https://github.com/kaezrr/steamer/releases/latest) of the tool directly from github.

Alternatively you can install it via `cargo`, the Rust package manager:

```sh
cargo install --git https://github.com/kaezrr/steamer.git
```

## Usage

You will need a SteamGridDB API key in order to use this tool, you can get it by creating an account on [SteamGridDB](https://www.steamgriddb.com/) and then going to [preferences > api](https://www.steamgriddb.com/profile/preferences/api)

```sh
Usage: steamer [OPTIONS] --api-key <API_KEY>

Options:
      --api-key <API_KEY>  Your SteamGridDB API key
      --official           Fetch official steam store assets
  -d, --dry-run            Dry run the application without making any changes
  -i, --interactive        Interactively choose which SteamGridDB game to pick
  -o, --overwrite          Overwrite all existing assets and refetch them
  -c, --clean              Clean the grid directory of all assets
  -h, --help               Print help
```

By default it always picks the first match for icons, heroes, grids and logos.

## Possible Improvements

- Extend it to work on normal steam games
- ~Add the option to preserve existing steamgrid assets instead of always overwriting~
- Add configuration file to save api key and other configuration options for covers, heroes, etc
- Integrate OS file events and add a `--watch` so that it runs automatically in the background efficiently
- ~Possible further parallelization improvements to make it work even faster~
