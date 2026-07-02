use clap::{Parser, Subcommand};
use deck_analyzer::db::{find_card_type_line, open_cards_db, sync_cards_db};
use std::io::{Error, ErrorKind};

#[derive(Parser)]
#[command()]
struct Cli {
    #[arg(short, long)]
    debug: Option<bool>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Analyze { file_path: String },
    Sync { json_path: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Analyze { file_path } => {
            analyze_deck(file_path)?;
        }
        Commands::Sync { json_path } => {
            println!(
                "Syncing file {} with local db (may take a while)",
                &json_path
            );
            sync_cards_db(json_path)?;
        }
    }

    Ok(())
}

fn analyze_deck(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let deck_text = std::fs::read_to_string(file_path)?;
    let conn = open_cards_db()?;

    let mut total_cards = 0usize;
    let mut lands = 0usize;
    let mut missing_cards = 0usize;

    for (line_index, line) in deck_text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some((quantity_text, card_name)) = line.split_once(' ') else {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("line {line_number} is in the wrong format"),
            )
            .into());
        };

        let quantity = quantity_text.parse::<usize>().map_err(|_| {
            Error::new(
                ErrorKind::InvalidData,
                format!("line {line_number} has an invalid quantity"),
            )
        })?;
        let card_name = card_name.trim();

        total_cards += quantity;

        match find_card_type_line(&conn, card_name)? {
            Some(type_line) => {
                if type_line.contains("Land") {
                    lands += quantity;
                }
            }
            None => {
                missing_cards += 1;
                println!("Missing card in local database: {card_name}");
            }
        }
    }

    println!("Cards: {total_cards}");
    println!("Lands: {lands}");
    println!("Missing unique cards: {missing_cards}");

    Ok(())
}
