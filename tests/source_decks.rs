use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use deck_analyzer::source_decks::ArchidektClient;
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
        "Roles:",
        "Ramp: 3",
        "Warnings:",
        "Mana base may be light: 0 lands found",
        "Card draw package looks light: 0 draw cards found",
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
