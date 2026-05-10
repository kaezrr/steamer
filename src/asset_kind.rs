#[derive(Clone, Copy)]
pub enum AssetSource {
    OfficialSteam,
    SteamGridDb,
}

pub trait AssetKind: Send + Sync {
    fn url() -> &'static str;
    fn query_params() -> &'static [(&'static str, &'static str)];
    fn filename(app_id: u32, ext: &str) -> String;
    fn official_urls() -> &'static [&'static str];
    fn suffix() -> &'static str;
    fn preferred_source(offical: bool) -> AssetSource;
}

pub struct Icon;
pub struct Logo;
pub struct Hero;
pub struct Grid;
pub struct Head;

impl AssetKind for Grid {
    fn url() -> &'static str {
        "/grids/game/"
    }

    fn query_params() -> &'static [(&'static str, &'static str)] {
        &[
            ("dimensions", "600x900"),
            ("types", "static"),
            ("nsfw", "any"),
        ]
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}p.{ext}")
    }

    fn official_urls() -> &'static [&'static str] {
        &["library_600x900.jpg", "library_600x900_2x.jpg"]
    }

    fn suffix() -> &'static str {
        "p"
    }

    fn preferred_source(offical: bool) -> AssetSource {
        if offical {
            AssetSource::OfficialSteam
        } else {
            AssetSource::SteamGridDb
        }
    }
}

impl AssetKind for Hero {
    fn url() -> &'static str {
        "/heroes/game/"
    }

    fn query_params() -> &'static [(&'static str, &'static str)] {
        &[
            ("dimensions", "3840x1240"),
            ("types", "static"),
            ("nsfw", "any"),
        ]
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}_hero.{ext}")
    }

    fn official_urls() -> &'static [&'static str] {
        &["library_hero.jpg", "library_hero_2x.jpg"]
    }

    fn suffix() -> &'static str {
        "_hero"
    }

    fn preferred_source(offical: bool) -> AssetSource {
        if offical {
            AssetSource::OfficialSteam
        } else {
            AssetSource::SteamGridDb
        }
    }
}

impl AssetKind for Logo {
    fn url() -> &'static str {
        "/logos/game/"
    }

    fn query_params() -> &'static [(&'static str, &'static str)] {
        &[("styles", "official"), ("types", "static"), ("nsfw", "any")]
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}_logo.{ext}")
    }

    fn official_urls() -> &'static [&'static str] {
        &["logo.png", "logo_2x.png"]
    }

    fn suffix() -> &'static str {
        "_logo"
    }

    fn preferred_source(offical: bool) -> AssetSource {
        if offical {
            AssetSource::OfficialSteam
        } else {
            AssetSource::SteamGridDb
        }
    }
}

impl AssetKind for Icon {
    fn url() -> &'static str {
        "/icons/game/"
    }

    fn query_params() -> &'static [(&'static str, &'static str)] {
        &[("styles", "official"), ("types", "static"), ("nsfw", "any")]
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}_icon.{ext}")
    }

    fn official_urls() -> &'static [&'static str] {
        &[]
    }

    fn suffix() -> &'static str {
        "_icon"
    }

    fn preferred_source(_offical: bool) -> AssetSource {
        AssetSource::SteamGridDb
    }
}

impl AssetKind for Head {
    fn url() -> &'static str {
        ""
    }

    fn query_params() -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}.{ext}")
    }

    fn official_urls() -> &'static [&'static str] {
        &["header.jpg", "header_2x.jpg"]
    }

    fn suffix() -> &'static str {
        ""
    }

    fn preferred_source(_offical: bool) -> AssetSource {
        AssetSource::OfficialSteam
    }
}
