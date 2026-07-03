use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use deck_analyzer::source_decks::{
    ArchidektClient, ArchidektDeckSearchQuery, format_moxfield_export_line,
    parse_archidekt_deck_search,
};
use deck_analyzer::types::CardRole;
use rusqlite::{Connection, params};

fn temp_dir(name: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "deck-analyzer-source-{name}-{}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("failed to create temp dir");
    path
}

fn file_url(path: &PathBuf) -> String {
    format!("file://{}", path.display())
}

fn create_card_lookup(conn: &Connection) {
    conn.execute(
        "
        CREATE TABLE card_lookup (
            name TEXT PRIMARY KEY,
            type_line TEXT,
            cmc REAL,
            mana_cost TEXT,
            color_identity TEXT,
            oracle_text TEXT,
            keywords TEXT
        )
        ",
        (),
    )
    .expect("failed to create card_lookup");
    conn.execute(
        "
        CREATE TABLE card_roles (
            name TEXT NOT NULL,
            role TEXT NOT NULL,
            PRIMARY KEY(name, role)
        )
        ",
        (),
    )
    .expect("failed to create card_roles");

    for (name, type_line, cmc, mana_cost, color_identity) in [
        ("Forest", "Basic Land - Forest", 0.0, "", "[\"G\"]"),
        ("Sol Ring", "Artifact", 1.0, "{1}", "[]"),
        (
            "Llanowar Elves",
            "Creature - Elf Druid",
            1.0,
            "{G}",
            "[\"G\"]",
        ),
        ("Swords to Plowshares", "Instant", 1.0, "{W}", "[\"W\"]"),
    ] {
        conn.execute(
            "
            INSERT INTO card_lookup (
                name,
                type_line,
                cmc,
                mana_cost,
                color_identity,
                oracle_text,
                keywords
            )
            VALUES (?1, ?2, ?3, ?4, ?5, '', '[]')
            ",
            params![name, type_line, cmc, mana_cost, color_identity],
        )
        .expect("failed to insert card_lookup row");
    }
    for (name, role) in [("Llanowar Elves", "ramp"), ("Sol Ring", "ramp")] {
        conn.execute(
            "
            INSERT INTO card_roles (name, role)
            VALUES (?1, ?2)
            ",
            params![name, role],
        )
        .expect("failed to insert card_roles row");
    }
}

#[test]
fn archidekt_page_urls_resolve_to_api_urls() {
    let archidekt = ArchidektClient;
    let api_url = archidekt
        .deck_api_url("https://archidekt.com/decks/4772837/iron_maiden")
        .expect("archidekt page url should normalize");
    assert_eq!(api_url, "https://archidekt.com/api/decks/4772837/");

    let api_url = archidekt
        .deck_api_url("https://archidekt.com/api/decks/4772837/")
        .expect("archidekt api url should remain valid");
    assert_eq!(api_url, "https://archidekt.com/api/decks/4772837/");
}

#[test]
fn archidekt_deck_search_url_includes_verified_parameters() {
    let archidekt = ArchidektClient;
    let api_url = archidekt
        .deck_search_api_url(&ArchidektDeckSearchQuery {
            commander_name: Some("Sheoldred, the Apocalypse".to_string()),
            name: Some("mono black".to_string()),
            owner_username: Some("example-user".to_string()),
            deck_format: Some(3),
            edh_bracket: Some(4),
            order_by: Some("-updatedAt".to_string()),
            page: Some(2),
            page_size: Some(10),
        })
        .expect("search url should build");

    for expected in [
        "commanderName=Sheoldred%2C+the+Apocalypse",
        "name=mono+black",
        "ownerUsername=example-user",
        "deckFormat=3",
        "edhBracket=4",
        "orderBy=-updatedAt",
        "page=2",
        "pageSize=10",
    ] {
        assert!(
            api_url.contains(expected),
            "expected search url to contain {expected:?}; got {api_url}"
        );
    }
}

#[test]
fn archidekt_list_decks_accepts_hyphenated_order_by_values() {
    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .arg("archidekt")
        .arg("list-decks")
        .arg("--order-by")
        .arg("-viewCount")
        .arg("--limit")
        .arg("0")
        .output()
        .expect("failed to run archidekt deck search");

    assert!(
        !output.status.success(),
        "deck search unexpectedly succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Archidekt deck search limit must be greater than 0"),
        "expected command to parse order-by and fail on limit validation; got:\n{stderr}"
    );
}

#[test]
fn archidekt_list_decks_accepts_short_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .arg("archidekt")
        .arg("list-decks")
        .arg("-c")
        .arg("Ms. Bumbleflower")
        .arg("-o")
        .arg("-viewCount")
        .arg("-r")
        .arg("desc")
        .arg("-l")
        .arg("0")
        .output()
        .expect("failed to run archidekt deck search");

    assert!(
        !output.status.success(),
        "deck search unexpectedly succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Archidekt deck search limit must be greater than 0"),
        "expected command to parse short options and fail on limit validation; got:\n{stderr}"
    );
}

#[test]
fn archidekt_export_unique_cards_accepts_short_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .arg("archidekt")
        .arg("export-unique-cards")
        .arg("-c")
        .arg("Ms. Bumbleflower")
        .arg("-o")
        .arg("viewCount")
        .arg("-r")
        .arg("asc")
        .arg("-t")
        .arg("100")
        .arg("-y")
        .arg("-l")
        .arg("0")
        .output()
        .expect("failed to run archidekt unique card export");

    assert!(
        !output.status.success(),
        "unique card export unexpectedly succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Archidekt export limit must be greater than 0"),
        "expected command to parse short options and fail on limit validation; got:\n{stderr}"
    );
}

#[test]
fn archidekt_export_unique_cards_rejects_zero_top_count() {
    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .arg("archidekt")
        .arg("export-unique-cards")
        .arg("--top")
        .arg("0")
        .output()
        .expect("failed to run archidekt unique card export");

    assert!(
        !output.status.success(),
        "unique card export unexpectedly succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Archidekt export top count must be greater than 0"),
        "expected command to reject zero top count; got:\n{stderr}"
    );
}

#[test]
fn formats_many_moxfield_tags_on_one_card_line() {
    let line = format_moxfield_export_line(
        "Bloom Tender",
        &[
            CardRole::Ramp,
            CardRole::Removal,
            CardRole::CardDraw,
            CardRole::Ramp,
        ],
    );
    assert_eq!(line, "1 Bloom Tender #!card_draw #!ramp #!removal");
    assert_eq!(line.lines().count(), 1);
    assert_eq!(line.matches("Bloom Tender").count(), 1);
}

#[test]
fn formats_moxfield_export_lines_without_tags() {
    let line = format_moxfield_export_line("Command Tower", &[]);
    assert_eq!(line, "1 Command Tower");
}

#[test]
fn parses_legacy_role_strings() {
    assert_eq!(
        CardRole::from_db_value("targeted_removal"),
        Some(CardRole::Removal)
    );
    assert_eq!(
        CardRole::from_db_value("board_wipe"),
        Some(CardRole::MassRemoval)
    );
}

#[test]
fn parses_archidekt_deck_search_results() {
    let search = parse_archidekt_deck_search(
        r#"
        {
            "count": 425,
            "next": "https://archidekt.com/api/decks/v3/?page=2",
            "results": [
                {
                    "id": 8524795,
                    "name": "Apocalyptic",
                    "size": 100,
                    "updatedAt": "2026-07-02T23:36:11.381739Z",
                    "createdAt": "2026-07-01T23:36:11.381739Z",
                    "deckFormat": 3,
                    "edhBracket": 4,
                    "viewCount": 42,
                    "owner": {
                        "username": "El_Deadpool"
                    }
                }
            ]
        }
        "#,
    )
    .expect("search JSON should parse");

    assert_eq!(search.count, 425);
    assert_eq!(
        search.next.as_deref(),
        Some("https://archidekt.com/api/decks/v3/?page=2")
    );
    assert_eq!(search.results.len(), 1);

    let deck = &search.results[0];
    assert_eq!(deck.id, 8524795);
    assert_eq!(deck.name, "Apocalyptic");
    assert_eq!(deck.owner.username, "El_Deadpool");
    assert_eq!(deck.deck_format, Some(3));
    assert_eq!(deck.edh_bracket, Some(4));
    assert_eq!(deck.size, Some(100));
    assert_eq!(deck.view_count, Some(42));
}

#[test]
fn analyzes_archidekt_source_without_persisting_deck() {
    let dir = temp_dir("archidekt");
    let conn = Connection::open(dir.join("card.sqlite")).expect("failed to open db");
    create_card_lookup(&conn);

    let fixture_path = dir.join("archidekt.json");
    fs::write(
        &fixture_path,
        r#"
        {
            "name": "Archidekt Fixture",
            "cards": [
                {
                    "quantity": 2,
                    "categories": ["Ramp"],
                    "card": {
                        "oracleCard": {
                            "name": "Llanowar Elves"
                        }
                    }
                },
                {
                    "quantity": 1,
                    "categories": ["Ramp"],
                    "card": {
                        "oracleCard": {
                            "name": "Sol Ring"
                        }
                    }
                }
            ]
        }
        "#,
    )
    .expect("failed to write archidekt fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .current_dir(&dir)
        .arg("archidekt")
        .arg("analyze")
        .arg(file_url(&fixture_path))
        .output()
        .expect("failed to run archidekt source analysis");

    assert!(
        output.status.success(),
        "analysis failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Source deck: Archidekt Fixture (archidekt)",
        "Cards: 3",
        "Lands: 0",
        "1: 3",
        "Creature: 2",
        "Artifact: 1",
        "Ramp: 3",
    ] {
        assert!(
            stdout.contains(expected),
            "expected output to contain {expected:?}; got:\n{stdout}"
        );
    }

    let decks_table_count: i64 = conn
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
                AND name IN ('decks', 'deck_cards')
            ",
            (),
            |row| row.get(0),
        )
        .expect("failed to query sqlite schema");
    assert_eq!(decks_table_count, 0);
}
