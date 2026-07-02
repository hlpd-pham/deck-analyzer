use clap::{Parser, Subcommand};
use deck_analyzer::analyzer::{Analyzer, DeckStats, SqliteCardLookup};
use deck_analyzer::db::{CARD_DB_PATH, sync_cards_db};
use deck_analyzer::error::AppError;
use rusqlite::Connection;
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
    let cli = Cli::parse();
    let mut conn = match Connection::open(CARD_DB_PATH) {
        Ok(conn) => conn,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::FAILURE;
        }
    };

    let result: Result<(), AppError> = match &cli.command {
        Commands::Analyze { file_path } => {
            let card_lookup = SqliteCardLookup::new(&conn);
            let analyzer = Analyzer::new(card_lookup);
            match analyzer.analyze_file(file_path) {
                Ok(stats) => {
                    print_deck_stats(&stats);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        Commands::Sync { json_path } => {
            println!(
                "Syncing file {} with local db (may take a while)",
                &json_path
            );
            sync_cards_db(json_path, &mut conn)
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_deck_stats(stats: &DeckStats) {
    for card_name in &stats.missing_cards {
        println!("Missing card in local database: {card_name}");
    }

    println!("Cards: {}", stats.total_cards);
    println!("Lands: {}", stats.lands);
    println!("Missing unique cards: {}", stats.missing_cards.len());
    println!();
    println!("Mana curve:");
    for (bucket, count) in stats.mana_curve.iter().enumerate() {
        if bucket == 7 {
            println!("7+: {count}");
        } else {
            println!("{bucket}: {count}");
        }
    }
    println!();
    println!("Types:");
    println!("Creature: {}", stats.type_counts.creature);
    println!("Artifact: {}", stats.type_counts.artifact);
    println!("Enchantment: {}", stats.type_counts.enchantment);
    println!("Instant: {}", stats.type_counts.instant);
    println!("Sorcery: {}", stats.type_counts.sorcery);
    println!("Planeswalker: {}", stats.type_counts.planeswalker);
    println!("Battle: {}", stats.type_counts.battle);
    println!("Other: {}", stats.type_counts.other);
    println!();
    println!("Color identity:");
    println!("White: {}", stats.color_identity_counts.white);
    println!("Blue: {}", stats.color_identity_counts.blue);
    println!("Black: {}", stats.color_identity_counts.black);
    println!("Red: {}", stats.color_identity_counts.red);
    println!("Green: {}", stats.color_identity_counts.green);
    println!("Colorless: {}", stats.color_identity_counts.colorless);
    println!("Multicolor: {}", stats.color_identity_counts.multicolor);
}
