use crate::error::AppError;

#[derive(Debug)]
pub struct DeckEntry {
    pub quantity: usize,
    pub card_name: String,
}

pub fn parse_decklist(deck_text: &str) -> Result<Vec<DeckEntry>, AppError> {
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
