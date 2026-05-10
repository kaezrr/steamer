pub mod responses;

use std::marker::PhantomData;

use crate::Image;
use crate::ImageType;
use crate::asset_kind::AssetKind;
use crate::clients::responses::ApiResponse;
use crate::clients::responses::Asset;
use crate::clients::responses::GameSearchObject;

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

    pub async fn download_asset<T: AssetKind>(&self, asset: &Asset<T>) -> anyhow::Result<Image<T>> {
        let response = self
            .download_client
            .get(&asset.url)
            .send()
            .await?
            .error_for_status()?;

        let format = match asset.mime.as_str() {
            "image/png" => ImageType::Png,
            "image/jpeg" => ImageType::Jpg,
            "image/vnd.microsoft.icon" => ImageType::Ico,
            e => anyhow::bail!("Unknown mime type: {e}"),
        };

        Ok(Image {
            bytes: response.bytes().await?,
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
