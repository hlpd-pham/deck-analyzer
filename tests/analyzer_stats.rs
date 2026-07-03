use deck_analyzer::analyzer::{Analyzer, CardInfo, CardLookup};
use deck_analyzer::error::AppError;

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
                roles: Vec::new(),
            },
            "Llanowar Elves" => CardInfo {
                type_line: Some("Creature - Elf Druid".to_string()),
                cmc: Some(1.0),
                color_identity: Some("[\"G\"]".to_string()),
                roles: vec!["ramp".to_string()],
            },
            "Boros Charm" => CardInfo {
                type_line: Some("Instant".to_string()),
                cmc: Some(2.0),
                color_identity: Some("[\"R\",\"W\"]".to_string()),
                roles: vec!["protection".to_string()],
            },
            _ => return Ok(None),
        };

        Ok(Some(card_info))
    }
}

#[test]
fn analyzes_color_identity_counts() {
    let analyzer = Analyzer {
        card_lookup: TestLookup,
    };
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
    assert_eq!(stats.role_counts.ramp, 2);
    assert_eq!(stats.role_counts.protection, 1);
}
