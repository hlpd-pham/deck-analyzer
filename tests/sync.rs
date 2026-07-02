use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use deck_analyzer::db::sync_cards_db;
use rusqlite::Connection;

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

fn create_existing_cards_table(conn: &Connection) {
    conn.execute(
        "
        CREATE TABLE cards (
  id TEXT PRIMARY KEY,
  oracle_id TEXT,
  name TEXT NOT NULL,
  type_line TEXT,
  mana_cost TEXT,
  cmc REAL,
  colors TEXT,
  color_identity TEXT,
  layout TEXT,
  lang TEXT,
  set_code TEXT,
  collector_number TEXT,
  rarity TEXT
)
        ",
        (),
    )
    .expect("failed to create cards table");

    conn.execute(
        "
        INSERT INTO cards (
            id,
            name,
            type_line,
            colors,
            color_identity,
            lang
        )
        VALUES ('card-1', 'Llanowar Elves', 'Creature - Elf Druid', '', '', 'en')
        ",
        (),
    )
    .expect("failed to insert stale card row");
}

fn write_jsonl(path: &Path) {
    fs::write(
        path,
        r#"{"id":"card-1","name":"Llanowar Elves","lang":"en","layout":"normal","mana_cost":"{G}","cmc":1.0,"type_line":"Creature - Elf Druid","colors":["G"],"color_identity":["G"],"set":"lea","collector_number":"1","rarity":"common"}"#,
    )
    .expect("failed to write jsonl");
}

#[test]
fn sync_upserts_color_fields_and_rebuilds_lookup() {
    let dir = temp_dir("sync");
    let jsonl_path = dir.join("cards.jsonl");
    write_jsonl(&jsonl_path);

    let mut conn = Connection::open(dir.join("card.sqlite")).expect("failed to open test db");
    create_existing_cards_table(&conn);

    sync_cards_db(
        jsonl_path.to_str().expect("jsonl path should be utf8"),
        &mut conn,
    )
    .expect("sync should succeed");

    let (colors, color_identity): (String, String) = conn
        .query_row(
            "SELECT colors, color_identity FROM cards WHERE id = 'card-1'",
            (),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("failed to query cards row");
    assert_eq!(colors, "[\"G\"]");
    assert_eq!(color_identity, "[\"G\"]");

    let lookup_color_identity: String = conn
        .query_row(
            "SELECT color_identity FROM card_lookup WHERE name = 'Llanowar Elves'",
            (),
            |row| row.get(0),
        )
        .expect("failed to query card lookup row");
    assert_eq!(lookup_color_identity, "[\"G\"]");
}
