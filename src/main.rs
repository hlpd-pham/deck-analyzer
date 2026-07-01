use clap::Parser;
use deck_analyzer::db::sync_cards_db;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    file: String,
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let args = Args::parse();

    println!("Parsing file: {}", args.file);
    let file_path = args.file;

    let _ = sync_cards_db(&file_path);

    Ok(())
}
