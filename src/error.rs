use std::fmt;

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Http(reqwest::Error),
    InvalidDeckLine { line_number: usize },
    InvalidQuantity { line_number: usize },
    InvalidSourceDeckFormat(String),
    MissingCardLookup,
    MissingCardRoles,
    StaleCardLookup,
    CommanderValidationFailed,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(error) => write!(f, "{error}"),
            AppError::Sqlite(error) => write!(f, "{error}"),
            AppError::Json(error) => write!(f, "{error}"),
            AppError::Http(error) => write!(f, "{error}"),
            AppError::InvalidDeckLine { line_number } => {
                write!(f, "line {line_number} is in the wrong format")
            }
            AppError::InvalidQuantity { line_number } => {
                write!(f, "line {line_number} has an invalid quantity")
            }
            AppError::InvalidSourceDeckFormat(message) => write!(f, "{message}"),
            AppError::MissingCardLookup => {
                write!(f, "card_lookup table is missing; run sync before analyze")
            }
            AppError::MissingCardRoles => {
                write!(f, "card_roles table is missing; run sync before analyze")
            }
            AppError::StaleCardLookup => {
                write!(f, "card_lookup table is stale; run sync before analyze")
            }
            AppError::CommanderValidationFailed => write!(f, "commander validation failed"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::Io(error)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        AppError::Sqlite(error)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::Json(error)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        AppError::Http(error)
    }
}
