pub mod asset_kind;
pub mod clients;
pub mod util;

use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;

use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use futures_util::stream;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use new_vdf_parser::open_shortcuts_vdf;
use new_vdf_parser::write_shortcuts_vdf;
use serde_json::Map;
use serde_json::Value;
use steamlocate::SteamDir;

use crate::asset_kind::AssetKind;
use crate::asset_kind::Grid;
use crate::asset_kind::Header;
use crate::asset_kind::Hero;
use crate::asset_kind::Icon;
use crate::asset_kind::Logo;
use crate::clients::SteamClient;
use crate::clients::SteamGridClient;
use crate::clients::responses::Asset;
use crate::util::asset_exists;
use crate::util::choose_game;
use crate::util::maybe;

const CONCURRENT_REQUESTS: usize = 4;

#[derive(clap::Parser)]
#[command(
    name = "steamer",
    about = "Download SteamGridDB assets for your steam library automatically"
)]
pub struct Args {
    /// Your SteamGridDB API key
    #[arg(long)]
    pub api_key: String,

    /// Fetch official steam store assets
    #[arg(long, default_value_t = false)]
    pub official: bool,

    /// Dry run the application without making any changes
    #[arg(long, short, default_value_t = false)]
    pub dry_run: bool,

    /// Interactively choose which SteamGridDB game to pick
    #[arg(long, short, default_value_t = false)]
    pub interactive: bool,

    /// Overwrite all existing assets and refetch them
    #[arg(long, short, default_value_t = false)]
    pub overwrite: bool,
}

pub struct Image<T: AssetKind> {
    bytes: Bytes,
    format: ImageType,
    marker: PhantomData<T>,
}

pub struct ResolvedGame {
    app_id: u32,
    app_name: String,
    icon_key: String,

    icon: Option<Asset<Icon>>,
    grid: Option<Asset<Grid>>,
    hero: Option<Asset<Hero>>,
    logo: Option<Asset<Logo>>,
    header: Option<String>,
}

impl ResolvedGame {
    fn into_requests(self) -> Vec<AssetRequest> {
        let mut requests = Vec::new();

        if let Some(asset) = self.grid {
            requests.push(AssetRequest::Grid {
                app_id: self.app_id,
                asset,
            });
        }

        if let Some(asset) = self.hero {
            requests.push(AssetRequest::Hero {
                app_id: self.app_id,
                asset,
            });
        }

        if let Some(asset) = self.logo {
            requests.push(AssetRequest::Logo {
                app_id: self.app_id,
                asset,
            });
        }

        if let Some(asset) = self.icon {
            requests.push(AssetRequest::Icon {
                app_id: self.app_id,
                icon_key: self.icon_key,
                asset,
            });
        }

        if let Some(asset) = self.header {
            requests.push(AssetRequest::Header {
                app_id: self.app_id,
                asset,
            });
        }

        requests
    }
}

pub struct App {
    pub args: Args,

    paths: SteamPaths,
    grid_client: SteamGridClient,
    steam_client: SteamClient,
    shortcuts_vdf: Value,
}

impl App {
    pub fn build(args: Args) -> anyhow::Result<Self> {
        let grid_client = SteamGridClient::new(&args.api_key)?;
        let steam_client = SteamClient::new()?;

        let steam = steamlocate::locate()?;
        println!("Found Steam directory - {}", steam.path().display());

        let paths = SteamPaths::locate(&steam)?;
        std::fs::create_dir_all(&paths.grid)?;
        println!("Using Grid directory - {}", paths.grid.display());

        let shortcuts_vdf = open_shortcuts_vdf(&paths.shortcuts);

        Ok(Self {
            args,
            paths,
            grid_client,
            steam_client,
            shortcuts_vdf,
        })
    }

    pub async fn build_plan(&self) -> anyhow::Result<Vec<Plan>> {
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
                .map(|(k, v)| self.build_request(k, v))
                .buffer_unordered(CONCURRENT_REQUESTS)
                .try_collect()
                .await?
        };

        Ok(game_requests)
    }

    pub async fn execute(mut self, games: Vec<ResolvedGame>) -> anyhow::Result<()> {
        let requests = games
            .into_iter()
            .flat_map(ResolvedGame::into_requests)
            .collect::<Vec<_>>();

        if requests.is_empty() {
            println!("\nNothing to do...");
            return Ok(());
        }

        println!("\n");
        let progress_bar = ProgressBar::new(requests.len() as u64)
            .with_message("Downloading assets")
            .with_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {msg:<24} [{bar:40.cyan/blue}] {pos}/{len}",
                )
                .expect("set progress bar style")
                .progress_chars("=> "),
            );

        let icon_updates = stream::iter(requests)
            .map(|request| {
                let grid_client = &self.grid_client;
                let steam_client = &self.steam_client;
                let grid_dir = self.paths.grid.as_path();
                let pb = &progress_bar;

                async move {
                    request
                        .execute(grid_client, steam_client, grid_dir, pb)
                        .await
                }
            })
            .buffer_unordered(CONCURRENT_REQUESTS)
            .try_collect::<Vec<_>>()
            .await?;

        let icons_updated = !icon_updates.is_empty();
        for update in icon_updates.into_iter().flatten() {
            self.shortcuts_vdf[update.key]["icon"] = Value::String(update.path);
        }

        progress_bar.finish();

        if icons_updated {
            println!("\n\nUpdating shortcuts.vdf with icon data...");
            let mut vdf_to_write = Value::Object(Map::new());
            vdf_to_write["shortcuts"] = self.shortcuts_vdf;
            write_shortcuts_vdf(&self.paths.shortcuts, vdf_to_write);
        }

        println!(
            "Done! All assets were saved at {}",
            self.paths.grid.display()
        );

        Ok(())
    }

    async fn build_request(&self, key: &str, v: &Value) -> anyhow::Result<Plan> {
        let app_name = v["AppName"].as_str().expect("AppName key").to_string();
        let app_id = v["appid"].as_u64().expect("appid key") as u32;

        let games = self.grid_client.search_by_name(&app_name).await?;

        let Some(game) = choose_game(&games, self.args.interactive) else {
            return Ok(Plan::NotFound(app_name));
        };

        let steam_appid = self.grid_client.find_steam_appid(game.id).await?;

        let need_grid = self.need_asset::<Grid>(app_id);
        let need_hero = self.need_asset::<Hero>(app_id);
        let need_logo = self.need_asset::<Logo>(app_id);
        let need_icon = self.need_asset::<Icon>(app_id);

        let need_header = self.need_asset::<Header>(app_id);

        if !need_icon && !need_hero && !need_logo && !need_grid && !need_header {
            return Ok(Plan::AlreadyExists(app_name));
        }

        let (grids, heroes, logos, icons) = tokio::join!(
            maybe(need_grid, self.grid_client.find_asset::<Grid>(game.id)),
            maybe(need_hero, self.grid_client.find_asset::<Hero>(game.id)),
            maybe(need_logo, self.grid_client.find_asset::<Logo>(game.id)),
            maybe(need_icon, self.grid_client.find_asset::<Icon>(game.id)),
        );

        let header = if let Some(app_id) = steam_appid
            && need_header
        {
            self.steam_client.find_asset::<Header>(app_id).await
        } else {
            None
        };

        Ok(Plan::Found(Box::new(ResolvedGame {
            app_id,
            app_name,
            icon_key: key.to_owned(),

            icon: icons.transpose()?.and_then(|v| v.into_iter().next()),
            grid: grids.transpose()?.and_then(|v| v.into_iter().next()),
            hero: heroes.transpose()?.and_then(|v| v.into_iter().next()),
            logo: logos.transpose()?.and_then(|v| v.into_iter().next()),

            header,
        })))
    }

    fn need_asset<T: AssetKind>(&self, app_id: u32) -> bool {
        self.args.overwrite || !asset_exists::<T>(app_id, &self.paths.grid)
    }
}

impl<T: AssetKind> Image<T> {
    pub fn save(self, app_id: u32, dir: &Path) -> std::io::Result<String> {
        let ext = match self.format {
            ImageType::Jpg => "jpg",
            ImageType::Png | ImageType::Webp => "png", // Webp saves as png
            ImageType::Ico => "ico",
        };

        let filename = T::filename(app_id, ext);
        let path = dir.join(&filename);

        std::fs::write(&path, self.bytes)?;

        Ok(path.display().to_string())
    }
}

pub struct SteamPaths {
    pub shortcuts: PathBuf,
    pub grid: PathBuf,
}

impl SteamPaths {
    pub fn locate(steam: &SteamDir) -> anyhow::Result<Self> {
        let user_id: u64 = {
            let login_users_vdf = steam.path().join("config").join("loginusers.vdf");
            let contents = std::fs::read_to_string(login_users_vdf)?;
            let obj = keyvalues_parser::Vdf::parse(&contents)?.value.unwrap_obj();
            obj.keys().next().expect("login_user").parse::<u64>()? - 76_561_197_960_265_728
        };

        let config_path = steam
            .path()
            .join("userdata")
            .join(user_id.to_string())
            .join("config");

        let shortcuts_path = config_path.join("shortcuts.vdf");
        let grid_path = config_path.join("grid");

        Ok(Self {
            shortcuts: shortcuts_path,
            grid: grid_path,
        })
    }
}

pub enum ImageType {
    Png,
    Jpg,
    Webp,
    Ico,
}

pub enum Plan {
    Found(Box<ResolvedGame>),
    AlreadyExists(String),
    NotFound(String),
}

impl Plan {
    #[must_use]
    pub fn into_resolved_game(self) -> Option<ResolvedGame> {
        match self {
            Self::Found(req) => Some(*req),
            Self::AlreadyExists(_) | Self::NotFound(_) => None,
        }
    }
}

pub enum AssetRequest {
    Grid {
        app_id: u32,
        asset: Asset<Grid>,
    },

    Hero {
        app_id: u32,
        asset: Asset<Hero>,
    },

    Logo {
        app_id: u32,
        asset: Asset<Logo>,
    },

    Icon {
        app_id: u32,
        icon_key: String,
        asset: Asset<Icon>,
    },

    Header {
        app_id: u32,
        asset: String,
    },
}

impl AssetRequest {
    async fn execute(
        self,
        grid_client: &SteamGridClient,
        steam_client: &SteamClient,
        grid_dir: &Path,
        pb: &ProgressBar,
    ) -> anyhow::Result<Option<IconUpdate>> {
        match self {
            Self::Grid { app_id, asset } => {
                let image = grid_client.download_asset(&asset).await?;
                pb.inc(1);
                image.save(app_id, grid_dir)?;

                Ok(None)
            }

            Self::Hero { app_id, asset } => {
                let image = grid_client.download_asset(&asset).await?;
                pb.inc(1);
                image.save(app_id, grid_dir)?;

                Ok(None)
            }

            Self::Logo { app_id, asset } => {
                let image = grid_client.download_asset(&asset).await?;
                pb.inc(1);
                image.save(app_id, grid_dir)?;

                Ok(None)
            }

            Self::Icon {
                app_id,
                icon_key: key,
                asset,
            } => {
                let image = grid_client.download_asset(&asset).await?;
                pb.inc(1);
                let path = image.save(app_id, grid_dir)?;

                Ok(Some(IconUpdate { path, key }))
            }

            Self::Header { app_id, asset } => {
                let image = steam_client.download_asset::<Header>(&asset).await?;
                pb.inc(1);
                image.save(app_id, grid_dir)?;

                Ok(None)
            }
        }
    }
}

struct IconUpdate {
    path: String,
    key: String,
}
