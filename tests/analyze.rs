use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

fn temp_dir(name: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "deck-analyzer-{name}-{}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("failed to create temp dir");
    path
}

fn create_card_lookup_db(path: &PathBuf) {
    let conn = Connection::open(path.join("card.sqlite")).expect("failed to open test db");
    conn.execute(
        "
        CREATE TABLE card_lookup (
            name TEXT PRIMARY KEY,
            type_line TEXT,
            cmc REAL,
            mana_cost TEXT
        )
        ",
        (),
    )
    .expect("failed to create card_lookup");

    for (name, type_line, cmc, mana_cost) in [
        ("Command Tower", "Land", 0.0, ""),
        ("Llanowar Elves", "Creature - Elf Druid", 1.0, "{G}"),
        ("Solemn Simulacrum", "Artifact Creature - Golem", 4.0, "{4}"),
    ] {
        conn.execute(
            "
            INSERT INTO card_lookup (
                name,
                type_line,
                cmc,
                mana_cost
            )
            VALUES (?1, ?2, ?3, ?4)
            ",
            params![name, type_line, cmc, mana_cost],
        )
        .expect("failed to insert card lookup row");
    }
}

#[test]
fn analyze_reports_lands_curve_types_and_missing_cards() {
    let dir = temp_dir("stats");
    create_card_lookup_db(&dir);
    let deck_path = dir.join("deck.txt");
    fs::write(
        &deck_path,
        "\
1 Command Tower
2 Llanowar Elves
1 Solemn Simulacrum
1 Totally Missing Card
1 Totally Missing Card
",
    )
    .expect("failed to write deck file");

    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .current_dir(&dir)
        .arg("analyze")
        .arg("deck.txt")
        .output()
        .expect("failed to run analyzer");

    assert!(
        output.status.success(),
        "analyzer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Missing card in local database: Totally Missing Card",
        "Cards: 6",
        "Lands: 1",
        "Missing unique cards: 1",
        "1: 2",
        "4: 1",
        "Creature: 3",
        "Artifact: 1",
        "Other: 0",
    ] {
        assert!(
            stdout.contains(expected),
            "expected output to contain {expected:?}; got:\n{stdout}"
        );
    }
}

#[test]
fn analyze_fails_when_card_lookup_is_missing() {
    let dir = temp_dir("missing-lookup");
    fs::write(dir.join("deck.txt"), "1 Command Tower\n").expect("failed to write deck file");

    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .current_dir(&dir)
        .arg("analyze")
        .arg("deck.txt")
        .output()
        .expect("failed to run analyzer");

    assert!(!output.status.success(), "analyzer unexpectedly succeeded");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("card_lookup table is missing; run sync before analyze"),
        "expected missing lookup error; got:\n{stderr}"
    );
}
