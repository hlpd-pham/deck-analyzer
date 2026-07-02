use crate::decklist::parse_decklist;
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{HashMap, HashSet};

pub trait CardLookup {
    fn ensure_ready(&self) -> Result<(), AppError>;
    fn lookup_card(&self, card_name: &str) -> Result<Option<CardInfo>, AppError>;
}

pub struct SqliteCardLookup<'a> {
    pub conn: &'a Connection,
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
    pub card_lookup: L,
}

impl<L: CardLookup> Analyzer<L> {
    pub fn analyze_text(&self, deck_text: &str) -> Result<DeckStats, AppError> {
        self.card_lookup.ensure_ready()?;

        let mut total_cards = 0usize;
        let mut lands = 0usize;
        let mut missing_cards = HashSet::new();
        let mut missing_card_names = Vec::new();
        let mut mana_curve = [0usize; 8];
        let mut type_counts = TypeCounts::default();
        let mut color_identity_counts = ColorIdentityCounts::default();

        for deck_entry in parse_decklist(deck_text)? {
            total_cards += deck_entry.quantity;
            let card_info = self.card_lookup.lookup_card(&deck_entry.card_name)?;

            match card_info {
                Some(card_info) => {
                    let colors = parse_color_identity(card_info.color_identity)?;
                    match colors.as_slice() {
                        [] => color_identity_counts.colorless += deck_entry.quantity,
                        [color] => match color.as_str() {
                            "W" => color_identity_counts.white += deck_entry.quantity,
                            "U" => color_identity_counts.blue += deck_entry.quantity,
                            "B" => color_identity_counts.black += deck_entry.quantity,
                            "R" => color_identity_counts.red += deck_entry.quantity,
                            "G" => color_identity_counts.green += deck_entry.quantity,
                            _ => return Err(AppError::StaleCardLookup),
                        },
                        _ => color_identity_counts.multicolor += deck_entry.quantity,
                    }

                    let type_line = card_info.type_line.unwrap_or_default();
                    if type_line.contains("Land") {
                        lands += deck_entry.quantity;
                    } else {
                        let bucket = match card_info.cmc {
                            Some(cmc) if cmc >= 7.0 => 7,
                            Some(cmc) if cmc >= 0.0 => cmc.floor() as usize,
                            _ => 0,
                        };
                        mana_curve[bucket] += deck_entry.quantity;

                        let mut matched = false;
                        if type_line.contains("Creature") {
                            type_counts.creature += deck_entry.quantity;
                            matched = true;
                        }
                        if type_line.contains("Artifact") {
                            type_counts.artifact += deck_entry.quantity;
                            matched = true;
                        }
                        if type_line.contains("Enchantment") {
                            type_counts.enchantment += deck_entry.quantity;
                            matched = true;
                        }
                        if type_line.contains("Instant") {
                            type_counts.instant += deck_entry.quantity;
                            matched = true;
                        }
                        if type_line.contains("Sorcery") {
                            type_counts.sorcery += deck_entry.quantity;
                            matched = true;
                        }
                        if type_line.contains("Planeswalker") {
                            type_counts.planeswalker += deck_entry.quantity;
                            matched = true;
                        }
                        if type_line.contains("Battle") {
                            type_counts.battle += deck_entry.quantity;
                            matched = true;
                        }
                        if !matched {
                            type_counts.other += deck_entry.quantity;
                        }
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

    pub fn validate_commander(
        &self,
        deck_text: &str,
        commander_name: &str,
    ) -> Result<CommanderValidation, AppError> {
        self.card_lookup.ensure_ready()?;

        let deck_entries = parse_decklist(deck_text)?;
        let deck_size = deck_entries
            .iter()
            .filter(|deck_entry| deck_entry.card_name != commander_name)
            .map(|deck_entry| deck_entry.quantity)
            .sum::<usize>();

        let commander_info = self.card_lookup.lookup_card(commander_name)?;
        let commander_found = commander_info.is_some();
        let commander_colors = match commander_info {
            Some(card_info) => parse_color_identity(card_info.color_identity)?,
            None => Vec::new(),
        };

        let mut quantities: HashMap<String, usize> = HashMap::new();
        let mut type_lines: HashMap<String, String> = HashMap::new();
        let mut missing_cards = HashSet::new();
        let mut missing_card_names = Vec::new();
        let mut off_color_cards = HashSet::new();

        for deck_entry in deck_entries
            .iter()
            .filter(|deck_entry| deck_entry.card_name != commander_name)
        {
            let quantity = quantities.entry(deck_entry.card_name.clone()).or_insert(0);
            *quantity += deck_entry.quantity;

            match self.card_lookup.lookup_card(&deck_entry.card_name)? {
                Some(card_info) => {
                    let colors = parse_color_identity(card_info.color_identity)?;
                    if commander_found {
                        for color in colors {
                            if !commander_colors.contains(&color) {
                                off_color_cards.insert(deck_entry.card_name.clone());
                            }
                        }
                    }

                    type_lines
                        .entry(deck_entry.card_name.clone())
                        .or_insert_with(|| card_info.type_line.unwrap_or_default());
                }
                None => {
                    if missing_cards.insert(deck_entry.card_name.clone()) {
                        missing_card_names.push(deck_entry.card_name.clone());
                    }
                }
            }
        }

        let mut duplicate_cards = Vec::new();
        for (card_name, quantity) in quantities {
            if quantity <= 1 {
                continue;
            }
            let Some(type_line) = type_lines.get(&card_name) else {
                continue;
            };
            if !type_line.contains("Basic Land") {
                duplicate_cards.push(card_name);
            }
        }

        duplicate_cards.sort();
        let mut off_color_card_names = off_color_cards.into_iter().collect::<Vec<String>>();
        off_color_card_names.sort();

        let valid = commander_found
            && deck_size == 99
            && duplicate_cards.is_empty()
            && off_color_card_names.is_empty()
            && missing_card_names.is_empty();

        Ok(CommanderValidation {
            commander_name: commander_name.to_string(),
            commander_found,
            deck_size,
            duplicate_cards,
            off_color_cards: off_color_card_names,
            missing_cards: missing_card_names,
            valid,
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

pub struct CommanderValidation {
    pub commander_name: String,
    pub commander_found: bool,
    pub deck_size: usize,
    pub duplicate_cards: Vec<String>,
    pub off_color_cards: Vec<String>,
    pub missing_cards: Vec<String>,
    pub valid: bool,
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

fn parse_color_identity(color_identity: Option<String>) -> Result<Vec<String>, AppError> {
    let Some(color_identity) = color_identity else {
        return Err(AppError::StaleCardLookup);
    };
    if color_identity.is_empty() {
        return Err(AppError::StaleCardLookup);
    }

    serde_json::from_str(&color_identity).map_err(|_| AppError::StaleCardLookup)
}
