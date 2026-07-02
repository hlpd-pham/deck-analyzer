use clap::{Parser, Subcommand};
use deck_analyzer::db::{CARD_DB_PATH, sync_cards_db};
use deck_analyzer::error::AppError;
use rusqlite::{Connection, OptionalExtension, params};

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

fn main() -> Result<(), AppError> {
    let cli = Cli::parse();
    let conn = Connection::open(CARD_DB_PATH)?;

    match &cli.command {
        Commands::Analyze { file_path } => {
            analyze_deck(file_path, &conn)?;
        }
        Commands::Sync { json_path } => {
            println!(
                "Syncing file {} with local db (may take a while)",
                &json_path
            );
            sync_cards_db(json_path, &conn)?;
        }
    }

    Ok(())
}

fn analyze_deck(file_path: &str, conn: &Connection) -> Result<(), AppError> {
    let deck_text = std::fs::read_to_string(file_path)?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cards_name_lang ON cards(name, lang)",
        (),
    )?;

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
            return Err(AppError::InvalidDeckLine { line_number });
        };

        let quantity = quantity_text
            .parse::<usize>()
            .map_err(|_| AppError::InvalidQuantity { line_number })?;
        let card_name = card_name.trim();

        total_cards += quantity;
        let type_line = conn
            .query_row(
                "
                SELECT type_line
                FROM cards
                WHERE name = ?1
                ORDER BY lang = 'en' DESC
                LIMIT 1
                ",
                params![card_name],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        match type_line {
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
