use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// Primitive Mochi Types
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Deck {
    pub id: String,
    pub name: String,
    #[serde(rename = "parent-id")]
    pub parent_id: Option<String>,
    #[serde(rename = "template-id")]
    pub template_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateField {
    pub id: String,
    pub name: String,
    pub pos: String,
    pub options: Option<HashMap<String, Value>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub content: String,
    pub fields: Option<HashMap<String, TemplateField>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CardField {
    pub id: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Card {
    pub content: String,
    #[serde(rename = "deck-id")]
    pub deck_id: String,
    #[serde(rename = "template-id")]
    pub template_id: Option<String>,
    pub fields: Option<HashMap<String, CardField>>,
    #[serde(rename = "archived?", default)]
    pub archived: bool,
    #[serde(rename = "review-reverse?", default)]
    pub review_reverse: bool,
    pub pos: Option<String>,
    // Retrieval Only Values
    #[serde(skip_serializing)]
    pub id: String,
    #[serde(skip_serializing)]
    pub tags: Vec<String>,
    #[serde(skip_serializing)]
    pub references: Vec<String>,
    #[serde(skip_serializing)]
    pub attachments: Option<Value>,
    #[serde(rename = "trashed?", skip_serializing)]
    pub trashed: Option<Value>,
}

// API
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaginatedResponse<T> {
    pub bookmark: Option<String>,
    pub docs: Vec<T>,
}
