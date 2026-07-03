use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use deck_analyzer::db::sync_cards_db;
use rusqlite::Connection;

#[test]
fn sync_upserts_color_fields_and_rebuilds_lookup() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "deck-analyzer-sync-{}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("failed to create temp dir");

    let jsonl_path = dir.join("cards.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"id":"card-1","name":"Llanowar Elves","lang":"en","layout":"normal","mana_cost":"{G}","cmc":1.0,"type_line":"Creature - Elf Druid","oracle_text":"{T}: Add {G}.","colors":["G"],"color_identity":["G"],"keywords":["Mana Ability"],"set":"lea","collector_number":"1","rarity":"common"}
{"id":"card-2","name":"Divination","lang":"en","layout":"normal","mana_cost":"{2}{U}","cmc":3.0,"type_line":"Sorcery","oracle_text":"Draw two cards.","colors":["U"],"color_identity":["U"],"keywords":[],"set":"m10","collector_number":"2","rarity":"common"}
{"id":"card-3","name":"Swords to Plowshares","lang":"en","layout":"normal","mana_cost":"{W}","cmc":1.0,"type_line":"Instant","oracle_text":"Exile target creature. Its controller gains life equal to its power.","colors":["W"],"color_identity":["W"],"keywords":[],"set":"sta","collector_number":"3","rarity":"uncommon"}
{"id":"card-4","name":"Wrath of God","lang":"en","layout":"normal","mana_cost":"{2}{W}{W}","cmc":4.0,"type_line":"Sorcery","oracle_text":"Destroy all creatures. They can't be regenerated.","colors":["W"],"color_identity":["W"],"keywords":[],"set":"lea","collector_number":"4","rarity":"rare"}
{"id":"card-5","name":"Demonic Tutor","lang":"en","layout":"normal","mana_cost":"{1}{B}","cmc":2.0,"type_line":"Sorcery","oracle_text":"Search your library for a card, put that card into your hand, then shuffle.","colors":["B"],"color_identity":["B"],"keywords":[],"set":"lea","collector_number":"5","rarity":"rare"}
{"id":"card-6","name":"Heroic Intervention","lang":"en","layout":"normal","mana_cost":"{1}{G}","cmc":2.0,"type_line":"Instant","oracle_text":"Permanents you control gain hexproof and indestructible until end of turn.","colors":["G"],"color_identity":["G"],"keywords":[],"set":"aer","collector_number":"6","rarity":"rare"}
{"id":"card-7","name":"Thassa's Oracle","lang":"en","layout":"normal","mana_cost":"{U}{U}","cmc":2.0,"type_line":"Creature - Merfolk Wizard","oracle_text":"When Thassa's Oracle enters, look at the top X cards of your library. If X is greater than or equal to the number of cards in your library, you win the game.","colors":["U"],"color_identity":["U"],"keywords":[],"set":"thb","collector_number":"7","rarity":"rare"}
"#,
    )
    .expect("failed to write jsonl");

    let mut conn = Connection::open(dir.join("card.sqlite")).expect("failed to open test db");
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

    let (lookup_color_identity, lookup_oracle_text, lookup_keywords): (String, String, String) =
        conn.query_row(
            "
            SELECT color_identity, oracle_text, keywords
            FROM card_lookup
            WHERE name = 'Llanowar Elves'
            ",
            (),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("failed to query card lookup row");
    assert_eq!(lookup_color_identity, "[\"G\"]");
    assert_eq!(lookup_oracle_text, "{T}: Add {G}.");
    assert_eq!(lookup_keywords, "[\"Mana Ability\"]");

    let role_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM card_roles", (), |row| row.get(0))
        .expect("failed to count card roles");
    assert_eq!(role_count, 7);

    for (name, role) in [
        ("Llanowar Elves", "ramp"),
        ("Divination", "card_draw"),
        ("Swords to Plowshares", "removal"),
        ("Wrath of God", "mass_removal"),
        ("Demonic Tutor", "tutor"),
        ("Heroic Intervention", "protection"),
        ("Thassa's Oracle", "win_condition"),
    ] {
        let role_exists: i64 = conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM card_roles
                WHERE name = ?1
                    AND role = ?2
                ",
                (name, role),
                |row| row.get(0),
            )
            .expect("failed to query card role");
        assert_eq!(role_exists, 1, "expected {name} to have role {role}");
    }
}
