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
    pub id: String,
    pub content: String,
    #[serde(rename = "deck-id")]
    pub deck_id: String,
    pub tags: Vec<String>,
    pub references: Vec<String>,
    #[serde(rename = "template-id")]
    pub template_id: Option<String>,
    pub fields: Option<HashMap<String, CardField>>,
    #[serde(rename = "archived?")]
    pub archived: Option<bool>,
    #[serde(rename = "trashed?")]
    pub trashed: Option<Value>,
    #[serde(rename = "review-reverse?")]
    pub review_reverse: Option<bool>,
    pub pos: Option<String>,
    pub attachments: Option<Value>,
}

// API
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaginatedResponse<T> {
    pub bookmark: Option<String>,
    pub docs: Vec<T>,
}
