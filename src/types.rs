#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CardRole {
    Ramp,
    CardDraw,
    Removal,
    MassRemoval,
    Tutor,
    Protection,
    WinCondition,
}

impl CardRole {
    pub fn known_values() -> &'static str {
        "ramp, card_draw, removal, mass_removal, tutor, protection, win_condition"
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CardRole::Ramp => "ramp",
            CardRole::CardDraw => "card_draw",
            CardRole::Removal => "removal",
            CardRole::MassRemoval => "mass_removal",
            CardRole::Tutor => "tutor",
            CardRole::Protection => "protection",
            CardRole::WinCondition => "win_condition",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "ramp" => Some(CardRole::Ramp),
            "card_draw" => Some(CardRole::CardDraw),
            "removal" | "targeted_removal" => Some(CardRole::Removal),
            "mass_removal" | "board_wipe" => Some(CardRole::MassRemoval),
            "tutor" => Some(CardRole::Tutor),
            "protection" => Some(CardRole::Protection),
            "win_condition" => Some(CardRole::WinCondition),
            _ => None,
        }
    }
}

impl std::str::FromStr for CardRole {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        CardRole::from_db_value(value).ok_or_else(|| {
            format!(
                "unknown card role {value}; expected one of: {}",
                CardRole::known_values()
            )
        })
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ScryfallCard {
    pub id: Option<String>,
    pub oracle_id: Option<String>,
    pub name: Option<String>,
    pub printed_name: Option<String>,
    pub lang: Option<String>,
    pub layout: Option<String>,
    pub mana_cost: Option<String>,
    pub cmc: Option<f64>,
    pub type_line: Option<String>,
    pub printed_type_line: Option<String>,
    pub oracle_text: Option<String>,
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub colors: Option<Vec<String>>,
    pub color_identity: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub set: Option<String>,
    pub set_name: Option<String>,
    pub collector_number: Option<String>,
    pub rarity: Option<String>,
}
