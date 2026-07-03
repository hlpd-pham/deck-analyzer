use clap::{Parser, Subcommand};
use deck_analyzer::analyzer::{Analyzer, DeckStats, SqliteCardLookup};
use deck_analyzer::db::{CARD_DB_PATH, sync_cards_db};
use deck_analyzer::error::AppError;
use deck_analyzer::source_decks::{ArchidektClient, ArchidektDeckSearchQuery};
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
    Archidekt {
        #[command(subcommand)]
        command: ArchidektCommands,
    },
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

#[derive(Subcommand)]
enum ArchidektCommands {
    Analyze {
        url: String,
    },
    ListDecks {
        #[arg(long)]
        commander_name: Option<String>,

        #[arg(long)]
        name: Option<String>,

        #[arg(long)]
        owner_username: Option<String>,

        #[arg(long, default_value_t = 3)]
        deck_format: u8,

        #[arg(long)]
        edh_bracket: Option<u8>,

        #[arg(long, default_value = "-updatedAt")]
        order_by: String,

        #[arg(long, default_value_t = 1)]
        page: usize,

        #[arg(long)]
        page_size: Option<usize>,

        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Commands::Archidekt { command } = &cli.command {
        let result: Result<(), AppError> = match command {
            ArchidektCommands::Analyze { url } => {
                let conn = match Connection::open(CARD_DB_PATH) {
                    Ok(conn) => conn,
                    Err(error) => {
                        eprintln!("Error: {error}");
                        return ExitCode::FAILURE;
                    }
                };
                let archidekt = ArchidektClient;
                match archidekt.load_deck(url) {
                    Ok(source_deck) => {
                        let mut deck_text = String::new();
                        for card in &source_deck.cards {
                            deck_text.push_str(&format!("{} {}\n", card.quantity, card.card_name));
                        }

                        let card_lookup = SqliteCardLookup { conn: &conn };
                        let analyzer = Analyzer { card_lookup };
                        match analyzer.analyze_text(&deck_text) {
                            Ok(stats) => {
                                println!(
                                    "Source deck: {} ({})",
                                    source_deck.name, source_deck.source
                                );
                                println!("Source URL: {}", source_deck.source_url);
                                println!();
                                print_deck_stats(&stats);
                                Ok(())
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            ArchidektCommands::ListDecks {
                commander_name,
                name,
                owner_username,
                deck_format,
                edh_bracket,
                order_by,
                page,
                page_size,
                limit,
            } => {
                if *page == 0 {
                    Err(AppError::InvalidSourceDeckFormat(
                        "Archidekt deck search page must be greater than 0".to_string(),
                    ))
                } else if page_size == &Some(0) {
                    Err(AppError::InvalidSourceDeckFormat(
                        "Archidekt deck search page size must be greater than 0".to_string(),
                    ))
                } else if *limit == 0 {
                    Err(AppError::InvalidSourceDeckFormat(
                        "Archidekt deck search limit must be greater than 0".to_string(),
                    ))
                } else {
                    let archidekt = ArchidektClient;
                    let query = ArchidektDeckSearchQuery {
                        commander_name: commander_name.clone(),
                        name: name.clone(),
                        owner_username: owner_username.clone(),
                        deck_format: Some(*deck_format),
                        edh_bracket: *edh_bracket,
                        order_by: Some(order_by.clone()),
                        page: Some(*page),
                        page_size: *page_size,
                    };

                    match archidekt.deck_search_api_url(&query) {
                        Ok(api_url) => match archidekt.search_decks(&query) {
                            Ok(search) => {
                                println!("Archidekt deck search");
                                println!("Query URL: {api_url}");
                                println!("Matches: {}", search.count);
                                println!("Returned by API: {}", search.results.len());
                                println!("Displayed: {}", search.results.len().min(*limit));
                                if let Some(next) = &search.next {
                                    println!("Next page: {next}");
                                }
                                println!();

                                for deck in search.results.iter().take(*limit) {
                                    let deck_format = deck
                                        .deck_format
                                        .map(|value| value.to_string())
                                        .unwrap_or_else(|| "-".to_string());
                                    let edh_bracket = deck
                                        .edh_bracket
                                        .map(|value| value.to_string())
                                        .unwrap_or_else(|| "-".to_string());
                                    let size = deck
                                        .size
                                        .map(|value| value.to_string())
                                        .unwrap_or_else(|| "-".to_string());
                                    let view_count = deck
                                        .view_count
                                        .map(|value| value.to_string())
                                        .unwrap_or_else(|| "-".to_string());
                                    let updated_at = deck.updated_at.as_deref().unwrap_or("-");

                                    println!(
                                        "{} | {} | owner={} | format={} | bracket={} | size={} | views={} | updated={} | https://archidekt.com/decks/{}",
                                        deck.id,
                                        deck.name,
                                        deck.owner.username,
                                        deck_format,
                                        edh_bracket,
                                        size,
                                        view_count,
                                        updated_at,
                                        deck.id
                                    );
                                }

                                Ok(())
                            }
                            Err(error) => Err(error),
                        },
                        Err(error) => Err(error),
                    }
                }
            }
        };

        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Error: {error}");
                ExitCode::FAILURE
            }
        };
    }

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
                        print_deck_stats(&stats);
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
        Commands::Archidekt { .. } => unreachable!(),
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
