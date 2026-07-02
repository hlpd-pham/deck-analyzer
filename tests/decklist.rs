use deck_analyzer::decklist::parse_decklist;
use deck_analyzer::error::AppError;

#[test]
fn parses_card_lines_and_ignores_blank_lines() {
    let entries = parse_decklist(
        "
1 Command Tower

2 Llanowar Elves
",
    )
    .expect("decklist should parse");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].quantity, 1);
    assert_eq!(entries[0].card_name, "Command Tower");
    assert_eq!(entries[1].quantity, 2);
    assert_eq!(entries[1].card_name, "Llanowar Elves");
}

#[test]
fn rejects_lines_without_quantity_and_name() {
    let error = parse_decklist("CommandTower").expect_err("line should fail");

    assert!(matches!(
        error,
        AppError::InvalidDeckLine { line_number: 1 }
    ));
}

#[test]
fn rejects_invalid_quantity() {
    let error = parse_decklist("1x Command Tower").expect_err("line should fail");

    assert!(matches!(
        error,
        AppError::InvalidQuantity { line_number: 1 }
    ));
}
