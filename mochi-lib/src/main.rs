use mochi_lib::Config;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = build_config()?;
    //let n3_cards = mochi_lib::list_cards(&config, "MK5LCEAL".to_string()).await?;
    let templates = mochi_lib::list_templates(&config).await?;

    print!("{:#?}", templates);
    print!("N3 Cards: {}", templates.len());
    Ok(())
}

pub fn build_config() -> Result<Config, Box<dyn std::error::Error>> {
    let mochi_key = env::var("MOCHI_KEY")?;
    Ok(Config { mochi_key })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_mochi_key() {
        // <-- actual test
        let config = build_config();
        assert!(!config.unwrap().mochi_key.is_empty())
    }
}
