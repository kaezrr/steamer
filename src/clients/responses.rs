use std::marker::PhantomData;

use serde::Deserialize;

use crate::asset_kind::AssetKind;

#[derive(Deserialize, Debug)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
}

#[derive(Deserialize, Debug)]
pub struct GameSearchObject {
    pub id: u64,
    pub name: String,
    pub external_platform_data: Option<SteamPlatformData>,
}

#[derive(Deserialize, Debug)]
pub struct SteamPlatformData {
    pub steam: Option<Vec<PlatformData>>,
}

#[derive(Deserialize, Debug)]
pub struct PlatformData {
    pub id: String,
}

#[derive(Deserialize, Debug)]
pub struct Asset<T: AssetKind> {
    pub mime: String,
    pub url: String,

    #[serde(skip)]
    marker: PhantomData<T>,
}

impl<T: AssetKind> Asset<T> {
    #[must_use]
    pub const fn stubbed_official(mime: String, url: String) -> Self {
        Self {
            mime,
            url,
            marker: PhantomData,
        }
    }
}
