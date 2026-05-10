use clap::Parser;
use steamer::App;
use steamer::Args;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let app = App::build(args)?;

    let plan = app.build_plan().await?;
    steamer::util::print_plan(&plan);

    if app.args.dry_run {
        return Ok(());
    }

    Ok(())
}
