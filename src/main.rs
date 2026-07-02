use clap::{Parser, Subcommand};
use deck_analyzer::analyzer::{Analyzer, SqliteCardLookup};
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
    Analyze {
        file_path: String,
    },
    Validate {
        file_path: String,

        #[arg(short, long)]
        commander: String,
    },
    Sync {
        json_path: String,
    },
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
        Commands::Analyze { file_path } => match std::fs::read_to_string(file_path) {
            Ok(deck_text) => {
                let card_lookup = SqliteCardLookup { conn: &conn };
                let analyzer = Analyzer { card_lookup };
                match analyzer.analyze_text(&deck_text) {
                    Ok(stats) => {
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

                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(AppError::Io(error)),
        },
        Commands::Validate {
            file_path,
            commander,
        } => match std::fs::read_to_string(file_path) {
            Ok(deck_text) => {
                let card_lookup = SqliteCardLookup { conn: &conn };
                let analyzer = Analyzer { card_lookup };
                match analyzer.validate_commander(&deck_text, commander) {
                    Ok(validation) => {
                        println!("Commander validation:");
                        if validation.commander_found {
                            println!("PASS commander: {} found", validation.commander_name);
                        } else {
                            println!(
                                "FAIL commander: {} not found in local database",
                                validation.commander_name
                            );
                        }

                        if validation.deck_size == 99 {
                            println!("PASS deck size: 99 main-deck cards");
                        } else {
                            println!(
                                "FAIL deck size: {} main-deck cards, expected 99",
                                validation.deck_size
                            );
                        }

                        if validation.duplicate_cards.is_empty() {
                            println!("PASS singleton: no duplicate non-basic cards");
                        } else {
                            println!(
                                "FAIL singleton: duplicate non-basic cards: {}",
                                validation.duplicate_cards.join(", ")
                            );
                        }

                        if !validation.commander_found {
                            println!("FAIL color identity: commander not found");
                        } else if validation.off_color_cards.is_empty() {
                            println!("PASS color identity: all found cards are legal");
                        } else {
                            println!(
                                "FAIL color identity: off-color cards: {}",
                                validation.off_color_cards.join(", ")
                            );
                        }

                        if validation.missing_cards.is_empty() {
                            println!("PASS missing cards: none");
                        } else {
                            println!(
                                "FAIL missing cards: {}",
                                validation.missing_cards.join(", ")
                            );
                        }

                        if validation.valid {
                            Ok(())
                        } else {
                            Err(AppError::CommanderValidationFailed)
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(AppError::Io(error)),
        },
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
