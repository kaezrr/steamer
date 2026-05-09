pub trait AssetKind: Send + Sync {
    fn url() -> &'static str;
    fn query_params() -> &'static [(&'static str, &'static str)];
    fn filename(app_id: u32, ext: &str) -> String;
    fn display_name() -> &'static str;
    fn suffix() -> &'static str;
}

pub struct Icon;
pub struct Logo;
pub struct Hero;
pub struct Grid;

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

    fn suffix() -> &'static str {
        "p"
    }

    fn display_name() -> &'static str {
        "Grid"
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}p.{ext}")
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

    fn suffix() -> &'static str {
        "_hero"
    }

    fn display_name() -> &'static str {
        "Hero"
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}_hero.{ext}")
    }
}

impl AssetKind for Logo {
    fn url() -> &'static str {
        "/logos/game/"
    }

    fn query_params() -> &'static [(&'static str, &'static str)] {
        &[("types", "static"), ("nsfw", "any")]
    }

    fn suffix() -> &'static str {
        "_logo"
    }

    fn display_name() -> &'static str {
        "Logo"
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}_logo.{ext}")
    }
}

impl AssetKind for Icon {
    fn url() -> &'static str {
        "/icons/game/"
    }

    fn query_params() -> &'static [(&'static str, &'static str)] {
        &[("types", "static"), ("nsfw", "any")]
    }

    fn suffix() -> &'static str {
        "_icon"
    }

    fn display_name() -> &'static str {
        "Icon"
    }

    fn filename(app_id: u32, ext: &str) -> String {
        format!("{app_id}_icon.{ext}")
    }
}
