use crate::error::AppError;
use crate::types::ScryfallCard;
use rusqlite::{Connection, params};
use std::{
    fs::File,
    io::{BufRead, BufReader},
};

pub const CARD_DB_PATH: &str = "card.sqlite";

pub fn sync_cards_db(path: &str, conn: &mut Connection) -> Result<(), AppError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut insert_successful = 0;

    let tx = conn.transaction()?;

    tx.execute(
        "
        CREATE TABLE IF NOT EXISTS cards (
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
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_cards_name_lang ON cards(name, lang)",
        (),
    )?;

    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                if line.is_empty() {
                    continue;
                }
                let card: ScryfallCard = serde_json::from_str(&line)?;

                let colors = match &card.colors {
                    Some(colors) => serde_json::to_string(colors)?,
                    None => "[]".to_string(),
                };
                let color_identity = match &card.color_identity {
                    Some(color_identity) => serde_json::to_string(color_identity)?,
                    None => "[]".to_string(),
                };

                match tx.execute(
                    "
                    INSERT INTO cards (
  id,
  oracle_id,
  name,
  type_line,
  mana_cost,
  cmc,
  colors,
  color_identity,
  layout,
  lang,
  set_code,
  collector_number,
  rarity
)
VALUES (
  ?1,
  ?2,
  ?3,
  ?4,
  ?5,
  ?6,
  ?7,
  ?8,
  ?9,
  ?10,
  ?11,
  ?12,
  ?13
)
ON CONFLICT(id) DO UPDATE SET
  oracle_id = excluded.oracle_id,
  name = excluded.name,
  type_line = excluded.type_line,
  mana_cost = excluded.mana_cost,
  cmc = excluded.cmc,
  colors = excluded.colors,
  color_identity = excluded.color_identity,
  layout = excluded.layout,
  lang = excluded.lang,
  set_code = excluded.set_code,
  collector_number = excluded.collector_number,
  rarity = excluded.rarity;
                    ",
                    params![
                        card.id,
                        card.oracle_id,
                        card.name,
                        card.type_line,
                        card.mana_cost,
                        card.cmc,
                        colors,
                        color_identity,
                        card.layout,
                        card.lang,
                        card.set,
                        card.collector_number,
                        card.rarity,
                    ],
                ) {
                    Ok(_) => insert_successful += 1,
                    Err(e) => {
                        println!("Encounter error while inserting {:?}: {}", &card, e)
                    }
                }
            }
            Err(e) => return Err(AppError::Io(e)),
        }
    }

    rebuild_card_lookup(&tx)?;
    tx.commit()?;
    println!("Inserted {insert_successful} cards");

    Ok(())
}

pub fn rebuild_card_lookup(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DROP TABLE IF EXISTS card_lookup", ())?;

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
    )?;

    let lookup_rows = conn.execute(
        "
        INSERT OR IGNORE INTO card_lookup (
            name,
            type_line,
            cmc,
            mana_cost,
            color_identity
        )
        SELECT
            name,
            type_line,
            cmc,
            mana_cost,
            color_identity
        FROM cards
        WHERE name IS NOT NULL
        ORDER BY name ASC, lang = 'en' DESC, id ASC
        ",
        (),
    )?;

    println!("Rebuilt {lookup_rows} card lookup rows");

    Ok(())
}
