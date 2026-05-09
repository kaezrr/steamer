use std::sync::Arc;

use clap::Parser;
use indicatif::MultiProgress;
use new_vdf_parser::open_shortcuts_vdf;
use new_vdf_parser::write_shortcuts_vdf;
use serde_json::Map;
use serde_json::Value;
use steamer::SteamGridClient;
use steamer::SteamPaths;
use steamer::asset_exists;
use steamer::asset_kind::Grid;
use steamer::asset_kind::Hero;
use steamer::asset_kind::Icon;
use steamer::asset_kind::Logo;
use steamer::choose_game;
use steamer::download_first_if_any;

macro_rules! async_if {
    ($cond:ident, $request:expr) => {
        async { if $cond { Some($request.await) } else { None } }
    };
}

#[derive(clap::Parser)]
#[command(
    name = "steamer",
    about = "Download SteamGridDB assets for your steam library automatically"
)]
struct Args {
    /// Your SteamGridDB API key
    #[arg(long)]
    api_key: String,

    /// Fetch official steam store assets
    #[arg(long, default_value_t = false)]
    official: bool,

    /// Dry run the application without making any changes
    #[arg(long, short, default_value_t = false)]
    dry_run: bool,

    /// Interactively choose which SteamGridDB game to pick
    #[arg(long, short, default_value_t = false)]
    interactive: bool,

    /// Overwrite all existing assets and refetch them
    #[arg(long, short, default_value_t = false)]
    overwrite: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let client = SteamGridClient::new(&args.api_key)?;

    let steam = steamlocate::locate()?;
    println!("Found Steam directory - {}", steam.path().display());

    let paths = SteamPaths::locate(&steam)?;
    std::fs::create_dir_all(&paths.grid)?;
    println!("Using Grid directory - {}", paths.grid.display());

    let mut shortcuts_vdf = open_shortcuts_vdf(&paths.shortcuts);

    let shortcuts = shortcuts_vdf
        .as_object_mut()
        .expect("shortcuts_vdf must be a json object");

    println!("Found {} non-steam game(s)!\n", shortcuts.len());

    for v in shortcuts.values_mut() {
        let app_name = v["AppName"].as_str().expect("AppName key");
        let app_id = v["appid"].as_u64().expect("appid key") as u32;

        let games = client.search_by_name(app_name).await?;

        let Some(game) = choose_game(&games, args.interactive) else {
            println!("No match for {app_name}\n");
            continue;
        };

        let need_grid = args.overwrite || !asset_exists::<Grid>(app_id, &paths.grid);
        let need_hero = args.overwrite || !asset_exists::<Hero>(app_id, &paths.grid);
        let need_logo = args.overwrite || !asset_exists::<Logo>(app_id, &paths.grid);
        let need_icon = args.overwrite || !asset_exists::<Icon>(app_id, &paths.grid);

        if !need_grid && !need_hero && !need_logo && !need_icon {
            println!("All assets already exist, skipping {app_name}\n");
            continue;
        }

        if args.official {
            let steam_appid = client.find_steam_appid(game.id).await?;
            println!("{steam_appid:?}");
        }

        println!("Downloading assets for: {} (app_id {})", game.name, game.id);

        let (grids, heroes, logos, icons) = tokio::join!(
            async_if!(need_grid, client.find_asset::<Grid>(game.id)),
            async_if!(need_hero, client.find_asset::<Hero>(game.id)),
            async_if!(need_logo, client.find_asset::<Logo>(game.id)),
            async_if!(need_icon, client.find_asset::<Icon>(game.id)),
        );

        let grids = grids.transpose()?;
        let heroes = heroes.transpose()?;
        let logos = logos.transpose()?;
        let icons = icons.transpose()?;

        let mp = Arc::new(MultiProgress::new());

        let (grid, hero, logo, icon) = tokio::join!(
            download_first_if_any::<Grid>(&client, grids.as_deref(), mp.clone()),
            download_first_if_any::<Hero>(&client, heroes.as_deref(), mp.clone()),
            download_first_if_any::<Logo>(&client, logos.as_deref(), mp.clone()),
            download_first_if_any::<Icon>(&client, icons.as_deref(), mp.clone()),
        );

        let grid = grid?;
        let hero = hero?;
        let logo = logo?;
        let icon = icon?;

        if let Some(g) = grid {
            g.save::<Grid>(app_id, &paths.grid)?;
        }
        if let Some(h) = hero {
            h.save::<Hero>(app_id, &paths.grid)?;
        }
        if let Some(l) = logo {
            l.save::<Logo>(app_id, &paths.grid)?;
        }
        if let Some(i) = icon {
            v["icon"] = Value::String(i.save::<Icon>(app_id, &paths.grid)?);
        }

        println!("\n\n");
    }

    println!("Updating shortcuts.vdf with icon data...");
    let mut vdf_to_write = Value::Object(Map::new());
    vdf_to_write["shortcuts"] = shortcuts_vdf;

    write_shortcuts_vdf(&paths.shortcuts, vdf_to_write);
    println!("Done! All assets were saved at {}", paths.grid.display());

    Ok(())
}
