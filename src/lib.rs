pub mod app;
pub mod asset_kind;

use std::io;
use std::io::Write;
use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use asset_kind::AssetKind;
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

    pub async fn find_asset<T: AssetKind>(&self, game_id: u64) -> anyhow::Result<Vec<Asset<T>>> {
        let url = format!("{}{}{}", self.base_url, T::url(), game_id);

        let response = self
            .client
            .get(&url)
            .query(T::query_params())
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<Vec<Asset<T>>>>()
            .await?;

        Ok(response.data)
    }

    pub async fn download_asset<T: AssetKind>(
        &self,
        asset: &Asset<T>,
        mp: Arc<MultiProgress>,
    ) -> anyhow::Result<Image<T>> {
        let response = self
            .download_client
            .get(&asset.url)
            .send()
            .await?
            .error_for_status()?;

        let total = response.content_length().unwrap_or(0);
        let pb = mp.add(
            ProgressBar::new(total)
                .with_message(format!("Downloading {}...", T::display_name()))
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
            marker: PhantomData,
        })
    }

    pub async fn find_steam_appid(&self, steamgrid_id: u64) -> anyhow::Result<Option<u64>> {
        let url = format!("{}/games/id/{steamgrid_id}", self.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("platformdata", "steam")])
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<GameSearchObject>>()
            .await?;

        Ok(response
            .data
            .external_platform_data
            .as_ref()
            .and_then(|x| x.steam.first())
            .map(|x| x.id.parse::<u64>())
            .transpose()?)
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
    pub external_platform_data: Option<SteamPlatformData>,
}

#[derive(Deserialize, Debug)]
pub struct SteamPlatformData {
    pub steam: Vec<PlatformData>,
}

#[derive(Deserialize, Debug)]
pub struct PlatformData {
    pub id: String,
}

#[derive(Deserialize, Debug)]
pub struct Asset<T: AssetKind> {
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

    #[serde(skip)]
    marker: PhantomData<T>,
}

#[derive(Deserialize, Debug)]
pub struct Author {
    pub name: String,
    pub steam64: String,
    pub avatar: String,
}

pub struct Image<T: AssetKind> {
    bytes: Bytes,
    format: ImageType,

    marker: PhantomData<T>,
}

pub enum ImageType {
    Png,
    Jpg,
    Webp,
    Ico,
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
pub fn asset_exists<T: AssetKind>(app_id: u32, grid_dir: &Path) -> bool {
    let suffix = T::suffix();
    for ext in &[".jpg", ".ico", ".png"] {
        let path = grid_dir.join(format!("{app_id}{suffix}{ext}"));
        if path.exists() {
            return true;
        }
    }

    false
}

pub async fn download_first_if_any<T: AssetKind>(
    client: &SteamGridClient,
    assets: Option<&[Asset<T>]>,
    mp: Arc<MultiProgress>,
) -> anyhow::Result<Option<Image<T>>> {
    if let Some(asset) = assets.and_then(|v| v.first()) {
        Ok(Some(client.download_asset::<T>(asset, mp).await?))
    } else {
        Ok(None)
    }
}
