use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use deck_analyzer::source_decks::{
    ArchidektClient, ArchidektDeckSearchQuery, parse_archidekt_deck_search,
};
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
            color_identity TEXT
        )
        ",
        (),
    )
    .expect("failed to create card_lookup");

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
                color_identity
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![name, type_line, cmc, mana_cost, color_identity],
        )
        .expect("failed to insert card_lookup row");
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
        .arg("analyze-source")
        .arg("archidekt")
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
