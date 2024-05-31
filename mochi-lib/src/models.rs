use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// Primitive Mochi Types
#[derive(Debug, Serialize, Deserialize)]
pub struct Deck {
    pub id: String,
    pub name: String,
    #[serde(rename = "parent-id")]
    pub parent_id: Option<String>,
    #[serde(rename = "template-id")]
    pub template_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub pos: String,
    pub options: Option<HashMap<String, Value>>
}


#[derive(Debug, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub content: String,
    pub fields: Option<HashMap<String, Field>>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub content: String,
    #[serde(rename = "deck-id")]
    pub deck_id: String,
    pub tags: Vec<String>,
    pub references: Vec<String>,
    #[serde(rename = "template-id")]
    pub template_id: Option<String>,
    pub fields: Option<HashMap<String, Field>>,
}

// API
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub bookmark: Option<String>,
    pub docs: Vec<T>,
}
