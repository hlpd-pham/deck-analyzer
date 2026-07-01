use clap::Parser;

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

    deck_analyzer::read_json_file(&file_path);

    Ok(())
}
