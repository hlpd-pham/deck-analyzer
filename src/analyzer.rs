use crate::decklist::DecklistParser;
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashSet;

pub trait CardLookup {
    fn ensure_ready(&self) -> Result<(), AppError>;
    fn lookup_card(&self, card_name: &str) -> Result<Option<CardInfo>, AppError>;
}

pub struct SqliteCardLookup<'a> {
    conn: &'a Connection,
}

impl<'a> SqliteCardLookup<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
}

impl CardLookup for SqliteCardLookup<'_> {
    fn ensure_ready(&self) -> Result<(), AppError> {
        let card_lookup_exists: i64 = self.conn.query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
                AND name = 'card_lookup'
            ",
            (),
            |row| row.get(0),
        )?;
        if card_lookup_exists == 0 {
            return Err(AppError::MissingCardLookup);
        }

        let color_identity_column_exists: i64 = self.conn.query_row(
            "
            SELECT COUNT(*)
            FROM pragma_table_info('card_lookup')
            WHERE name = 'color_identity'
            ",
            (),
            |row| row.get(0),
        )?;
        if color_identity_column_exists == 0 {
            return Err(AppError::StaleCardLookup);
        }

        Ok(())
    }

    fn lookup_card(&self, card_name: &str) -> Result<Option<CardInfo>, AppError> {
        Ok(self
            .conn
            .query_row(
                "
                SELECT type_line, cmc, color_identity
                FROM card_lookup
                WHERE name = ?1
                ",
                params![card_name],
                |row| {
                    Ok(CardInfo {
                        type_line: row.get(0)?,
                        cmc: row.get(1)?,
                        color_identity: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }
}

pub struct Analyzer<L> {
    card_lookup: L,
    decklist_parser: DecklistParser,
}

impl<L: CardLookup> Analyzer<L> {
    pub fn new(card_lookup: L) -> Self {
        Self {
            card_lookup,
            decklist_parser: DecklistParser,
        }
    }

    pub fn analyze_file(&self, file_path: &str) -> Result<DeckStats, AppError> {
        let deck_text = std::fs::read_to_string(file_path)?;
        self.analyze_text(&deck_text)
    }

    pub fn analyze_text(&self, deck_text: &str) -> Result<DeckStats, AppError> {
        self.card_lookup.ensure_ready()?;

        let mut total_cards = 0usize;
        let mut lands = 0usize;
        let mut missing_cards = HashSet::new();
        let mut missing_card_names = Vec::new();
        let mut mana_curve = [0usize; 8];
        let mut type_counts = TypeCounts::default();
        let mut color_identity_counts = ColorIdentityCounts::default();

        for deck_entry in self.decklist_parser.parse(deck_text)? {
            total_cards += deck_entry.quantity;
            let card_info = self.card_lookup.lookup_card(&deck_entry.card_name)?;

            match card_info {
                Some(card_info) => {
                    color_identity_counts
                        .add(card_info.color_identity.as_deref(), deck_entry.quantity)?;
                    let type_line = card_info.type_line.unwrap_or_default();
                    if type_line.contains("Land") {
                        lands += deck_entry.quantity;
                    } else {
                        let bucket = mana_curve_bucket(card_info.cmc);
                        mana_curve[bucket] += deck_entry.quantity;
                        type_counts.add(&type_line, deck_entry.quantity);
                    }
                }
                None => {
                    if missing_cards.insert(deck_entry.card_name.clone()) {
                        missing_card_names.push(deck_entry.card_name);
                    }
                }
            }
        }

        Ok(DeckStats {
            total_cards,
            lands,
            missing_cards: missing_card_names,
            mana_curve,
            type_counts,
            color_identity_counts,
        })
    }
}

pub struct CardInfo {
    pub type_line: Option<String>,
    pub cmc: Option<f64>,
    pub color_identity: Option<String>,
}

#[derive(Default)]
pub struct DeckStats {
    pub total_cards: usize,
    pub lands: usize,
    pub missing_cards: Vec<String>,
    pub mana_curve: [usize; 8],
    pub type_counts: TypeCounts,
    pub color_identity_counts: ColorIdentityCounts,
}

#[derive(Default)]
pub struct TypeCounts {
    pub creature: usize,
    pub artifact: usize,
    pub enchantment: usize,
    pub instant: usize,
    pub sorcery: usize,
    pub planeswalker: usize,
    pub battle: usize,
    pub other: usize,
}

impl TypeCounts {
    fn add(&mut self, type_line: &str, quantity: usize) {
        let mut matched = false;
        if type_line.contains("Creature") {
            self.creature += quantity;
            matched = true;
        }
        if type_line.contains("Artifact") {
            self.artifact += quantity;
            matched = true;
        }
        if type_line.contains("Enchantment") {
            self.enchantment += quantity;
            matched = true;
        }
        if type_line.contains("Instant") {
            self.instant += quantity;
            matched = true;
        }
        if type_line.contains("Sorcery") {
            self.sorcery += quantity;
            matched = true;
        }
        if type_line.contains("Planeswalker") {
            self.planeswalker += quantity;
            matched = true;
        }
        if type_line.contains("Battle") {
            self.battle += quantity;
            matched = true;
        }
        if !matched {
            self.other += quantity;
        }
    }
}

#[derive(Default)]
pub struct ColorIdentityCounts {
    pub white: usize,
    pub blue: usize,
    pub black: usize,
    pub red: usize,
    pub green: usize,
    pub colorless: usize,
    pub multicolor: usize,
}

impl ColorIdentityCounts {
    fn add(&mut self, color_identity: Option<&str>, quantity: usize) -> Result<(), AppError> {
        let Some(color_identity) = color_identity else {
            return Err(AppError::StaleCardLookup);
        };
        if color_identity.is_empty() {
            return Err(AppError::StaleCardLookup);
        }

        let colors: Vec<String> =
            serde_json::from_str(color_identity).map_err(|_| AppError::StaleCardLookup)?;

        match colors.as_slice() {
            [] => self.colorless += quantity,
            [color] => match color.as_str() {
                "W" => self.white += quantity,
                "U" => self.blue += quantity,
                "B" => self.black += quantity,
                "R" => self.red += quantity,
                "G" => self.green += quantity,
                _ => return Err(AppError::StaleCardLookup),
            },
            _ => self.multicolor += quantity,
        }

        Ok(())
    }
}

fn mana_curve_bucket(cmc: Option<f64>) -> usize {
    match cmc {
        Some(cmc) if cmc >= 7.0 => 7,
        Some(cmc) if cmc >= 0.0 => cmc.floor() as usize,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{Analyzer, CardInfo, CardLookup};
    use crate::error::AppError;

    struct TestLookup;

    impl CardLookup for TestLookup {
        fn ensure_ready(&self) -> Result<(), AppError> {
            Ok(())
        }

        fn lookup_card(&self, card_name: &str) -> Result<Option<CardInfo>, AppError> {
            let card_info = match card_name {
                "Command Tower" => CardInfo {
                    type_line: Some("Land".to_string()),
                    cmc: Some(0.0),
                    color_identity: Some("[]".to_string()),
                },
                "Llanowar Elves" => CardInfo {
                    type_line: Some("Creature - Elf Druid".to_string()),
                    cmc: Some(1.0),
                    color_identity: Some("[\"G\"]".to_string()),
                },
                "Boros Charm" => CardInfo {
                    type_line: Some("Instant".to_string()),
                    cmc: Some(2.0),
                    color_identity: Some("[\"R\",\"W\"]".to_string()),
                },
                _ => return Ok(None),
            };

            Ok(Some(card_info))
        }
    }

    #[test]
    fn analyzes_color_identity_counts() {
        let analyzer = Analyzer::new(TestLookup);
        let stats = analyzer
            .analyze_text(
                "
1 Command Tower
2 Llanowar Elves
1 Boros Charm
1 Missing Card
",
            )
            .expect("deck should analyze");

        assert_eq!(stats.total_cards, 5);
        assert_eq!(stats.lands, 1);
        assert_eq!(stats.missing_cards, ["Missing Card"]);
        assert_eq!(stats.color_identity_counts.green, 2);
        assert_eq!(stats.color_identity_counts.colorless, 1);
        assert_eq!(stats.color_identity_counts.multicolor, 1);
        assert_eq!(stats.mana_curve[1], 2);
        assert_eq!(stats.mana_curve[2], 1);
        assert_eq!(stats.type_counts.creature, 2);
        assert_eq!(stats.type_counts.instant, 1);
    }
}
