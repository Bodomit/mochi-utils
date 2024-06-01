use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ops::Deref;
use std::sync::Arc;

use regex::Regex;
use serde::Deserialize;
use tokio::task::JoinSet;

use crate::models::{Card, Deck, PaginatedResponse, Template};

mod models;

#[derive(Debug, Clone)]
pub struct Config {
    pub mochi_key: String,
}

const MOCHI_BASE: &str = "https://app.mochi.cards/api/";

// LIST

async fn list<T>(
    endpoint: String,
    additional_args: &HashMap<String, serde_json::Value>,
    config: &Config,
    limit: Option<usize>,
) -> Result<Box<[T]>, Box<dyn Error>>
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

        if limit.is_some() {
            let limit = limit.unwrap();
            if mochi_objects.len() >= limit {
                mochi_objects.truncate(limit);
                return Ok(mochi_objects.into_boxed_slice());
            }
        }
    }

    Ok(mochi_objects.into_boxed_slice())
}

pub async fn list_decks(config: &Config) -> Result<Box<[Deck]>, Box<dyn Error>> {
    let additional_args = HashMap::new();
    let decks = list("decks".to_string(), &additional_args, config, None).await?;
    Ok(decks)
}

pub async fn list_templates(config: &Config) -> Result<Box<[Template]>, Box<dyn Error>> {
    let additional_args = HashMap::new();
    let templates = list("templates".to_string(), &additional_args, config, None).await?;
    Ok(templates)
}

pub async fn list_cards(
    config: &Config,
    deck_id: String,
    limit: Option<usize>,
) -> Result<Box<[Card]>, Box<dyn Error>> {
    let additional_args = HashMap::from([
        (
            "deck-id".to_string(),
            serde_json::to_value(deck_id).unwrap(),
        ),
        ("limit".to_string(), serde_json::to_value(100).unwrap()),
    ]);
    let cards = list("cards".to_string(), &additional_args, config, limit).await?;
    Ok(cards)
}

// Update Cards.
pub async fn update_card(
    config: Arc<Config>,
    cards: Arc<[Card]>,
    index: usize,
) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let card = cards[index].clone();
    let url = format!("{}{}{}", MOCHI_BASE, "cards/", card.id);
    client
        .post(url)
        .basic_auth(&config.mochi_key, Some(""))
        .json(&card)
        .send()
        .await?;
    Ok(())
}

pub async fn update_cards(config: &Config, cards: &Box<[Card]>) -> Result<(), Box<dyn Error>> {
    let config: Arc<Config> = Arc::from(config.clone());
    let cards: Arc<[Card]> = Arc::from(cards.deref());

    let mut tasks = JoinSet::new();
    for i in 0..cards.len() {
        tasks.spawn(update_card(Arc::clone(&config), Arc::clone(&cards), i));
    }

    let mut completed = 0u32;

    // Join and process the results.
    while let Some(res) = tasks.join_next().await {
        let result = res.unwrap();
        match result {
            Ok(_) => {
                completed = completed + 1;
                let percent = (completed as f32 / cards.len() as f32) * 100f32;
                println!("Progress: {}/{} {}%", completed, cards.len(), percent);
            }
            Err(err) => {
                print!("Error: {:#?}", err);
            }
        };
    }

    Ok(())
}

// Japanese String
#[derive(Debug, Clone)]
pub struct KanaString(String);

impl KanaString {
    pub fn iter_mora(&self) -> impl Iterator<Item = String> {
        let mut chars = self.0.chars().peekable();

        let ignore_list: HashSet<char> = HashSet::from([
            'ぁ', 'ぃ', 'ぅ', 'ぇ', 'ぉ', 'っ', 'ゃ', 'ゅ', 'ょ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ',
            'ッ', 'ャ', 'ュ', 'ョ', 'ヮ',
        ]);

        let mut morae = vec![];
        let mut mora = vec![];
        while let Some(c) = chars.next() {
            mora.push(c);

            let next_c = chars.peek();

            if next_c.is_some() && ignore_list.contains(next_c.unwrap()) {
                continue;
            }

            morae.push(mora.iter().collect::<String>());
            mora.clear();
        }

        morae.into_iter()
    }
}

impl From<String> for KanaString {
    fn from(string: String) -> Self {
        KanaString { 0: string }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_iter_mora() {
        // <-- actual test
        let s1 = KanaString::from("サッカー".to_string())
            .iter_mora()
            .collect::<Vec<_>>();
        assert_eq!(s1.len(), 3);
        assert_eq!(s1[0], "サッ");
        assert_eq!(s1[1], "カ");
        assert_eq!(s1[2], "ー");

        let s2 = KanaString::from("れっしゃ".to_string())
            .iter_mora()
            .collect::<Vec<_>>();
        assert_eq!(s2.len(), 2);
        assert_eq!(s2[0], "れっ");
        assert_eq!(s2[1], "しゃ");
    }
}

// Accents
pub type Word = String;

#[derive(Debug, Clone)]
pub enum AccentType {
    Heiban,
    Atamadaka,
    Nakadaka(usize),
    Odaka,
}

#[derive(Debug, Clone)]
pub struct Accent {
    pub accent_type: AccentType,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccentDefinition {
    kana: KanaString,
    accents: Vec<Accent>,
}

impl AccentDefinition {
    pub fn get_html(&self, accent_map: &AccentMap) -> String {
        let mora = self.kana.iter_mora().collect::<Vec<_>>();
        mora[0].clone()
    }
}

pub type AccentMap = HashMap<Word, Vec<AccentDefinition>>;

pub fn load_accents() -> AccentMap {
    let raw = std::str::from_utf8(include_bytes!("../resources/accents.txt")).unwrap();
    let lines = raw.lines().collect::<Vec<_>>();

    let mut words = AccentMap::with_capacity(lines.len());
    let regex_note_ex = Regex::new(r"\((\w)\)").unwrap();
    let regex_index_ex = Regex::new(r"(\d+)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        let mut splits = line.split('\t');
        let word = splits.next().unwrap().to_string();
        let kana = splits.next().unwrap().to_string();

        let accents = splits
            .next()
            .unwrap()
            .split(',')
            .map(|s| {
                let note = regex_note_ex
                    .captures(s)
                    .and_then(|c| c.get(0))
                    .and_then(|c| Some(c.as_str().to_string()));

                let index = regex_index_ex
                    .captures(s)
                    .and_then(|c| c.get(0))
                    .and_then(|c| Some(c.as_str().parse::<usize>().unwrap()))
                    .unwrap();

                let accent_type = if index == 0 {
                    AccentType::Heiban
                } else if index == 1 {
                    AccentType::Atamadaka
                } else if index == s.len() - 1 {
                    AccentType::Odaka
                } else {
                    AccentType::Nakadaka(index)
                };

                Accent { accent_type, note }
            })
            .collect::<Vec<_>>();

        let accent_definition = AccentDefinition {
            kana: KanaString::from(if kana.is_empty() { word.clone() } else { kana }),
            accents,
        };

        let word_entry = words.entry(word).or_insert(vec![]);
        word_entry.push(accent_definition);

        if i % 10000 == 0 {
            let percentage = (i + 1) * 100 / lines.len();
            println!(
                "Loading Accents Progress: {}/{} ({:.0}%)",
                i + 1,
                lines.len(),
                percentage
            );
        }
    }

    words
}
