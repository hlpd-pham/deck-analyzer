use clap::{Parser, Subcommand, ValueEnum};
use deck_analyzer::analyzer::{Analyzer, DeckStats, SqliteCardLookup};
use deck_analyzer::db::{CARD_DB_PATH, sync_cards_db};
use deck_analyzer::error::AppError;
use deck_analyzer::source_decks::{
    ArchidektClient, ArchidektDeckSearchPage, ArchidektDeckSearchQuery, format_moxfield_export_line,
};
use deck_analyzer::types::CardRole;
use reqwest::StatusCode;
use rusqlite::{Connection, params};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
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
        #[arg(short = 'c', long)]
        commander_name: Option<String>,

        #[arg(short = 'n', long)]
        name: Option<String>,

        #[arg(short = 'u', long)]
        owner_username: Option<String>,

        #[arg(short = 'f', long, default_value_t = 3)]
        deck_format: u8,

        #[arg(short = 'b', long)]
        edh_bracket: Option<u8>,

        #[arg(
            short = 'o',
            long,
            default_value = "updatedAt",
            allow_hyphen_values = true,
            long_help = "Sort key for Archidekt deck search. Known values: name, updatedAt, createdAt, viewCount, size, edhBracket. Use --order-direction to choose ascending or descending. Legacy values like -viewCount are also accepted."
        )]
        order_by: String,

        #[arg(short = 'r', long, value_enum, default_value = "desc")]
        order_direction: OrderDirection,

        #[arg(short = 'p', long, default_value_t = 1)]
        page: usize,

        #[arg(short = 's', long)]
        page_size: Option<usize>,

        #[arg(short = 'l', long, default_value_t = 10)]
        limit: usize,

        #[arg(
            short = 't',
            long,
            long_help = "Only export the top N cards ranked by how many fetched decks include each card. Omit to export all included cards."
        )]
        top: Option<usize>,

        #[arg(
            short = 'R',
            long,
            long_help = "Only export cards with this local role. Known values: ramp, card_draw, removal, mass_removal, tutor, protection, win_condition."
        )]
        role: Option<CardRole>,

        #[arg(
            short = 'y',
            long = "pbcopy",
            long_help = "Copy the generated export file contents to the macOS clipboard with pbcopy."
        )]
        pbcopy: bool,
    },
    ListDecks {
        #[arg(short = 'c', long)]
        commander_name: Option<String>,

        #[arg(short = 'n', long)]
        name: Option<String>,

        #[arg(short = 'u', long)]
        owner_username: Option<String>,

        #[arg(short = 'f', long, default_value_t = 3)]
        deck_format: u8,

        #[arg(short = 'b', long)]
        edh_bracket: Option<u8>,

        #[arg(
            short = 'o',
            long,
            default_value = "updatedAt",
            allow_hyphen_values = true,
            long_help = "Sort key for Archidekt deck search. Known values: name, updatedAt, createdAt, viewCount, size, edhBracket. Use --order-direction to choose ascending or descending. Legacy values like -viewCount are also accepted."
        )]
        order_by: String,

        #[arg(short = 'r', long, value_enum, default_value = "desc")]
        order_direction: OrderDirection,

        #[arg(short = 'p', long, default_value_t = 1)]
        page: usize,

        #[arg(short = 's', long)]
        page_size: Option<usize>,

        #[arg(short = 'l', long, default_value_t = 25)]
        limit: usize,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum OrderDirection {
    Asc,
    Desc,
}

struct ExportRoleLookup {
    roles_by_card: HashMap<String, Vec<CardRole>>,
    warning: Option<String>,
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
                order_direction,
                page,
                page_size,
                limit,
                top,
                role,
                pbcopy,
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
                } else if top == &Some(0) {
                    Err(AppError::InvalidSourceDeckFormat(
                        "Archidekt export top count must be greater than 0".to_string(),
                    ))
                } else {
                    (|| {
                        let archidekt = ArchidektClient;
                        let mut query = ArchidektDeckSearchQuery {
                            commander_name: commander_name.clone(),
                            name: name.clone(),
                            owner_username: owner_username.clone(),
                            deck_format: Some(*deck_format),
                            edh_bracket: *edh_bracket,
                            order_by: archidekt_order_by_value(order_by, *order_direction),
                            page: Some(*page),
                            page_size: *page_size,
                        };

                        let api_url = archidekt.deck_search_api_url(&query)?;
                        let mut current_page = *page;
                        let mut selected_decks = Vec::new();
                        let mut search_match_count = 0;
                        let mut search_pages_fetched = 0;
                        let mut search_results_returned = 0;
                        while selected_decks.len() < *limit {
                            query.page = Some(current_page);
                            let search = search_archidekt_decks_with_backoff(&archidekt, &query)?;
                            if search_pages_fetched == 0 {
                                search_match_count = search.count;
                            }

                            search_pages_fetched += 1;
                            let returned_count = search.results.len();
                            let has_next = search.next.is_some();
                            search_results_returned += returned_count;

                            let remaining_decks = *limit - selected_decks.len();
                            selected_decks.extend(search.results.into_iter().take(remaining_decks));

                            if selected_decks.len() >= *limit || !has_next || returned_count == 0 {
                                break;
                            }
                            current_page += 1;
                        }

                        let deck_count = selected_decks.len();
                        println!("Archidekt unique card export");
                        println!("Query URL: {api_url}");
                        println!("Matches: {search_match_count}");
                        println!("Search pages fetched: {search_pages_fetched}");
                        println!("Returned by API: {search_results_returned}");
                        println!("Decks selected: {deck_count}");
                        println!("Detail request interval: 2 seconds");
                        println!();

                        let mut card_counts: HashMap<String, usize> = HashMap::new();
                        let mut decks_fetched = 0;
                        let mut next_detail_request_at = Instant::now();

                        for (deck_index, deck) in selected_decks.iter().enumerate() {
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
                            let mut deck_card_names = HashSet::new();
                            for card in source_deck.cards {
                                if card.is_token() {
                                    continue;
                                }
                                deck_card_names.insert(card.card_name);
                            }
                            for card_name in deck_card_names {
                                let count = card_counts.entry(card_name).or_insert(0);
                                *count += 1;
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
                        let unique_card_count = card_counts.len();
                        let mut ranked_cards = card_counts.into_iter().collect::<Vec<_>>();
                        ranked_cards.sort_by(|left, right| {
                            right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0))
                        });
                        let role_lookup_card_names = if role.is_some() {
                            ranked_cards
                                .iter()
                                .map(|(card_name, _)| card_name.clone())
                                .collect::<Vec<_>>()
                        } else {
                            let export_count =
                                top.unwrap_or(ranked_cards.len()).min(ranked_cards.len());
                            ranked_cards
                                .iter()
                                .take(export_count)
                                .map(|(card_name, _)| card_name.clone())
                                .collect::<Vec<_>>()
                        };
                        let role_lookup =
                            load_export_roles(&role_lookup_card_names, role.is_some())?;
                        let mut roles_by_card = role_lookup.roles_by_card;

                        if let Some(role) = role {
                            ranked_cards.retain(|(card_name, _)| {
                                roles_by_card
                                    .get(card_name)
                                    .is_some_and(|roles| roles.contains(role))
                            });
                        }

                        let export_count =
                            top.unwrap_or(ranked_cards.len()).min(ranked_cards.len());
                        let export_card_names = ranked_cards
                            .iter()
                            .take(export_count)
                            .map(|(card_name, _)| card_name.clone())
                            .collect::<Vec<_>>();

                        if let Some(warning) = role_lookup.warning {
                            eprintln!("Warning: {warning}");
                        }

                        for card_name in export_card_names {
                            let roles = roles_by_card.remove(&card_name).unwrap_or_default();
                            let line = format_moxfield_export_line(&card_name, &roles);
                            writeln!(output_file, "{line}")?;
                        }
                        if *pbcopy {
                            copy_file_to_pbcopy(&output_path)?;
                        }

                        println!();
                        println!("Decks fetched: {decks_fetched}");
                        println!("Unique cards: {unique_card_count}");
                        if let Some(role) = role {
                            println!("Role filter: {}", role.as_str());
                            println!("Matching unique cards: {}", ranked_cards.len());
                        }
                        println!("Exported cards: {export_count}");
                        println!("Output file: {}", output_path.display());
                        if *pbcopy {
                            println!("Copied to clipboard: yes");
                        }

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
                order_direction,
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
                    (|| {
                        let archidekt = ArchidektClient;
                        let mut query = ArchidektDeckSearchQuery {
                            commander_name: commander_name.clone(),
                            name: name.clone(),
                            owner_username: owner_username.clone(),
                            deck_format: Some(*deck_format),
                            edh_bracket: *edh_bracket,
                            order_by: archidekt_order_by_value(order_by, *order_direction),
                            page: Some(*page),
                            page_size: *page_size,
                        };

                        let api_url = archidekt.deck_search_api_url(&query)?;
                        let mut current_page = *page;
                        let mut selected_decks = Vec::new();
                        let mut search_match_count = 0;
                        let mut search_pages_fetched = 0;
                        let mut search_results_returned = 0;
                        let mut next_page_url = None;
                        while selected_decks.len() < *limit {
                            query.page = Some(current_page);
                            let search = search_archidekt_decks_with_backoff(&archidekt, &query)?;
                            if search_pages_fetched == 0 {
                                search_match_count = search.count;
                            }

                            search_pages_fetched += 1;
                            let returned_count = search.results.len();
                            let has_next = search.next.is_some();
                            next_page_url = search.next.clone();
                            search_results_returned += returned_count;

                            let remaining_decks = *limit - selected_decks.len();
                            selected_decks.extend(search.results.into_iter().take(remaining_decks));

                            if selected_decks.len() >= *limit || !has_next || returned_count == 0 {
                                break;
                            }
                            current_page += 1;
                        }

                        println!("Archidekt deck search");
                        println!("Query URL: {api_url}");
                        println!("Matches: {search_match_count}");
                        println!("Search pages fetched: {search_pages_fetched}");
                        println!("Returned by API: {search_results_returned}");
                        println!("Displayed: {}", selected_decks.len());
                        if selected_decks.len() >= *limit
                            && let Some(next) = &next_page_url
                        {
                            println!("Next page: {next}");
                        }
                        println!();

                        for deck in &selected_decks {
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
                    })()
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

fn search_archidekt_decks_with_backoff(
    archidekt: &ArchidektClient,
    query: &ArchidektDeckSearchQuery,
) -> Result<ArchidektDeckSearchPage, AppError> {
    let mut attempt = 1;
    let mut rate_limit_backoff = Duration::from_secs(2);
    loop {
        match archidekt.search_decks(query) {
            Ok(search) => break Ok(search),
            Err(AppError::Http(error))
                if error.status() == Some(StatusCode::TOO_MANY_REQUESTS) && attempt < 6 =>
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
    }
}

fn copy_file_to_pbcopy(path: &Path) -> Result<(), AppError> {
    let contents = std::fs::read(path)?;
    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
    {
        let Some(mut stdin) = child.stdin.take() else {
            return Err(AppError::InvalidSourceDeckFormat(
                "failed to open pbcopy stdin".to_string(),
            ));
        };
        stdin.write_all(&contents)?;
    }

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::InvalidSourceDeckFormat(format!(
            "pbcopy exited with status {status}"
        )))
    }
}

fn load_export_roles(card_names: &[String], required: bool) -> Result<ExportRoleLookup, AppError> {
    let mut roles_by_card = HashMap::new();

    if !Path::new(CARD_DB_PATH).exists() {
        if required {
            return Err(AppError::InvalidSourceDeckFormat(
                "card.sqlite not found; run sync before filtering by role".to_string(),
            ));
        }
        return Ok(ExportRoleLookup {
            roles_by_card,
            warning: Some("card.sqlite not found; exporting without Moxfield tags".to_string()),
        });
    }

    let conn = match Connection::open(CARD_DB_PATH) {
        Ok(conn) => conn,
        Err(error) => {
            if required {
                return Err(AppError::Sqlite(error));
            }
            return Ok(ExportRoleLookup {
                roles_by_card,
                warning: Some(format!(
                    "failed to open local card database: {error}; exporting without Moxfield tags"
                )),
            });
        }
    };

    let card_roles_exists = match conn.query_row(
        "
        SELECT COUNT(*)
        FROM sqlite_master
        WHERE type = 'table'
            AND name = 'card_roles'
        ",
        (),
        |row| row.get::<_, i64>(0),
    ) {
        Ok(card_roles_exists) => card_roles_exists,
        Err(error) => {
            if required {
                return Err(AppError::Sqlite(error));
            }
            return Ok(ExportRoleLookup {
                roles_by_card,
                warning: Some(format!(
                    "failed to inspect card_roles table: {error}; exporting without Moxfield tags"
                )),
            });
        }
    };
    if card_roles_exists == 0 {
        if required {
            return Err(AppError::InvalidSourceDeckFormat(
                "card_roles table is missing; run sync before filtering by role".to_string(),
            ));
        }
        return Ok(ExportRoleLookup {
            roles_by_card,
            warning: Some(
                "card_roles table is missing; exporting without Moxfield tags".to_string(),
            ),
        });
    }

    let mut role_lookup_warning = None;
    let mut stmt = match conn.prepare(
        "
        SELECT role
        FROM card_roles
        WHERE name = ?1
        ORDER BY role ASC
        ",
    ) {
        Ok(stmt) => stmt,
        Err(error) => {
            if required {
                return Err(AppError::Sqlite(error));
            }
            return Ok(ExportRoleLookup {
                roles_by_card,
                warning: Some(format!(
                    "failed to prepare card role lookup: {error}; exporting without Moxfield tags"
                )),
            });
        }
    };

    for card_name in card_names {
        let rows = match stmt.query_map(params![card_name], |row| row.get::<_, String>(0)) {
            Ok(rows) => rows,
            Err(error) => {
                if required {
                    return Err(AppError::Sqlite(error));
                }
                role_lookup_warning = Some(format!(
                    "failed to query card roles: {error}; exporting without Moxfield tags"
                ));
                break;
            }
        };
        let mut roles = Vec::new();
        for row in rows {
            let role_value = match row {
                Ok(role_value) => role_value,
                Err(error) => {
                    if required {
                        return Err(AppError::Sqlite(error));
                    }
                    role_lookup_warning = Some(format!(
                        "failed to read card roles: {error}; exporting without Moxfield tags for affected cards"
                    ));
                    roles.clear();
                    break;
                }
            };
            match CardRole::from_db_value(&role_value) {
                Some(role) => roles.push(role),
                None => {
                    if role_lookup_warning.is_none() {
                        role_lookup_warning = Some(format!(
                            "unknown card role {role_value}; skipping unknown export tags"
                        ));
                    }
                }
            }
        }

        if !roles.is_empty() {
            roles_by_card.insert(card_name.clone(), roles);
        }
    }

    Ok(ExportRoleLookup {
        roles_by_card,
        warning: role_lookup_warning,
    })
}

fn archidekt_order_by_value(order_by: &str, order_direction: OrderDirection) -> Option<String> {
    let order_by = order_by.trim();
    let order_by = order_by
        .strip_prefix('-')
        .or_else(|| order_by.strip_prefix('+'))
        .unwrap_or(order_by);

    if order_by.is_empty() {
        return None;
    }

    match order_direction {
        OrderDirection::Asc => Some(order_by.to_string()),
        OrderDirection::Desc => Some(format!("-{order_by}")),
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
    println!("Removal: {}", stats.role_counts.removal);
    println!("Mass removal: {}", stats.role_counts.mass_removal);
    println!("Tutor: {}", stats.role_counts.tutor);
    println!("Protection: {}", stats.role_counts.protection);
    println!("Win condition: {}", stats.role_counts.win_condition);
}
