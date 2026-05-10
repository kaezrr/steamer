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
    pub verified: bool,
    pub types: Vec<String>,
    pub release_date: Option<u64>,
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
