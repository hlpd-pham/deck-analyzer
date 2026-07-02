use crate::analyzer::DeckStats;
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

pub struct SourceDeckQuality {
    pub role_counts: RoleCounts,
    pub warnings: Vec<String>,
}

#[derive(Default)]
pub struct RoleCounts {
    pub ramp: usize,
    pub draw: usize,
    pub removal: usize,
    pub wipes: usize,
    pub tutors: usize,
    pub protection: usize,
    pub win_conditions: usize,
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

pub fn evaluate_source_deck(source_deck: &SourceDeck, stats: &DeckStats) -> SourceDeckQuality {
    let mut role_counts = RoleCounts::default();

    for card in &source_deck.cards {
        let category = card.category.as_deref().unwrap_or("").to_lowercase();
        let name = card.card_name.to_lowercase();
        let quantity = card.quantity;

        if category.contains("ramp")
            || category.contains("mana")
            || name.contains("signet")
            || name.contains("talisman")
            || matches!(
                card.card_name.as_str(),
                "Sol Ring"
                    | "Arcane Signet"
                    | "Cultivate"
                    | "Kodama's Reach"
                    | "Farseek"
                    | "Nature's Lore"
                    | "Three Visits"
                    | "Rampant Growth"
                    | "Llanowar Elves"
                    | "Elvish Mystic"
                    | "Fyndhorn Elves"
                    | "Birds of Paradise"
            )
        {
            role_counts.ramp += quantity;
        }

        if category.contains("draw")
            || category.contains("card advantage")
            || matches!(
                card.card_name.as_str(),
                "Rhystic Study"
                    | "Mystic Remora"
                    | "Beast Whisperer"
                    | "Harmonize"
                    | "Esper Sentinel"
                    | "Skullclamp"
                    | "Guardian Project"
                    | "The Great Henge"
            )
        {
            role_counts.draw += quantity;
        }

        if category.contains("removal")
            || matches!(
                card.card_name.as_str(),
                "Swords to Plowshares"
                    | "Path to Exile"
                    | "Nature's Claim"
                    | "Beast Within"
                    | "Chaos Warp"
                    | "Generous Gift"
                    | "Assassin's Trophy"
            )
        {
            role_counts.removal += quantity;
        }

        if category.contains("wipe")
            || category.contains("board wipe")
            || matches!(
                card.card_name.as_str(),
                "Wrath of God"
                    | "Damnation"
                    | "Blasphemous Act"
                    | "Toxic Deluge"
                    | "Cyclonic Rift"
                    | "Farewell"
            )
        {
            role_counts.wipes += quantity;
        }

        if category.contains("tutor")
            || name.contains("tutor")
            || matches!(
                card.card_name.as_str(),
                "Chord of Calling" | "Finale of Devastation" | "Green Sun's Zenith"
            )
        {
            role_counts.tutors += quantity;
        }

        if category.contains("protection")
            || matches!(
                card.card_name.as_str(),
                "Heroic Intervention"
                    | "Teferi's Protection"
                    | "Lightning Greaves"
                    | "Swiftfoot Boots"
                    | "Boros Charm"
            )
        {
            role_counts.protection += quantity;
        }

        if category.contains("win")
            || category.contains("finisher")
            || matches!(
                card.card_name.as_str(),
                "Triumph of the Hordes"
                    | "Craterhoof Behemoth"
                    | "Finale of Devastation"
                    | "Overwhelming Stampede"
                    | "Torment of Hailfire"
            )
        {
            role_counts.win_conditions += quantity;
        }
    }

    let mut warnings = Vec::new();
    if stats.lands < 35 {
        warnings.push(format!(
            "Mana base may be light: {} lands found, common Commander baseline is 35-40",
            stats.lands
        ));
    }
    if stats.lands > 40 {
        warnings.push(format!(
            "Mana base may be heavy: {} lands found, common Commander baseline is 35-40",
            stats.lands
        ));
    }

    let nonland_cards = stats.total_cards.saturating_sub(stats.lands);
    let expensive_nonlands = stats.mana_curve[5] + stats.mana_curve[6] + stats.mana_curve[7];
    if nonland_cards > 0 && expensive_nonlands * 3 > nonland_cards {
        warnings.push(format!(
            "Nonland curve is top-heavy: {expensive_nonlands} spells cost five or more"
        ));
    }

    if role_counts.ramp < 8 {
        warnings.push(format!(
            "Ramp package looks light: {} ramp cards found",
            role_counts.ramp
        ));
    }
    if role_counts.draw < 8 {
        warnings.push(format!(
            "Card draw package looks light: {} draw cards found",
            role_counts.draw
        ));
    }
    if role_counts.removal < 6 {
        warnings.push(format!(
            "Interaction package looks light: {} removal cards found",
            role_counts.removal
        ));
    }
    if role_counts.wipes < 2 {
        warnings.push(format!(
            "Board wipe count looks light: {} wipes found",
            role_counts.wipes
        ));
    }
    if !stats.missing_cards.is_empty() {
        warnings.push(format!(
            "Missing local card data for {} unique cards",
            stats.missing_cards.len()
        ));
    }

    SourceDeckQuality {
        role_counts,
        warnings,
    }
}
