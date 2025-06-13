// src/markets/errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MarketSimulationError {
    #[error("Failed to parse amount for {market} route (field: {field}): {value} - {source}")]
    AmountParseError {
        market: String,
        value: String,
        field: String, // "outAmount" or "otherAmountThreshold"
        #[source]
        source: std::num::ParseIntError,
    },
    
    #[error("Missing required field '{field}' in {market} API response")]
    MissingField {
        market: String,
        field: String,
    },
    
    #[error("API request failed for {market}: {message}")]
    ApiRequestFailed {
        market: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>, // Made source optional and boxed
    },
    
    #[error("Invalid response format from {market} API: {details}")]
    InvalidResponseFormat {
        market: String,
        details: String, // Can store serde_json::Error::to_string() here
    },
    
    #[error("Route not found for {market} - {reason}")]
    NoRouteFound {
        market: String,
        reason: String,
    },

    #[error("Token info not found for mint: {mint}")] // Kept from my original, might be useful
    TokenInfoNotFound { mint: String },
}

// Helper to convert reqwest::Error into MarketSimulationError
impl From<(reqwest::Error, String)> for MarketSimulationError {
    fn from(item: (reqwest::Error, String)) -> Self {
        MarketSimulationError::ApiRequestFailed {
            market: item.1,
            message: item.0.to_string(),
            source: Some(Box::new(item.0)),
        }
    }
}

// Helper to convert serde_json::Error into MarketSimulationError
impl From<(serde_json::Error, String)> for MarketSimulationError {
    fn from(item: (serde_json::Error, String)) -> Self {
        MarketSimulationError::InvalidResponseFormat {
            market: item.1,
            details: item.0.to_string(),
        }
    }
}
