use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ops::Deref;
use std::sync::Arc;
use std::{cmp, env};

use regex::Regex;
use reqwest::Response;
use serde::Deserialize;
use serde_json::Value;
use tokio::task::JoinSet;

use crate::models::{Card, CardField, Deck, PaginatedResponse, Template};

mod models;

#[derive(Debug, Clone)]
pub struct Config {
    pub mochi_key: String,
}

impl Config {
    pub fn build() -> Result<Config, Box<dyn std::error::Error>> {
        let mochi_key = env::var("MOCHI_KEY")?;
        Ok(Config { mochi_key })
    }
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
    let mut errors = vec![];
    loop {
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

        match resp.error_for_status_ref() {
            Ok(_) => {}
            Err(err) => {
                let text = resp.text().await.unwrap();
                let json: Value = serde_json::from_str(text.as_str())?;
                errors.push(format!("Error {:#?} with body {:#?}", err, json));
                continue;
            }
        }

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

    if errors.is_empty() {
        Ok(mochi_objects.into_boxed_slice())
    } else {
        Err(errors.join("\n").into())
    }
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
    deck_id: &String,
    limit: Option<usize>,
) -> Result<Box<[Card]>, Box<dyn Error>> {
    let per_call_limit = cmp::min(limit.unwrap_or(100), 100); // Max allowed is 100.
    let additional_args = HashMap::from([
        (
            "deck-id".to_string(),
            serde_json::to_value(deck_id).unwrap(),
        ),
        (
            "limit".to_string(),
            serde_json::to_value(per_call_limit).unwrap(),
        ),
    ]);
    let cards = list("cards".to_string(), &additional_args, config, limit).await?;
    Ok(cards)
}

// Update Cards.
pub async fn update_card(
    config: Arc<Config>,
    cards: Arc<[Card]>,
    index: usize,
) -> Result<Response, reqwest::Error> {
    let client = reqwest::Client::new();
    let card = cards[index].clone();
    let url = format!("{}{}{}", MOCHI_BASE, "cards/", card.id);
    let resp = client
        .post(url)
        .basic_auth(&config.mochi_key, Some(""))
        .json(&card)
        .send()
        .await;

    resp
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
    let mut errors = vec![];
    while let Some(res) = tasks.join_next().await {
        let result = res.unwrap().unwrap();

        match result.error_for_status_ref() {
            Ok(_) => {
                completed = completed + 1;
                let percent = (completed as f32 / cards.len() as f32) * 100f32;
                println!("Progress: {}/{} {}%", completed, cards.len(), percent);
            }
            Err(err) => {
                let body = result.text().await?;
                let json: Value = serde_json::from_str(body.as_str())?;
                println!("Error: {:#?} with {:#?}", err, json);
                errors.push(err);
            }
        };
    }

    if errors.len() > 0 {
        Err(errors
            .into_iter()
            .map(|e| format!("{:#?}", e))
            .collect::<Vec<_>>()
            .join("\n")
            .into())
    } else {
        Ok(())
    }
}

pub async fn add_pitch_accent_to_cards(
    config: &Config,
    cards: &Box<[Card]>,
    word_field_name: &String,
    pitch_accent_field_name: &String,
) -> Result<Box<[Card]>, Box<dyn Error>> {
    let accents = load_accents();
    let templates = list_templates(config).await?;
    let cards = cards
        .iter()
        .map(|card| {
            // Get the template.
            let template_id = card.template_id.as_ref().unwrap();
            let template_fields = templates
                .iter()
                .find(|t| t.id.eq(template_id))
                .and_then(|f| f.fields.clone());
            if template_fields.is_none() {
                return card.clone();
            }
            let template_fields = template_fields.as_ref().unwrap();

            // Get the word field.
            let word_field = template_fields
                .iter()
                .find(|(_, v)| v.name.eq(word_field_name));
            if word_field.is_none() {
                return card.clone();
            }
            let word_field = word_field.unwrap().1;

            // Get the pitch accent field.
            let pitch_accent_field = &template_fields
                .iter()
                .find(|(_, v)| v.name.eq(pitch_accent_field_name));
            if pitch_accent_field.is_none() {
                return card.clone();
            }
            let pitch_accent_field = pitch_accent_field.unwrap().1;

            let mut fields = card.fields.clone();
            if fields.is_none() {
                return card.clone();
            }
            let fields: &mut HashMap<std::string::String, CardField> = fields.as_mut().unwrap();
            let word = &fields.get(&word_field.id);
            if word.is_none() {
                return card.clone();
            }
            let word = &word.unwrap().value;
            let html = generate_html(word, &accents);
            let pitch_accent = CardField {
                id: pitch_accent_field.id.clone(),
                value: html,
            };
            fields.insert(pitch_accent_field.id.clone(), pitch_accent);

            let mut card = card.clone();
            card.fields = Some(fields.clone());
            card
        })
        .collect::<Vec<_>>();

    Ok(cards.into_boxed_slice())
}

// Japanese String
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
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

// Accents
pub type Word = String;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AccentType {
    Heiban,
    Atamadaka,
    Nakadaka(usize),
    Odaka,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MoraEdges {
    Top,
    Bottom,
    Left,
}

#[derive(Debug, Clone)]
pub struct Accent {
    pub accent_type: AccentType,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WordAccents {
    kana: KanaString,
    accents: Vec<Accent>,
}
pub fn load_accents() -> AccentMap {
    let raw = std::str::from_utf8(include_bytes!("../resources/accents.txt")).unwrap();
    let lines = raw.lines().collect::<Vec<_>>();

    let mut words = AccentMap::with_capacity(lines.len());
    let regex_note_ex = Regex::new(r"\(([\D]+)\)").unwrap();
    let regex_index_ex = Regex::new(r"(\d+)").unwrap();

    for line in lines.iter() {
        let mut splits = line.split('\t');
        let word = splits.next().unwrap().to_string();
        let kana = splits.next().unwrap().to_string();
        let kana = KanaString::from(if kana.is_empty() { word.clone() } else { kana });
        let n_mora = kana.iter_mora().collect::<Vec<_>>().len();

        let accents = splits
            .next()
            .unwrap()
            .split(',')
            .map(|s| {
                let note = regex_note_ex
                    .captures(s)
                    .and_then(|c| c.get(1))
                    .and_then(|c| Some(c.as_str().to_string()));

                let index = regex_index_ex
                    .captures(s)
                    .and_then(|c| c.get(1))
                    .and_then(|c| Some(c.as_str().parse::<usize>().unwrap()))
                    .unwrap();

                let accent_type = if index == 0 {
                    AccentType::Heiban
                } else if index == 1 {
                    AccentType::Atamadaka
                } else if index == n_mora {
                    AccentType::Odaka
                } else {
                    AccentType::Nakadaka(index)
                };

                Accent { accent_type, note }
            })
            .collect::<Vec<_>>();

        let accent_definition = WordAccents { kana, accents };

        let word_entry = words.entry(word).or_insert(vec![]);
        word_entry.push(accent_definition);
    }

    words
}

pub fn generate_html(word: &Word, accent_map: &AccentMap) -> String {
    let inner = accent_map
        .get(word)
        .unwrap_or(&vec![])
        .iter()
        .map(|wa| {
            wa.accents
                .iter()
                .map(|a| generate_html_for_accent(&wa.kana, a))
                .collect::<Vec<_>>()
                .join(&vec!['\u{30FB}'].iter().collect::<String>())
        })
        .collect::<Vec<_>>()
        .join("<div style=\"line-height:100%;\"><br></div>");

    format!("<div style=\"text-align: center\">{}</div>", inner)
}

fn generate_html_for_accent(kana_string: &KanaString, accent: &Accent) -> String {
    let mora_edges = generate_mora_edges(kana_string, &accent.accent_type);
    let kana_with_final_whitespace = KanaString::from(
        kana_string
            .0
            .chars()
            .chain(vec!['…'].into_iter())
            .collect::<String>(),
    );

    let mora_html = kana_with_final_whitespace
        .iter_mora()
        .zip(mora_edges)
        .map(|(mora, edges)| {
            let colour = "#FF6633";
            let width = "medium";
            let border_style = format!(": {} {} solid;", colour, width);
            let border_css = edges
                .iter()
                .map(|e| match e {
                    MoraEdges::Top => format!("BORDER-TOP{}", border_style),
                    MoraEdges::Bottom => format!("BORDER-BOTTOM{}", border_style),
                    MoraEdges::Left => format!("BORDER-LEFT{}", border_style),
                })
                .collect::<String>();

            format!("<span style=\"{}\">{}</span>", border_css, mora)
        })
        .collect::<String>();

    // If the accent has a note, prepend it to the html.
    if accent.note.is_some() {
        format!(
            "<span style=\"font-weight:bold\">{}: </span>{}",
            accent.note.clone().unwrap(),
            mora_html
        )
    } else {
        mora_html
    }
}

fn generate_mora_edges(kana_string: &KanaString, accent_type: &AccentType) -> Vec<Vec<MoraEdges>> {
    // Get the edges for the more itself.
    let n_mora = kana_string.iter_mora().collect::<Vec<_>>().len();
    let mut mora_edges = kana_string
        .iter_mora()
        .enumerate()
        .map(|(i, _)| match accent_type {
            AccentType::Heiban => match i {
                0 => vec![MoraEdges::Bottom],
                1 => vec![MoraEdges::Left, MoraEdges::Top],
                2.. => vec![MoraEdges::Top],
            },
            AccentType::Atamadaka => match i {
                0 => vec![MoraEdges::Top],
                1 => vec![MoraEdges::Left, MoraEdges::Bottom],
                2.. => vec![MoraEdges::Bottom],
            },
            AccentType::Nakadaka(idx) => match i {
                0 => vec![MoraEdges::Bottom],
                1 => vec![MoraEdges::Left, MoraEdges::Top],
                _ if i < *idx => vec![MoraEdges::Top],
                _ if i == *idx => vec![MoraEdges::Left, MoraEdges::Bottom],
                _ => vec![MoraEdges::Bottom],
            },
            AccentType::Odaka => match i {
                0 => {
                    if n_mora == 1 {
                        vec![MoraEdges::Top]
                    } else {
                        vec![MoraEdges::Bottom]
                    }
                }
                1 => vec![MoraEdges::Left, MoraEdges::Top],
                _ => vec![MoraEdges::Top],
            },
        })
        .collect::<Vec<Vec<MoraEdges>>>();

    // Insert the edges for the particle following the word.
    mora_edges.push(match accent_type {
        AccentType::Heiban => vec![MoraEdges::Top],
        AccentType::Atamadaka => vec![MoraEdges::Bottom],
        AccentType::Nakadaka(_) => vec![MoraEdges::Bottom],
        AccentType::Odaka => vec![MoraEdges::Left, MoraEdges::Bottom],
    });

    mora_edges
}

pub type AccentMap = HashMap<Word, Vec<WordAccents>>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_mochi_key() {
        // <-- actual test
        let config = Config::build().unwrap();
        assert!(!config.mochi_key.is_empty())
    }

    #[tokio::test]
    async fn test_list_decks() {
        let config = Config::build().unwrap();
        let decks = list_decks(&config).await.unwrap();
        assert!(!decks.is_empty());
    }

    #[tokio::test]
    async fn test_list_cards() {
        let config = Config::build().unwrap();
        let decks = list_decks(&config).await.unwrap();
        let n3_deck = decks.iter().find(|d| d.name == "N3");

        let cards = list_cards(&config, &n3_deck.unwrap().id, Some(10))
            .await
            .unwrap();
        assert!(!cards.is_empty());
    }

    #[tokio::test]
    async fn test_list_template() {
        let config = Config::build().unwrap();
        let templates = list_templates(&config).await.unwrap();
        assert!(!templates.is_empty());
    }

    #[tokio::test]
    async fn test_add_pitch_accent_to_cards() {
        let config = Config::build().unwrap();
        let decks = list_decks(&config).await.unwrap();
        let n3_deck = decks.iter().find(|d| d.id == "MK5LCEAL");

        let cards = list_cards(&config, &n3_deck.unwrap().id, Some(10))
            .await
            .unwrap();
        let cards = add_pitch_accent_to_cards(
            &config,
            &cards,
            &"Word".to_string(),
            &"PitchAccent".to_string(),
        )
        .await
        .unwrap();

        let result = update_cards(&config, &cards).await;
        match result {
            Ok(_) => {}
            Err(err) => println!("{:#?}", err),
        }
    }

    #[test]
    fn test_accent_notes() {
        let accents = load_accents();

        let t1 = &accents[&"かちかち".to_string()][0].accents;
        for accent in t1 {
            match accent.accent_type {
                AccentType::Heiban => {
                    assert_eq!("形動".to_string(), accent.note.clone().unwrap_or_default())
                }
                AccentType::Atamadaka => {
                    assert_eq!("副;名".to_string(), accent.note.clone().unwrap_or_default())
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_accent_type() {
        let accents = load_accents();

        let trials = vec![
            ("サッカー", "サッカー", vec![AccentType::Atamadaka]),
            ("箸", "はし", vec![AccentType::Atamadaka]),
            ("橋", "はし", vec![AccentType::Odaka]),
            ("端", "はし", vec![AccentType::Heiban]),
            ("鼻", "はな", vec![AccentType::Heiban]),
            ("花", "はな", vec![AccentType::Odaka]),
            (
                "あの方",
                "あのかた",
                vec![AccentType::Nakadaka(3), AccentType::Odaka],
            ),
        ];
        let trials = trials
            .iter()
            .map(|(w, k, v)| (w.to_string(), KanaString::from(k.to_string()), v))
            .collect::<Vec<_>>();

        for (word, kana, true_accents) in trials.iter() {
            let test_accents = &accents[word]
                .iter()
                .filter(|w| w.kana == *kana)
                .flat_map(|w| w.accents.clone())
                .map(|a| a.accent_type)
                .collect::<Vec<_>>();
            let true_accents: HashSet<&AccentType> = true_accents.iter().collect();

            assert_eq!(test_accents.len(), true_accents.len());
            for test_accent in test_accents {
                assert!(
                    true_accents.contains(test_accent),
                    "{:#?} in {:#?}",
                    test_accent,
                    true_accents
                )
            }
        }
    }

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

    #[test]
    fn test_generate_mora_edges() {
        let t = generate_mora_edges(&KanaString::from("き".to_string()), &AccentType::Odaka);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].len(), 1);
        assert_eq!(t[0][0], MoraEdges::Top);
        assert_eq!(t[1].len(), 2);
        assert_eq!(t[1][0], MoraEdges::Left);
        assert_eq!(t[1][1], MoraEdges::Bottom);

        let t = generate_mora_edges(&KanaString::from("かわ".to_string()), &AccentType::Odaka);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].len(), 1);
        assert_eq!(t[0][0], MoraEdges::Bottom);
        assert_eq!(t[1].len(), 2);
        assert_eq!(t[1][0], MoraEdges::Left);
        assert_eq!(t[1][1], MoraEdges::Top);
        assert_eq!(t[2].len(), 2);
        assert_eq!(t[2][0], MoraEdges::Left);
        assert_eq!(t[2][1], MoraEdges::Bottom);

        let t = generate_mora_edges(&KanaString::from("じかん".to_string()), &AccentType::Heiban);
        assert_eq!(t.len(), 4);
        assert_eq!(t[0].len(), 1);
        assert_eq!(t[0][0], MoraEdges::Bottom);
        assert_eq!(t[1].len(), 2);
        assert_eq!(t[1][0], MoraEdges::Left);
        assert_eq!(t[1][1], MoraEdges::Top);
        assert_eq!(t[2].len(), 1);
        assert_eq!(t[2][0], MoraEdges::Top);
        assert_eq!(t[3].len(), 1);
        assert_eq!(t[3][0], MoraEdges::Top);

        let t = generate_mora_edges(
            &KanaString::from("てんき".to_string()),
            &AccentType::Atamadaka,
        );
        assert_eq!(t.len(), 4);
        assert_eq!(t[0].len(), 1);
        assert_eq!(t[0][0], MoraEdges::Top);
        assert_eq!(t[1].len(), 2);
        assert_eq!(t[1][0], MoraEdges::Left);
        assert_eq!(t[1][1], MoraEdges::Bottom);
        assert_eq!(t[2].len(), 1);
        assert_eq!(t[2][0], MoraEdges::Bottom);
        assert_eq!(t[3].len(), 1);
        assert_eq!(t[3][0], MoraEdges::Bottom);

        let t = generate_mora_edges(
            &KanaString::from("ひとつ".to_string()),
            &AccentType::Nakadaka(2),
        );
        assert_eq!(t.len(), 4);
        assert_eq!(t[0].len(), 1);
        assert_eq!(t[0][0], MoraEdges::Bottom);
        assert_eq!(t[1].len(), 2);
        assert_eq!(t[1][0], MoraEdges::Left);
        assert_eq!(t[1][1], MoraEdges::Top);
        assert_eq!(t[2].len(), 2);
        assert_eq!(t[2][0], MoraEdges::Left);
        assert_eq!(t[2][1], MoraEdges::Bottom);
        assert_eq!(t[3].len(), 1);
        assert_eq!(t[3][0], MoraEdges::Bottom);

        let t = generate_mora_edges(
            &KanaString::from("こうじょう".to_string()),
            &AccentType::Nakadaka(3),
        );
        assert_eq!(t.len(), 5);
        assert_eq!(t[0].len(), 1);
        assert_eq!(t[0][0], MoraEdges::Bottom);
        assert_eq!(t[1].len(), 2);
        assert_eq!(t[1][0], MoraEdges::Left);
        assert_eq!(t[1][1], MoraEdges::Top);
        assert_eq!(t[2].len(), 1);
        assert_eq!(t[2][0], MoraEdges::Top);
        assert_eq!(t[3].len(), 2);
        assert_eq!(t[3][0], MoraEdges::Left);
        assert_eq!(t[3][1], MoraEdges::Bottom);
        assert_eq!(t[4].len(), 1);
        assert_eq!(t[3][1], MoraEdges::Bottom);
    }

    #[test]
    fn test_generate_html_for_accent() {
        let accents = load_accents();
        let t1 = &accents[&"あの方".to_string()][0];
        let r1 = generate_html_for_accent(
            &t1.kana,
            &t1.accents
                .iter()
                .find(|a| a.accent_type == AccentType::Nakadaka(3))
                .unwrap(),
        );
        assert_eq!(r1, "<span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">あ</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">か</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-BOTTOM: #FF6633 medium solid;\">た</span><span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">…</span>");

        let t2 = &accents[&"かちかち".to_string()][0];
        let r2 = generate_html_for_accent(
            &t2.kana,
            &t2.accents
                .iter()
                .find(|a| a.accent_type == AccentType::Heiban)
                .unwrap(),
        );

        assert_eq!(r2, "<span style=\"font-weight:bold\">形動: </span><span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">か</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-TOP: #FF6633 medium solid;\">ち</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">か</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">ち</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">…</span>");
    }

    #[test]
    fn test_generate_html() {
        let accents = load_accents();
        let t1 = generate_html(&"あの方".to_string(), &accents);
        assert_eq!(t1, "<div style=\"text-align: center\"><span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">あ</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">か</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-BOTTOM: #FF6633 medium solid;\">た</span><span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">…</span>・<span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">あ</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">か</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">た</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-BOTTOM: #FF6633 medium solid;\">…</span></div>");

        let t2 = generate_html(&"この後".to_string(), &accents);
        assert_eq!(t2, "<div style=\"text-align: center\"><span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">こ</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">あ</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-BOTTOM: #FF6633 medium solid;\">と</span><span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">…</span><div style=\"line-height:100%;\"><br></div><span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">こ</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">ち</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-BOTTOM: #FF6633 medium solid;\">…</span>・<span style=\"BORDER-BOTTOM: #FF6633 medium solid;\">こ</span><span style=\"BORDER-LEFT: #FF6633 medium solid;BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">の</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">ち</span><span style=\"BORDER-TOP: #FF6633 medium solid;\">…</span></div>");
    }
}
