use deck_analyzer::analyzer::{Analyzer, CardInfo, CardLookup};
use deck_analyzer::error::AppError;

struct TestLookup;

impl CardLookup for TestLookup {
    fn ensure_ready(&self) -> Result<(), AppError> {
        Ok(())
    }

    fn lookup_card(&self, card_name: &str) -> Result<Option<CardInfo>, AppError> {
        let card_info = match card_name {
            "Ezuri, Renegade Leader" => CardInfo {
                type_line: Some("Legendary Creature - Elf Warrior".to_string()),
                cmc: Some(3.0),
                color_identity: Some("[\"G\"]".to_string()),
            },
            "Forest" => CardInfo {
                type_line: Some("Basic Land - Forest".to_string()),
                cmc: Some(0.0),
                color_identity: Some("[\"G\"]".to_string()),
            },
            "Sol Ring" => CardInfo {
                type_line: Some("Artifact".to_string()),
                cmc: Some(1.0),
                color_identity: Some("[]".to_string()),
            },
            "Llanowar Elves" => CardInfo {
                type_line: Some("Creature - Elf Druid".to_string()),
                cmc: Some(1.0),
                color_identity: Some("[\"G\"]".to_string()),
            },
            "Lightning Bolt" => CardInfo {
                type_line: Some("Instant".to_string()),
                cmc: Some(1.0),
                color_identity: Some("[\"R\"]".to_string()),
            },
            _ => return Ok(None),
        };

        Ok(Some(card_info))
    }
}

#[test]
fn validates_valid_commander_deck_with_basic_land_duplicates() {
    let analyzer = Analyzer {
        card_lookup: TestLookup,
    };
    let validation = analyzer
        .validate_commander(
            "
1 Ezuri, Renegade Leader
97 Forest
1 Sol Ring
1 Llanowar Elves
",
            "Ezuri, Renegade Leader",
        )
        .expect("validation should run");

    assert!(validation.valid);
    assert!(validation.commander_found);
    assert_eq!(validation.deck_size, 99);
    assert!(validation.duplicate_cards.is_empty());
    assert!(validation.off_color_cards.is_empty());
    assert!(validation.missing_cards.is_empty());
}

#[test]
fn invalidates_wrong_deck_size() {
    let analyzer = Analyzer {
        card_lookup: TestLookup,
    };
    let validation = analyzer
        .validate_commander(
            "
1 Ezuri, Renegade Leader
1 Forest
",
            "Ezuri, Renegade Leader",
        )
        .expect("validation should run");

    assert!(!validation.valid);
    assert_eq!(validation.deck_size, 1);
}

#[test]
fn invalidates_duplicate_non_basic_cards() {
    let analyzer = Analyzer {
        card_lookup: TestLookup,
    };
    let validation = analyzer
        .validate_commander(
            "
1 Ezuri, Renegade Leader
97 Forest
2 Sol Ring
",
            "Ezuri, Renegade Leader",
        )
        .expect("validation should run");

    assert!(!validation.valid);
    assert_eq!(validation.deck_size, 99);
    assert_eq!(validation.duplicate_cards, ["Sol Ring"]);
}

#[test]
fn invalidates_off_color_cards() {
    let analyzer = Analyzer {
        card_lookup: TestLookup,
    };
    let validation = analyzer
        .validate_commander(
            "
1 Ezuri, Renegade Leader
97 Forest
1 Sol Ring
1 Lightning Bolt
",
            "Ezuri, Renegade Leader",
        )
        .expect("validation should run");

    assert!(!validation.valid);
    assert_eq!(validation.off_color_cards, ["Lightning Bolt"]);
}

#[test]
fn invalidates_missing_commander() {
    let analyzer = Analyzer {
        card_lookup: TestLookup,
    };
    let validation = analyzer
        .validate_commander(
            "
1 Missing Commander
99 Forest
",
            "Missing Commander",
        )
        .expect("validation should run");

    assert!(!validation.valid);
    assert!(!validation.commander_found);
    assert_eq!(validation.deck_size, 99);
    assert!(validation.missing_cards.is_empty());
}

#[test]
fn invalidates_missing_deck_cards() {
    let analyzer = Analyzer {
        card_lookup: TestLookup,
    };
    let validation = analyzer
        .validate_commander(
            "
1 Ezuri, Renegade Leader
98 Forest
1 Missing Card
",
            "Ezuri, Renegade Leader",
        )
        .expect("validation should run");

    assert!(!validation.valid);
    assert_eq!(validation.deck_size, 99);
    assert_eq!(validation.missing_cards, ["Missing Card"]);
}
