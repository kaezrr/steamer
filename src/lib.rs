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
use crate::asset_kind::AssetSource;
use crate::asset_kind::Grid;
use crate::asset_kind::Head;
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

    /// Clean all the assets in the grid directory
    #[arg(long, short, default_value_t = false)]
    pub clean: bool,
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
    head: Option<Asset<Head>>,
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

        if let Some(asset) = self.head {
            requests.push(AssetRequest::Head {
                app_id: self.app_id,
                asset,
            });
        }

        requests
    }
}

pub struct App {
    pub args: Args,
    pub paths: SteamPaths,

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
                .then(|(k, v)| self.build_request(k, v, None))
                .try_collect()
                .await?
        } else {
            // Build parallely, 4 at a time
            let progress_bar = ProgressBar::new(shortcuts.len() as u64)
                .with_message("Processing shortcuts...")
                .with_style(
                    ProgressStyle::with_template("{msg:<24} [{bar:40.cyan/blue}] {pos}/{len}")
                        .expect("set progress bar style")
                        .progress_chars("=> "),
                );

            let plans = stream::iter(shortcuts)
                .map(|(k, v)| self.build_request(k, v, Some(&progress_bar)))
                .buffer_unordered(CONCURRENT_REQUESTS)
                .try_collect()
                .await?;

            progress_bar.finish();

            plans
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
                ProgressStyle::with_template("{msg:<24} [{bar:40.cyan/blue}] {pos}/{len}")
                    .expect("set progress bar style")
                    .progress_chars("=> "),
            );

        let icon_updates = stream::iter(requests)
            .map(|request| {
                let app = &self;
                let pb = &progress_bar;
                async move { request.execute(app, pb).await }
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

    async fn build_request(
        &self,
        key: &str,
        v: &Value,
        pb: Option<&ProgressBar>,
    ) -> anyhow::Result<Plan> {
        let app_name = v["AppName"].as_str().expect("AppName key").to_string();
        let app_id = v["appid"].as_u64().expect("appid key") as u32;

        let games = self.grid_client.search_by_name(&app_name).await?;

        let Some(game) = choose_game(&games, self.args.interactive) else {
            return Ok(Plan::NotFound(app_name));
        };

        let need_grid = self.need_asset::<Grid>(app_id);
        let need_hero = self.need_asset::<Hero>(app_id);
        let need_logo = self.need_asset::<Logo>(app_id);
        let need_icon = self.need_asset::<Icon>(app_id);
        let need_head = self.need_asset::<Head>(app_id);

        if !need_icon && !need_hero && !need_logo && !need_grid && !need_head {
            return Ok(Plan::AlreadyExists(app_name));
        }

        let steam_appid = self.grid_client.find_steam_appid(game.id).await?;

        let (grid, hero, logo, icon, head) = tokio::join!(
            maybe(need_grid, self.fetch_asset::<Grid>(game.id, steam_appid)),
            maybe(need_hero, self.fetch_asset::<Hero>(game.id, steam_appid)),
            maybe(need_logo, self.fetch_asset::<Logo>(game.id, steam_appid)),
            maybe(need_icon, self.fetch_asset::<Icon>(game.id, steam_appid)),
            maybe(need_head, self.fetch_asset::<Head>(game.id, steam_appid)),
        );

        if let Some(pb) = pb {
            pb.inc(1);
        }

        Ok(Plan::Found(Box::new(ResolvedGame {
            app_id,
            app_name,
            icon_key: key.to_owned(),

            icon: icon?,
            grid: grid?,
            hero: hero?,
            logo: logo?,
            head: head?,
        })))
    }

    async fn fetch_asset<T: AssetKind>(
        &self,
        grid_game_id: u64,
        steam_appid: Option<u64>,
    ) -> anyhow::Result<Option<Asset<T>>> {
        match T::preferred_source(self.args.official) {
            AssetSource::SteamGridDb => Ok(self
                .grid_client
                .find_asset::<T>(grid_game_id)
                .await?
                .into_iter()
                .next()),

            AssetSource::OfficialSteam => {
                let Some(appid) = steam_appid else {
                    return Ok(None);
                };

                Ok(self.steam_client.find_asset::<T>(appid).await)
            }
        }
    }

    fn need_asset<T: AssetKind>(&self, app_id: u32) -> bool {
        self.args.overwrite || !asset_exists::<T>(app_id, &self.paths.grid)
    }
}

impl<T: AssetKind> Image<T> {
    pub fn save(self, app_id: u32, dir: &Path, overwrite: bool) -> std::io::Result<String> {
        let ext = match self.format {
            ImageType::Jpg => "jpg",
            ImageType::Png | ImageType::Webp => "png",
            ImageType::Ico => "ico",
        };

        if overwrite {
            for old_ext in ["png", "jpg", "ico"] {
                let old_filename = T::filename(app_id, old_ext);
                let old_path = dir.join(old_filename);

                match std::fs::remove_file(&old_path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }

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

    Head {
        app_id: u32,
        asset: Asset<Head>,
    },
}

impl AssetRequest {
    async fn execute(self, app: &App, pb: &ProgressBar) -> anyhow::Result<Option<IconUpdate>> {
        match self {
            Self::Grid { app_id, asset } => {
                let image = match Grid::preferred_source(app.args.official) {
                    AssetSource::OfficialSteam => app.steam_client.download_asset(&asset).await?,
                    AssetSource::SteamGridDb => app.grid_client.download_asset(&asset).await?,
                };

                pb.inc(1);
                image.save(app_id, &app.paths.grid, app.args.overwrite)?;

                Ok(None)
            }

            Self::Hero { app_id, asset } => {
                let image = match Hero::preferred_source(app.args.official) {
                    AssetSource::OfficialSteam => app.steam_client.download_asset(&asset).await?,
                    AssetSource::SteamGridDb => app.grid_client.download_asset(&asset).await?,
                };

                pb.inc(1);
                image.save(app_id, &app.paths.grid, app.args.overwrite)?;

                Ok(None)
            }

            Self::Logo { app_id, asset } => {
                let image = match Logo::preferred_source(app.args.official) {
                    AssetSource::OfficialSteam => app.steam_client.download_asset(&asset).await?,
                    AssetSource::SteamGridDb => app.grid_client.download_asset(&asset).await?,
                };

                pb.inc(1);
                image.save(app_id, &app.paths.grid, app.args.overwrite)?;

                Ok(None)
            }

            Self::Icon {
                app_id,
                icon_key: key,
                asset,
            } => {
                let image = match Icon::preferred_source(app.args.official) {
                    AssetSource::OfficialSteam => app.steam_client.download_asset(&asset).await?,
                    AssetSource::SteamGridDb => app.grid_client.download_asset(&asset).await?,
                };

                pb.inc(1);
                let path = image.save(app_id, &app.paths.grid, app.args.overwrite)?;

                Ok(Some(IconUpdate { path, key }))
            }

            Self::Head { app_id, asset } => {
                let image = match Head::preferred_source(app.args.official) {
                    AssetSource::OfficialSteam => app.steam_client.download_asset(&asset).await?,
                    AssetSource::SteamGridDb => app.grid_client.download_asset(&asset).await?,
                };

                pb.inc(1);
                image.save(app_id, &app.paths.grid, app.args.overwrite)?;

                Ok(None)
            }
        }
    }
}

struct IconUpdate {
    path: String,
    key: String,
}
