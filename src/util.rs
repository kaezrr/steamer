use std::io::Write;
use std::path::Path;

use comfy_table::Cell;
use comfy_table::ContentArrangement;
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;

use crate::Plan;
use crate::asset_kind::AssetKind;
use crate::clients::responses::GameSearchObject;

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
        std::io::stdout().flush().expect("io flush");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).expect("read line");

        if let Ok(n) = input.trim().parse::<usize>()
            && n < max
        {
            println!("\n");
            return n;
        }

        println!("Invalid choice, try again.");
    }
}

pub fn print_plan(plans: &[Plan]) {
    let mut table = Table::new();

    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Game", "Grid", "Hero", "Logo", "Icon", "Header"]);

    let mut already_exists = Vec::new();
    let mut not_found = Vec::new();

    for plan in plans {
        match plan {
            Plan::Found(req) => {
                let asset = |v: bool| if v { "Y" } else { "N" };

                table.add_row(vec![
                    Cell::new(&req.app_name),
                    Cell::new(asset(req.grid.is_some())),
                    Cell::new(asset(req.hero.is_some())),
                    Cell::new(asset(req.logo.is_some())),
                    Cell::new(asset(req.icon.is_some())),
                    Cell::new(asset(req.header.is_some())),
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
        println!("Assets To Download:\n{table}");
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

pub async fn maybe<T>(cond: bool, fut: impl Future<Output = T>) -> Option<T> {
    if cond { Some(fut.await) } else { None }
}

#[must_use]
pub fn create_pb(mp: &MultiProgress, name: &str, kind: &str) -> ProgressBar {
    mp.add(
        ProgressBar::new(0)
            .with_message(format!("{name} ({kind})"))
            .with_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {msg:<30!} [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
                )
                .expect("set pb style")
                .progress_chars("=> "),
            ),
    )
}
