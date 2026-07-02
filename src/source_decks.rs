use crate::error::AppError;
use serde::Deserialize;
use std::time::Duration;

pub struct SourceDeck {
    pub name: String,
    pub source: String,
    pub source_url: String,
    pub cards: Vec<SourceDeckCard>,
}

pub struct SourceDeckCard {
    pub card_name: String,
    pub quantity: usize,
    pub category: Option<String>,
}

pub struct ArchidektClient;

impl ArchidektClient {
    pub fn load_deck(&self, url: &str) -> Result<SourceDeck, AppError> {
        let api_url = self.deck_api_url(url)?;
        let json_text = if let Some(file_path) = api_url.strip_prefix("file://") {
            std::fs::read_to_string(file_path)?
        } else {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(20))
                .user_agent("deck-analyzer/0.1 local CLI")
                .build()?
                .get(&api_url)
                .send()?
                .error_for_status()?
                .text()?
        };

        parse_archidekt_deck(&json_text, url)
    }

    pub fn deck_api_url(&self, url: &str) -> Result<String, AppError> {
        if url.starts_with("file://") {
            return Ok(url.to_string());
        }
        if url.contains("/api/decks/") {
            return Ok(url.to_string());
        }

        let Some((base_url, path)) = url.split_once("/decks/") else {
            return Err(AppError::InvalidSourceDeckFormat(
                "Archidekt source expects a deck page URL or API deck URL".to_string(),
            ));
        };

        let Some(deck_id) = path.split('/').next() else {
            return Err(AppError::InvalidSourceDeckFormat(
                "Archidekt deck URL is missing a deck id".to_string(),
            ));
        };
        if deck_id.is_empty() || !deck_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(AppError::InvalidSourceDeckFormat(
                "Archidekt deck URL has an invalid deck id".to_string(),
            ));
        }

        Ok(format!("{base_url}/api/decks/{deck_id}/"))
    }
}

#[derive(Deserialize)]
struct ArchidektDeckResponse {
    name: String,
    cards: Vec<ArchidektCardRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchidektCardRow {
    quantity: usize,
    categories: Option<Vec<String>>,
    card: ArchidektCardPrint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchidektCardPrint {
    oracle_card: ArchidektOracleCard,
}

#[derive(Deserialize)]
struct ArchidektOracleCard {
    name: String,
}

pub fn parse_archidekt_deck(json_text: &str, source_url: &str) -> Result<SourceDeck, AppError> {
    let response: ArchidektDeckResponse = serde_json::from_str(json_text).map_err(|error| {
        AppError::InvalidSourceDeckFormat(format!("Archidekt deck JSON is invalid: {error}"))
    })?;

    let mut cards = Vec::new();
    for card in response.cards {
        let category = card
            .categories
            .and_then(|categories| categories.into_iter().next());

        cards.push(SourceDeckCard {
            card_name: card.card.oracle_card.name,
            quantity: card.quantity,
            category,
        });
    }

    if cards.is_empty() {
        return Err(AppError::InvalidSourceDeckFormat(
            "Archidekt deck JSON did not contain any cards".to_string(),
        ));
    }

    Ok(SourceDeck {
        name: response.name,
        source: "archidekt".to_string(),
        source_url: source_url.to_string(),
        cards,
    })
}
