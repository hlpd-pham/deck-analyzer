use crate::error::AppError;

#[derive(Debug)]
pub struct DeckEntry {
    pub quantity: usize,
    pub card_name: String,
}

pub struct DecklistParser;

impl DecklistParser {
    pub fn parse(&self, deck_text: &str) -> Result<Vec<DeckEntry>, AppError> {
        let mut entries = Vec::new();

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

            entries.push(DeckEntry {
                quantity,
                card_name: card_name.trim().to_string(),
            });
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::AppError;

    use super::DecklistParser;

    #[test]
    fn parses_card_lines_and_ignores_blank_lines() {
        let parser = DecklistParser;
        let entries = parser
            .parse(
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
        let parser = DecklistParser;
        let error = parser.parse("CommandTower").expect_err("line should fail");

        assert!(matches!(
            error,
            AppError::InvalidDeckLine { line_number: 1 }
        ));
    }

    #[test]
    fn rejects_invalid_quantity() {
        let parser = DecklistParser;
        let error = parser
            .parse("1x Command Tower")
            .expect_err("line should fail");

        assert!(matches!(
            error,
            AppError::InvalidQuantity { line_number: 1 }
        ));
    }
}
