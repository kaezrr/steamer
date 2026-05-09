use std::fmt::Display;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::BufMut;
use bytes::Bytes;
use bytes::BytesMut;
use comfy_table::Table;
use futures_util::StreamExt;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use serde::Deserialize;
use steamlocate::SteamDir;

pub struct SteamGridClient {
    client: reqwest::Client,
    download_client: reqwest::Client,
    base_url: String,
}

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

impl SteamGridClient {
    pub fn new(api_key: &str) -> anyhow::Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();

        let auth_value = format!("Bearer {api_key}");

        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&auth_value)?,
        );

        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .user_agent(APP_USER_AGENT)
            .build()?;

        // Downloading assets doesn't need auth headers
        let download_client = reqwest::ClientBuilder::new()
            .user_agent(APP_USER_AGENT)
            .build()?;

        Ok(Self {
            client,
            download_client,
            base_url: "https://www.steamgriddb.com/api/v2".to_owned(),
        })
    }

    pub async fn search_by_name(&self, name: &str) -> anyhow::Result<Vec<GameSearchObject>> {
        let url = format!("{}/search/autocomplete/{}", self.base_url, name);

        let response = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<Vec<GameSearchObject>>>()
            .await?;

        Ok(response.data)
    }

    pub async fn find_asset(
        &self,
        game_id: u64,
        asset_type: AssetType,
    ) -> anyhow::Result<Vec<GridAsset>> {
        let url = format!("{}{}{}", self.base_url, asset_type.get_url(), game_id);

        let response = self
            .client
            .get(&url)
            .query(asset_type.get_query_params())
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<Vec<GridAsset>>>()
            .await?;

        Ok(response.data)
    }

    pub async fn download_asset(
        &self,
        asset: &GridAsset,
        asset_type: AssetType,
        mp: Arc<MultiProgress>,
    ) -> anyhow::Result<Image> {
        let response = self
            .download_client
            .get(&asset.url)
            .send()
            .await?
            .error_for_status()?;

        let total = response.content_length().unwrap_or(0);
        let pb = mp.add(
            ProgressBar::new(total)
                .with_message(format!("Downloading {asset_type}..."))
                .with_style(ProgressStyle::with_template(
                    "{msg:12} [{bar:40.cyan/blue}] {bytes:>7}/{total_bytes:7} {eta}",
                )?),
        );

        let format = match asset.mime.as_str() {
            "image/png" => ImageType::Png,
            "image/jpeg" => ImageType::Jpg,
            "image/vnd.microsoft.icon" => ImageType::Ico,
            e => anyhow::bail!("Unknown mime type: {e}"),
        };

        let mut stream = response.bytes_stream();
        let mut bytes = BytesMut::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            pb.inc(chunk.len() as u64);
            bytes.put(chunk);
        }

        pb.finish();

        Ok(Image {
            bytes: bytes.freeze(),
            format,
        })
    }
}

#[derive(Deserialize, Debug)]
struct ApiResponse<T> {
    #[expect(unused)]
    pub success: bool,
    pub data: T,
}

#[derive(Deserialize, Debug)]
pub struct GameSearchObject {
    pub id: u64,
    pub name: String,
    pub verified: bool,
    pub types: Vec<String>,
    pub release_date: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct GridAsset {
    pub id: u64,
    pub score: i32,
    pub style: String,
    pub width: u32,
    pub height: u32,
    pub nsfw: bool,
    pub humor: bool,
    pub notes: Option<String>,
    pub mime: String,
    pub language: String,
    pub url: String,
    pub thumb: String,
    pub lock: bool,
    pub epilepsy: bool,
    pub upvotes: u32,
    pub downvotes: u32,
    pub author: Author,
}

#[derive(Deserialize, Debug)]
pub struct Author {
    pub name: String,
    pub steam64: String,
    pub avatar: String,
}

#[derive(Clone, Copy)]
pub enum AssetType {
    Grid,
    Hero,
    Logo,
    Icon,
}

impl Display for AssetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Grid => write!(f, "Grid"),
            Self::Hero => write!(f, "Hero"),
            Self::Logo => write!(f, "Logo"),
            Self::Icon => write!(f, "Icon"),
        }
    }
}

pub struct Image {
    bytes: Bytes,
    format: ImageType,
}

pub enum ImageType {
    Png,
    Jpg,
    Webp,
    Ico,
}

impl Image {
    pub fn save(self, app_id: u32, dir: &Path, asset_type: AssetType) -> std::io::Result<String> {
        let ext = match self.format {
            ImageType::Jpg => "jpg",
            ImageType::Png | ImageType::Webp => "png", // Webp saves as png
            ImageType::Ico => "ico",
        };

        let filename = match asset_type {
            AssetType::Grid => format!("{app_id}p.{ext}"),
            AssetType::Hero => format!("{app_id}_hero.{ext}"),
            AssetType::Logo => format!("{app_id}_logo.{ext}"),
            AssetType::Icon => format!("{app_id}_icon.{ext}"),
        };

        let path = dir.join(&filename);

        std::fs::write(&path, self.bytes)?;

        Ok(path.display().to_string())
    }
}

impl AssetType {
    const fn get_url(self) -> &'static str {
        match self {
            Self::Grid => "/grids/game/",
            Self::Hero => "/heroes/game/",
            Self::Logo => "/logos/game/",
            Self::Icon => "/icons/game/",
        }
    }

    /// If you want to customize your banner types, you will need to change these query parameters
    /// Maybe in future these could be user configurable
    const fn get_query_params(&self) -> &[(&'static str, &'static str)] {
        match self {
            Self::Grid => &[
                ("dimensions", "600x900"),
                ("types", "static"),
                ("nsfw", "any"),
            ],

            Self::Hero => &[
                ("dimensions", "3840x1240"),
                ("types", "static"),
                ("nsfw", "any"),
            ],

            Self::Logo | Self::Icon => {
                &[("styles", "official"), ("types", "static"), ("nsfw", "any")]
            }
        }
    }
}

#[must_use]
pub fn choose_game(
    games: &'_ [GameSearchObject],
    interactive: bool,
) -> Option<&'_ GameSearchObject> {
    if !interactive || games.is_empty() {
        return games.first();
    }

    let mut table = Table::new();
    table.set_header(vec!["#", "Name", "ID"]);

    let max_choices = games.len().min(5);

    // Only show the first 5 games, others are almost always irrelevant
    (0..max_choices).for_each(|i| {
        table.add_row(&[
            i.to_string(),
            games[i].name.clone(),
            games[i].id.to_string(),
        ]);
    });

    println!("Choose which game to pick:\n{table}");

    games.get(read_choice(max_choices))
}

#[must_use]
pub fn read_choice(max: usize) -> usize {
    loop {
        print!("Enter choice, (0-{}): ", max - 1);
        io::stdout().flush().expect("io flush");

        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("read line");

        if let Ok(n) = input.trim().parse::<usize>()
            && n < max
        {
            return n;
        }

        println!("Invalid choice, try again.");
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

#[must_use]
pub fn asset_exists(app_id: u32, grid_dir: &Path, asset_type: &AssetType) -> bool {
    let suffix = match asset_type {
        AssetType::Grid => "p",
        AssetType::Hero => "_hero",
        AssetType::Logo => "_logo",
        AssetType::Icon => "_icon",
    };

    for ext in &[".jpg", ".ico", ".png"] {
        let path = grid_dir.join(format!("{app_id}{suffix}{ext}"));
        if path.exists() {
            return true;
        }
    }

    false
}

pub async fn download_first_if_any(
    client: &SteamGridClient,
    assets: Option<&[GridAsset]>,
    asset_type: AssetType,
    mp: Arc<MultiProgress>,
) -> anyhow::Result<Option<Image>> {
    if let Some(asset) = assets.and_then(|v| v.first()) {
        Ok(Some(client.download_asset(asset, asset_type, mp).await?))
    } else {
        Ok(None)
    }
}
