use clap::{Parser, Subcommand};
use deck_analyzer::analyzer::{Analyzer, DeckStats, SqliteCardLookup};
use deck_analyzer::db::{CARD_DB_PATH, sync_cards_db};
use deck_analyzer::error::AppError;
use deck_analyzer::source_decks::{ArchidektClient, ArchidektDeckSearchQuery};
use reqwest::StatusCode;
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    ExportUniqueCards {
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

        #[arg(
            long,
            default_value = "-updatedAt",
            long_help = "Sort key for Archidekt deck search. Known values: name, updatedAt, createdAt, viewCount, size, edhBracket. Prefix with '-' for descending, for example -updatedAt."
        )]
        order_by: String,

        #[arg(long, default_value_t = 1)]
        page: usize,

        #[arg(long)]
        page_size: Option<usize>,

        #[arg(long, default_value_t = 10)]
        limit: usize,
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

        #[arg(
            long,
            default_value = "-updatedAt",
            long_help = "Sort key for Archidekt deck search. Known values: name, updatedAt, createdAt, viewCount, size, edhBracket. Prefix with '-' for descending, for example -updatedAt."
        )]
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
            ArchidektCommands::ExportUniqueCards {
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
                        "Archidekt export limit must be greater than 0".to_string(),
                    ))
                } else {
                    (|| {
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

                        let api_url = archidekt.deck_search_api_url(&query)?;
                        let mut attempt = 1;
                        let mut rate_limit_backoff = Duration::from_secs(2);
                        let search = loop {
                            match archidekt.search_decks(&query) {
                                Ok(search) => break Ok(search),
                                Err(AppError::Http(error))
                                    if error.status() == Some(StatusCode::TOO_MANY_REQUESTS)
                                        && attempt < 6 =>
                                {
                                    eprintln!(
                                        "Archidekt rate limited deck search. Waiting {} seconds before retry {}.",
                                        rate_limit_backoff.as_secs(),
                                        attempt + 1
                                    );
                                    sleep(rate_limit_backoff);
                                    rate_limit_backoff = std::cmp::min(
                                        rate_limit_backoff.saturating_mul(2),
                                        Duration::from_secs(64),
                                    );
                                    attempt += 1;
                                }
                                Err(error) => break Err(error),
                            }
                        }?;
                        let deck_count = search.results.len().min(*limit);
                        println!("Archidekt unique card export");
                        println!("Query URL: {api_url}");
                        println!("Matches: {}", search.count);
                        println!("Returned by API: {}", search.results.len());
                        println!("Decks selected: {deck_count}");
                        println!("Detail request interval: 2 seconds");
                        println!();

                        let mut card_names = HashSet::new();
                        let mut decks_fetched = 0;
                        let mut next_detail_request_at = Instant::now();

                        for (deck_index, deck) in search.results.iter().take(*limit).enumerate() {
                            println!(
                                "Fetching deck {}/{}: {} | {}",
                                deck_index + 1,
                                deck_count,
                                deck.id,
                                deck.name
                            );

                            let mut attempt = 1;
                            let mut rate_limit_backoff = Duration::from_secs(2);
                            let source_deck = loop {
                                let now = Instant::now();
                                if now < next_detail_request_at {
                                    sleep(next_detail_request_at - now);
                                }
                                next_detail_request_at = Instant::now() + Duration::from_secs(2);

                                let deck_url = format!("https://archidekt.com/decks/{}", deck.id);
                                match archidekt.load_deck(&deck_url) {
                                    Ok(source_deck) => break Ok(source_deck),
                                    Err(AppError::Http(error))
                                        if error.status()
                                            == Some(StatusCode::TOO_MANY_REQUESTS)
                                            && attempt < 6 =>
                                    {
                                        eprintln!(
                                            "Archidekt rate limited deck {}. Waiting {} seconds before retry {}.",
                                            deck.id,
                                            rate_limit_backoff.as_secs(),
                                            attempt + 1
                                        );
                                        sleep(rate_limit_backoff);
                                        rate_limit_backoff = std::cmp::min(
                                            rate_limit_backoff.saturating_mul(2),
                                            Duration::from_secs(64),
                                        );
                                        attempt += 1;
                                    }
                                    Err(error) => break Err(error),
                                }
                            }?;

                            decks_fetched += 1;
                            for card in source_deck.cards {
                                card_names.insert(card.card_name);
                            }
                        }

                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_err(|error| {
                                AppError::InvalidSourceDeckFormat(format!(
                                    "system clock is before unix epoch: {error}"
                                ))
                            })?
                            .as_secs();
                        let output_path = PathBuf::from(format!(
                            "/tmp/deck-analyzer-archidekt-unique-cards-{timestamp}.txt"
                        ));
                        let mut output_file = File::create(&output_path)?;
                        let mut sorted_card_names = card_names.into_iter().collect::<Vec<_>>();
                        sorted_card_names.sort();
                        for card_name in &sorted_card_names {
                            writeln!(output_file, "{card_name}")?;
                        }

                        println!();
                        println!("Decks fetched: {decks_fetched}");
                        println!("Unique cards: {}", sorted_card_names.len());
                        println!("Output file: {}", output_path.display());

                        Ok(())
                    })()
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
    println!();
    println!("Roles:");
    println!("Ramp: {}", stats.role_counts.ramp);
    println!("Card draw: {}", stats.role_counts.card_draw);
    println!("Targeted removal: {}", stats.role_counts.targeted_removal);
    println!("Board wipe: {}", stats.role_counts.board_wipe);
    println!("Tutor: {}", stats.role_counts.tutor);
    println!("Protection: {}", stats.role_counts.protection);
    println!("Win condition: {}", stats.role_counts.win_condition);
}
