use crate::error::AppError;
use crate::types::{CardRole, ScryfallCard};
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
  oracle_text TEXT,
  mana_cost TEXT,
  cmc REAL,
  colors TEXT,
  color_identity TEXT,
  keywords TEXT,
  layout TEXT,
  lang TEXT,
  set_code TEXT,
  collector_number TEXT,
  rarity TEXT
)
        ",
        (),
    )?;

    for (column_name, column_definition) in [
        ("oracle_text", "oracle_text TEXT"),
        ("keywords", "keywords TEXT"),
    ] {
        let column_exists: i64 = tx.query_row(
            "
            SELECT COUNT(*)
            FROM pragma_table_info('cards')
            WHERE name = ?1
            ",
            params![column_name],
            |row| row.get(0),
        )?;

        if column_exists == 0 {
            tx.execute(
                &format!("ALTER TABLE cards ADD COLUMN {column_definition}"),
                (),
            )?;
        }
    }

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
                let keywords = match &card.keywords {
                    Some(keywords) => serde_json::to_string(keywords)?,
                    None => "[]".to_string(),
                };

                match tx.execute(
                    "
                    INSERT INTO cards (
  id,
  oracle_id,
  name,
  type_line,
  oracle_text,
  mana_cost,
  cmc,
  colors,
  color_identity,
  keywords,
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
  ?13,
  ?14,
  ?15
)
ON CONFLICT(id) DO UPDATE SET
  oracle_id = excluded.oracle_id,
  name = excluded.name,
  type_line = excluded.type_line,
  oracle_text = excluded.oracle_text,
  mana_cost = excluded.mana_cost,
  cmc = excluded.cmc,
  colors = excluded.colors,
  color_identity = excluded.color_identity,
  keywords = excluded.keywords,
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
                        card.oracle_text,
                        card.mana_cost,
                        card.cmc,
                        colors,
                        color_identity,
                        keywords,
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

    tx.execute("DROP TABLE IF EXISTS card_lookup", ())?;

    tx.execute(
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
    )?;

    let lookup_rows = tx.execute(
        "
        INSERT OR IGNORE INTO card_lookup (
            name,
            type_line,
            cmc,
            mana_cost,
            color_identity,
            oracle_text,
            keywords
        )
        SELECT
            name,
            type_line,
            cmc,
            mana_cost,
            color_identity,
            oracle_text,
            keywords
        FROM cards
        WHERE name IS NOT NULL
        ORDER BY name ASC, lang = 'en' DESC, id ASC
        ",
        (),
    )?;

    println!("Rebuilt {lookup_rows} card lookup rows");

    tx.execute("DROP TABLE IF EXISTS card_roles", ())?;
    tx.execute(
        "
        CREATE TABLE card_roles (
            name TEXT NOT NULL,
            role TEXT NOT NULL,
            PRIMARY KEY(name, role)
        )
        ",
        (),
    )?;

    let role_rows = {
        let mut stmt = tx.prepare(
            "
            SELECT name, type_line, oracle_text, keywords
            FROM card_lookup
            ORDER BY name ASC
            ",
        )?;
        let rows = stmt.query_map((), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;

        let mut role_rows = Vec::new();
        for row in rows {
            let (name, type_line, oracle_text, keywords) = row?;
            for role in roles_for_card(
                type_line.as_deref(),
                oracle_text.as_deref(),
                keywords.as_deref(),
            ) {
                role_rows.push((name.clone(), role));
            }
        }
        role_rows
    };

    let mut role_count = 0;
    for (name, role) in role_rows {
        tx.execute(
            "
            INSERT INTO card_roles (name, role)
            VALUES (?1, ?2)
            ",
            params![name, role.as_str()],
        )?;
        role_count += 1;
    }
    println!("Rebuilt {role_count} card role rows");
    tx.commit()?;
    println!("Inserted {insert_successful} cards");

    Ok(())
}

fn roles_for_card(
    type_line: Option<&str>,
    oracle_text: Option<&str>,
    _keywords: Option<&str>,
) -> Vec<CardRole> {
    let type_line = type_line.unwrap_or("");
    let is_land = type_line.contains("Land");
    let text = oracle_text.unwrap_or("").to_lowercase();
    let mut roles = Vec::new();

    let searches_library = text.contains("search your library for");
    let searches_land = searches_library
        && (text.contains("land card")
            || text.contains("land cards")
            || text.contains("basic land"));
    let puts_land_somewhere =
        text.contains("onto the battlefield") || text.contains("into your hand");
    if !is_land
        && (text.contains("add {")
            || text.contains("add one mana")
            || text.contains("add two mana")
            || text.contains("add three mana")
            || text.contains("add x mana")
            || (searches_land && puts_land_somewhere))
    {
        roles.push(CardRole::Ramp);
    }

    if text.contains("draw a card")
        || text.contains("draw two cards")
        || text.contains("draw three cards")
        || text.contains("draw four cards")
        || text.contains("draw x cards")
        || text.contains("draw that many cards")
        || text.contains("draw cards")
    {
        roles.push(CardRole::CardDraw);
    }

    if text.contains("destroy all")
        || text.contains("destroy each")
        || text.contains("exile all")
        || text.contains("exile each")
        || text.contains("all creatures get -")
        || text.contains("each creature gets -")
        || text.contains("damage to each creature")
    {
        roles.push(CardRole::BoardWipe);
    }

    let has_target = text.contains("target ");
    if has_target
        && (text.contains("destroy target")
            || text.contains("exile target")
            || text.contains("counter target")
            || text.contains("return target")
            || text.contains("target opponent sacrifices")
            || text.contains("target player sacrifices"))
    {
        roles.push(CardRole::TargetedRemoval);
    }

    let searches_nonland_card = text.contains("nonland card")
        || text.contains("creature card")
        || text.contains("artifact card")
        || text.contains("enchantment card")
        || text.contains("instant card")
        || text.contains("sorcery card")
        || text.contains("planeswalker card")
        || text.contains("for a card")
        || text.contains("for any card");
    let tutor_is_not_land_only = !searches_land || text.contains("nonland");
    if searches_library && searches_nonland_card && tutor_is_not_land_only {
        roles.push(CardRole::Tutor);
    }

    if text.contains("gain indestructible")
        || text.contains("gains indestructible")
        || text.contains("have indestructible")
        || text.contains("hexproof")
        || text.contains("protection from")
        || text.contains("prevent all damage")
        || text.contains("prevent the next")
        || text.contains("phase out")
        || text.contains("phases out")
    {
        roles.push(CardRole::Protection);
    }

    if text.contains("win the game")
        || text.contains("opponent loses the game")
        || text.contains("opponents lose the game")
        || text.contains("that player loses the game")
    {
        roles.push(CardRole::WinCondition);
    }

    roles
}
