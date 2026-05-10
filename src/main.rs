use clap::Parser;
use steamer::App;
use steamer::Args;
use steamer::Plan;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let app = App::build(args)?;

    if app.args.clean {
        steamer::util::clean_dir(&app.paths.grid)?;
        return Ok(());
    }

    let plan = app.build_plan().await?;
    steamer::util::print_plan(&plan);

    if app.args.dry_run {
        return Ok(());
    }

    let requests = plan
        .into_iter()
        .filter_map(Plan::into_resolved_game)
        .collect();

    app.execute(requests).await
}
