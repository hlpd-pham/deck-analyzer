use clap::{Parser, Subcommand};
use deck_analyzer::db::{CARD_DB_PATH, sync_cards_db};
use deck_analyzer::error::AppError;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashSet;
use std::process::ExitCode;

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

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
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
    let card_lookup_exists: i64 = conn.query_row(
        "
        SELECT COUNT(*)
        FROM sqlite_master
        WHERE type = 'table'
            AND name = 'card_lookup'
        ",
        (),
        |row| row.get(0),
    )?;
    if card_lookup_exists == 0 {
        return Err(AppError::MissingCardLookup);
    }

    let mut total_cards = 0usize;
    let mut lands = 0usize;
    let mut missing_cards = HashSet::new();
    let mut mana_curve = [0usize; 8];
    let mut type_counts = TypeCounts::default();

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
        let card_info = conn
            .query_row(
                "
                SELECT type_line, cmc
                FROM card_lookup
                WHERE name = ?1
                ",
                params![card_name],
                |row| {
                    Ok(CardInfo {
                        type_line: row.get(0)?,
                        cmc: row.get(1)?,
                    })
                },
            )
            .optional()?;

        match card_info {
            Some(card_info) => {
                let type_line = card_info.type_line.unwrap_or_default();
                if type_line.contains("Land") {
                    lands += quantity;
                } else {
                    let bucket = mana_curve_bucket(card_info.cmc);
                    mana_curve[bucket] += quantity;
                    type_counts.add(&type_line, quantity);
                }
            }
            None => {
                if missing_cards.insert(card_name.to_string()) {
                    println!("Missing card in local database: {card_name}");
                }
            }
        }
    }

    println!("Cards: {total_cards}");
    println!("Lands: {lands}");
    println!("Missing unique cards: {}", missing_cards.len());
    println!();
    println!("Mana curve:");
    for (bucket, count) in mana_curve.iter().enumerate() {
        if bucket == 7 {
            println!("7+: {count}");
        } else {
            println!("{bucket}: {count}");
        }
    }
    println!();
    println!("Types:");
    println!("Creature: {}", type_counts.creature);
    println!("Artifact: {}", type_counts.artifact);
    println!("Enchantment: {}", type_counts.enchantment);
    println!("Instant: {}", type_counts.instant);
    println!("Sorcery: {}", type_counts.sorcery);
    println!("Planeswalker: {}", type_counts.planeswalker);
    println!("Battle: {}", type_counts.battle);
    println!("Other: {}", type_counts.other);

    Ok(())
}

struct CardInfo {
    type_line: Option<String>,
    cmc: Option<f64>,
}

#[derive(Default)]
struct TypeCounts {
    creature: usize,
    artifact: usize,
    enchantment: usize,
    instant: usize,
    sorcery: usize,
    planeswalker: usize,
    battle: usize,
    other: usize,
}

impl TypeCounts {
    fn add(&mut self, type_line: &str, quantity: usize) {
        let mut matched = false;
        if type_line.contains("Creature") {
            self.creature += quantity;
            matched = true;
        }
        if type_line.contains("Artifact") {
            self.artifact += quantity;
            matched = true;
        }
        if type_line.contains("Enchantment") {
            self.enchantment += quantity;
            matched = true;
        }
        if type_line.contains("Instant") {
            self.instant += quantity;
            matched = true;
        }
        if type_line.contains("Sorcery") {
            self.sorcery += quantity;
            matched = true;
        }
        if type_line.contains("Planeswalker") {
            self.planeswalker += quantity;
            matched = true;
        }
        if type_line.contains("Battle") {
            self.battle += quantity;
            matched = true;
        }
        if !matched {
            self.other += quantity;
        }
    }
}

fn mana_curve_bucket(cmc: Option<f64>) -> usize {
    match cmc {
        Some(cmc) if cmc >= 7.0 => 7,
        Some(cmc) if cmc >= 0.0 => cmc.floor() as usize,
        _ => 0,
    }
}
