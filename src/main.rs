use clap::Parser;
use comfy_table::Cell;
use comfy_table::ContentArrangement;
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL;
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use futures_util::stream;
use new_vdf_parser::open_shortcuts_vdf;
use serde_json::Value;
use steamer::Asset;
use steamer::SteamGridClient;
use steamer::SteamPaths;
use steamer::asset_exists;
use steamer::asset_kind::Grid;
use steamer::asset_kind::Hero;
use steamer::asset_kind::Icon;
use steamer::asset_kind::Logo;
use steamer::choose_game;

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
    let app = App::build(args)?;

    let plan = app.build_plan().await?;
    print_plan(&plan);

    Ok(())
}

struct App {
    args: Args,
    paths: SteamPaths,
    client: SteamGridClient,
    shortcuts_vdf: Value,
}

impl App {
    fn build(args: Args) -> anyhow::Result<Self> {
        let client = SteamGridClient::new(&args.api_key)?;

        let steam = steamlocate::locate()?;
        println!("Found Steam directory - {}", steam.path().display());

        let paths = SteamPaths::locate(&steam)?;
        std::fs::create_dir_all(&paths.grid)?;
        println!("Using Grid directory - {}", paths.grid.display());

        let shortcuts_vdf = open_shortcuts_vdf(&paths.shortcuts);

        Ok(Self {
            args,
            paths,
            client,
            shortcuts_vdf,
        })
    }

    async fn build_plan(&self) -> anyhow::Result<Vec<Plan>> {
        let shortcuts = self
            .shortcuts_vdf
            .as_object()
            .expect("shortcuts_vdf must be a json object");

        println!("Found {} non-steam game(s)!\n", shortcuts.len());

        let game_requests = if self.args.interactive {
            // Build sequentially, let user choose each game
            stream::iter(shortcuts)
                .then(|(k, v)| self.build_request(k, v))
                .try_collect()
                .await?
        } else {
            // Build parallely, 4 at a time
            stream::iter(shortcuts)
                .map(|(k, v)| async move { self.build_request(k, v).await })
                .buffer_unordered(4)
                .try_collect()
                .await?
        };

        Ok(game_requests)
    }

    async fn build_request(&self, key: &str, v: &Value) -> anyhow::Result<Plan> {
        let app_name = v["AppName"].as_str().expect("AppName key").to_string();
        let app_id = v["appid"].as_u64().expect("appid key") as u32;

        let games = self.client.search_by_name(&app_name).await?;

        let Some(game) = choose_game(&games, self.args.interactive) else {
            return Ok(Plan::NotFound(app_name));
        };

        let need_grid = self.args.overwrite || !asset_exists::<Grid>(app_id, &self.paths.grid);
        let need_hero = self.args.overwrite || !asset_exists::<Hero>(app_id, &self.paths.grid);
        let need_logo = self.args.overwrite || !asset_exists::<Logo>(app_id, &self.paths.grid);
        let need_icon = self.args.overwrite || !asset_exists::<Icon>(app_id, &self.paths.grid);

        if !need_icon && !need_hero && !need_logo && !need_grid {
            return Ok(Plan::AlreadyExists(app_name));
        }

        let (grids, heroes, logos, icons) = tokio::join!(
            async_if!(need_grid, self.client.find_asset::<Grid>(game.id)),
            async_if!(need_hero, self.client.find_asset::<Hero>(game.id)),
            async_if!(need_logo, self.client.find_asset::<Logo>(game.id)),
            async_if!(need_icon, self.client.find_asset::<Icon>(game.id)),
        );

        Ok(Plan::Found(Box::new(GameRequest {
            app_id,
            app_name,
            icon_key: key.to_owned(),

            icon: icons.transpose()?.and_then(|v| v.into_iter().next()),
            grid: grids.transpose()?.and_then(|v| v.into_iter().next()),
            hero: heroes.transpose()?.and_then(|v| v.into_iter().next()),
            logo: logos.transpose()?.and_then(|v| v.into_iter().next()),
        })))
    }
}

struct GameRequest {
    app_id: u32,
    app_name: String,
    icon_key: String,

    icon: Option<Asset<Icon>>,
    grid: Option<Asset<Grid>>,
    hero: Option<Asset<Hero>>,
    logo: Option<Asset<Logo>>,
}

fn print_plan(plans: &[Plan]) {
    let mut table = Table::new();

    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Game", "Grid", "Hero", "Logo", "Icon"]);

    let mut already_exists = Vec::new();
    let mut not_found = Vec::new();

    for plan in plans {
        match plan {
            Plan::Found(req) => {
                let asset = |v: bool| if v { "✓" } else { "✗" };

                table.add_row(vec![
                    Cell::new(&req.app_name),
                    Cell::new(asset(req.grid.is_some())),
                    Cell::new(asset(req.hero.is_some())),
                    Cell::new(asset(req.logo.is_some())),
                    Cell::new(asset(req.icon.is_some())),
                ]);
            }

            Plan::AlreadyExists(name) => {
                already_exists.push(name);
            }

            Plan::NotFound(name) => {
                not_found.push(name);
            }
        }
    }

    if table.row_count() > 0 {
        println!("Assets To Download:\n");
        println!("{table}");
    }

    if !already_exists.is_empty() {
        println!("\nAlready Up To Date (use --overwrite to refetch):");

        for name in already_exists {
            println!("- {name}");
        }
    }

    if !not_found.is_empty() {
        println!("\nNo Match Found (try changing the shortcut name):");

        for name in not_found {
            println!("- {name}");
        }
    }
}

enum Plan {
    Found(Box<GameRequest>),
    AlreadyExists(String),
    NotFound(String),
}
