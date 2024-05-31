mod models;

use crate::models::{Card, Deck, PaginatedResponse, Template};
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;

pub struct Config {
    pub mochi_key: String,
}

const MOCHI_BASE: &str = "https://app.mochi.cards/api/";

async fn list<T>(
    endpoint: String,
    additional_args: &HashMap<String, serde_json::Value>,
    config: &Config,
) -> Result<Vec<T>, Box<dyn Error>>
where
    T: for<'a> Deserialize<'a> + std::fmt::Debug,
{
    let mut mochi_objects: Vec<T> = vec![];
    let client = reqwest::Client::new();
    let mut bookmark: Option<String> = None;
    let mut page_count = 1u32;
    loop {

        println!("Page {}", page_count);
        page_count = page_count + 1;

        let url = format!("{}{}", MOCHI_BASE, endpoint);
        let mut query_args = additional_args
            .into_iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        if bookmark.is_some() {
            let bookmark = bookmark.clone().unwrap();
            query_args.push((
                "bookmark".to_string(),
                serde_json::to_value(bookmark).unwrap(),
            ));
        }

        let resp = client
            .get(url)
            .basic_auth(&config.mochi_key, Some(""))
            .query(&query_args)
            .send()
            .await?;

        let page = resp.json::<PaginatedResponse<T>>().await?;


        if page.docs.len() == 0 {
            break;
        }

        mochi_objects.extend(page.docs);
        bookmark = page.bookmark;
    }

    Ok(mochi_objects)
}

pub async fn list_decks(config: &Config) -> Result<Box<[Deck]>, Box<dyn Error>> {
    let additional_args = HashMap::new();
    let decks: Vec<Deck> = list("decks".to_string(), &additional_args, config).await?;
    Ok(decks.into_boxed_slice())
}

pub async fn list_templates(config: &Config) -> Result<Box<[Template]>, Box<dyn Error>> {
    let additional_args = HashMap::new();
    let templates: Vec<Template> = list("templates".to_string(), &additional_args, config).await?;
    Ok(templates.into_boxed_slice())
}

pub async fn list_cards(config: &Config, deck_id: String) -> Result<Box<[Card]>, Box<dyn Error>> {
    let additional_args = HashMap::from([
        (
            "deck-id".to_string(),
            serde_json::to_value(deck_id).unwrap(),
        ),
        ("limit".to_string(), serde_json::to_value(100).unwrap()),
    ]);
    let cards: Vec<Card> = list("cards".to_string(), &additional_args, config).await?;
    Ok(cards.into_boxed_slice())
}
