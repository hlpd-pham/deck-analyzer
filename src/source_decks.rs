use crate::error::AppError;
use crate::types::CardRole;
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

pub struct ArchidektDeckSearchQuery {
    pub commander_name: Option<String>,
    pub name: Option<String>,
    pub owner_username: Option<String>,
    pub deck_format: Option<u8>,
    pub edh_bracket: Option<u8>,
    pub order_by: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchidektDeckSearchPage {
    pub count: usize,
    pub next: Option<String>,
    pub results: Vec<ArchidektDeckSearchResult>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchidektDeckSearchResult {
    pub id: usize,
    pub name: String,
    pub size: Option<usize>,
    pub updated_at: Option<String>,
    pub created_at: Option<String>,
    pub deck_format: Option<u8>,
    pub edh_bracket: Option<u8>,
    pub view_count: Option<usize>,
    pub owner: ArchidektDeckOwner,
}

#[derive(Deserialize)]
pub struct ArchidektDeckOwner {
    pub username: String,
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

    pub fn search_decks(
        &self,
        query: &ArchidektDeckSearchQuery,
    ) -> Result<ArchidektDeckSearchPage, AppError> {
        let api_url = self.deck_search_api_url(query)?;
        let json_text = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("deck-analyzer/0.1 local CLI")
            .build()?
            .get(&api_url)
            .send()?
            .error_for_status()?
            .text()?;

        parse_archidekt_deck_search(&json_text)
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

    pub fn deck_search_api_url(
        &self,
        query: &ArchidektDeckSearchQuery,
    ) -> Result<String, AppError> {
        let mut url =
            reqwest::Url::parse("https://archidekt.com/api/decks/v3/").map_err(|error| {
                AppError::InvalidSourceDeckFormat(format!(
                    "Archidekt deck search URL is invalid: {error}"
                ))
            })?;

        {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(commander_name) = query
                .commander_name
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                query_pairs.append_pair("commanderName", commander_name);
            }
            if let Some(name) = query.name.as_deref().filter(|value| !value.is_empty()) {
                query_pairs.append_pair("name", name);
            }
            if let Some(owner_username) = query
                .owner_username
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                query_pairs.append_pair("ownerUsername", owner_username);
            }
            if let Some(deck_format) = query.deck_format {
                query_pairs.append_pair("deckFormat", &deck_format.to_string());
            }
            if let Some(edh_bracket) = query.edh_bracket {
                query_pairs.append_pair("edhBracket", &edh_bracket.to_string());
            }
            if let Some(order_by) = query.order_by.as_deref().filter(|value| !value.is_empty()) {
                query_pairs.append_pair("orderBy", order_by);
            }
            if let Some(page) = query.page {
                query_pairs.append_pair("page", &page.to_string());
            }
            if let Some(page_size) = query.page_size {
                query_pairs.append_pair("pageSize", &page_size.to_string());
            }
        }

        Ok(url.to_string())
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

pub fn parse_archidekt_deck_search(json_text: &str) -> Result<ArchidektDeckSearchPage, AppError> {
    serde_json::from_str(json_text).map_err(|error| {
        AppError::InvalidSourceDeckFormat(format!("Archidekt deck search JSON is invalid: {error}"))
    })
}

pub fn format_moxfield_export_line(card_name: &str, roles: &[CardRole]) -> String {
    let mut line = format!("1 {card_name}");
    let mut sorted_roles = roles.to_vec();
    sorted_roles.sort_by_key(|role| role.as_str());
    sorted_roles.dedup();
    for role in sorted_roles {
        line.push_str(" #!");
        line.push_str(role.as_str());
    }
    line
}
