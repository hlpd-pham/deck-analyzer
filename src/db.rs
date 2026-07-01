use crate::types::ScryfallCard;
use rusqlite::{Connection, params};
use std::{
    fs::File,
    io::{BufRead, BufReader},
};

pub fn sync_cards_db(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path).expect(&format!("cannot read_json_file: {}", path));
    let reader = BufReader::new(file);
    let mut line_index = 0;
    let mut insert_successful = 0;

    let conn = Connection::open("card.sqlite")?;

    let _ = conn.execute(
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
    );

    for line_result in reader.lines() {
        line_index += 1;
        match line_result {
            Ok(line) => {
                if line.len() < 1 {
                    continue;
                }
                let card: ScryfallCard =
                    serde_json::from_str(&line).expect("Invalid scryfall json");

                match conn.execute(
                    "
                    INSERT OR IGNORE INTO cards (
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
);
                    ",
                    params![
                        card.id,
                        card.oracle_id,
                        card.name,
                        card.type_line,
                        card.mana_cost,
                        card.cmc,
                        "",
                        "",
                        card.layout,
                        card.lang,
                        card.set,
                        card.collector_number,
                        card.rarity,
                    ],
                ) {
                    Ok(_) => insert_successful += 1,
                    Err(e) => {
                        println!("Encounter error while inserting {:?}", &card)
                    }
                }
            }
            Err(e) => {
                println!("Encounter error: {}", e);
            }
        }
    }

    println!("Inserted {insert_successful} cards");

    Ok(())
}
