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
        "deck-analyzer-validate-{name}-{}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("failed to create temp dir");
    path
}

#[test]
fn validate_prints_checklist_for_valid_deck() {
    let dir = temp_dir("valid");
    let conn = Connection::open(dir.join("card.sqlite")).expect("failed to open test db");
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
        (
            "Ezuri, Renegade Leader",
            "Legendary Creature - Elf Warrior",
            3.0,
            "{1}{G}{G}",
            "[\"G\"]",
        ),
        ("Forest", "Basic Land - Forest", 0.0, "", "[\"G\"]"),
        ("Sol Ring", "Artifact", 1.0, "{1}", "[]"),
        (
            "Llanowar Elves",
            "Creature - Elf Druid",
            1.0,
            "{G}",
            "[\"G\"]",
        ),
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
        .expect("failed to insert card lookup row");
    }

    fs::write(
        dir.join("deck.txt"),
        "\
1 Ezuri, Renegade Leader
97 Forest
1 Sol Ring
1 Llanowar Elves
",
    )
    .expect("failed to write deck file");

    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .current_dir(&dir)
        .arg("validate")
        .arg("deck.txt")
        .arg("-c")
        .arg("Ezuri, Renegade Leader")
        .output()
        .expect("failed to run validation");

    assert!(
        output.status.success(),
        "validation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Commander validation:",
        "PASS commander: Ezuri, Renegade Leader found",
        "PASS deck size: 99 main-deck cards",
        "PASS singleton: no duplicate non-basic cards",
        "PASS color identity: all found cards are legal",
        "PASS missing cards: none",
    ] {
        assert!(
            stdout.contains(expected),
            "expected output to contain {expected:?}; got:\n{stdout}"
        );
    }
}

#[test]
fn validate_fails_when_card_lookup_is_stale() {
    let dir = temp_dir("stale");
    let conn = Connection::open(dir.join("card.sqlite")).expect("failed to open test db");
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
    .expect("failed to create stale card_lookup");
    fs::write(dir.join("deck.txt"), "99 Forest\n").expect("failed to write deck file");

    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .current_dir(&dir)
        .arg("validate")
        .arg("deck.txt")
        .arg("-c")
        .arg("Ezuri, Renegade Leader")
        .output()
        .expect("failed to run validation");

    assert!(
        !output.status.success(),
        "validation unexpectedly succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .contains("card_lookup table is missing color identity data; run sync before analyze"),
        "expected stale lookup error; got:\n{stderr}"
    );
}

#[test]
fn validate_fails_when_card_lookup_is_missing() {
    let dir = temp_dir("missing");
    fs::write(dir.join("deck.txt"), "99 Forest\n").expect("failed to write deck file");

    let output = Command::new(env!("CARGO_BIN_EXE_deck-analyzer"))
        .current_dir(&dir)
        .arg("validate")
        .arg("deck.txt")
        .arg("-c")
        .arg("Ezuri, Renegade Leader")
        .output()
        .expect("failed to run validation");

    assert!(
        !output.status.success(),
        "validation unexpectedly succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("card_lookup table is missing; run sync before analyze"),
        "expected missing lookup error; got:\n{stderr}"
    );
}
