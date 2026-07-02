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

        Ok(())
    }

    fn lookup_card(&self, card_name: &str) -> Result<Option<CardInfo>, AppError> {
        Ok(self
            .conn
            .query_row(
                "
                SELECT type_line, cmc
                FROM card_lookup
                WHERE name = ?1
                ",
                params![card_name],
                |row| {
                    Ok(CardInfo {
                        type_line: row.get(0)?,
                        cmc: row.get(1)?,
                    })
                },
            )
            .optional()?)
    }
}

pub struct Analyzer<L> {
    card_lookup: L,
}

impl<L: CardLookup> Analyzer<L> {
    pub fn new(card_lookup: L) -> Self {
        Self { card_lookup }
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

        for (line_index, line) in deck_text.lines().enumerate() {
            let line_number = line_index + 1;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let Some((quantity_text, card_name)) = line.split_once(' ') else {
                return Err(AppError::InvalidDeckLine { line_number });
            };

            let quantity = quantity_text
                .parse::<usize>()
                .map_err(|_| AppError::InvalidQuantity { line_number })?;
            let card_name = card_name.trim();

            total_cards += quantity;
            let card_info = self.card_lookup.lookup_card(card_name)?;

            match card_info {
                Some(card_info) => {
                    let type_line = card_info.type_line.unwrap_or_default();
                    if type_line.contains("Land") {
                        lands += quantity;
                    } else {
                        let bucket = mana_curve_bucket(card_info.cmc);
                        mana_curve[bucket] += quantity;
                        type_counts.add(&type_line, quantity);
                    }
                }
                None => {
                    if missing_cards.insert(card_name.to_string()) {
                        missing_card_names.push(card_name.to_string());
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
        })
    }
}

pub struct CardInfo {
    pub type_line: Option<String>,
    pub cmc: Option<f64>,
}

#[derive(Default)]
pub struct DeckStats {
    pub total_cards: usize,
    pub lands: usize,
    pub missing_cards: Vec<String>,
    pub mana_curve: [usize; 8],
    pub type_counts: TypeCounts,
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

fn mana_curve_bucket(cmc: Option<f64>) -> usize {
    match cmc {
        Some(cmc) if cmc >= 7.0 => 7,
        Some(cmc) if cmc >= 0.0 => cmc.floor() as usize,
        _ => 0,
    }
}
