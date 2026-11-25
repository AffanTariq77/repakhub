//! GOG store browsing: reads GOG's public, unauthenticated catalogue (the one gog.com's own store pages use)
//! to show what GOG sells. Nothing is bought or downloaded here; each result links to its gog.com page.
//! See "Store listings" in docs/SOURCE_POLICY.md.

use crate::downloads::{self, Transport};
use serde::{Deserialize, Serialize};

const CATALOG: &str = "https://catalog.gog.com/v1/catalog";
/// A page of 48 results is about 500 KB.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StoreGame {
    id: String,
    title: String,
    /// Always an `https://www.gog.com/` page, so the UI may open it.
    store_link: String,
    /// Always on `https://images.gog-statics.com/`.
    cover: Option<String>,
    /// Display price in GOG's formatting, e.g. `$4.19`. `None` for free or unpriced items.
    price: Option<String>,
    /// Display discount, e.g. `-30%`.
    discount: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    products: Vec<Product>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Product {
    id: String,
    title: String,
    store_link: String,
    cover_horizontal: Option<String>,
    price: Option<Price>,
}

#[derive(Deserialize)]
struct Price {
    #[serde(rename = "final")]
    final_: Option<String>,
    discount: Option<String>,
}

/// Search GOG's catalogue; an empty query lists the store's top-rated games.
#[tauri::command]
pub async fn search_gog(query: String) -> Result<Vec<StoreGame>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let query = query.trim();
        let mut params = vec![("limit", "48".to_string()), ("order", "desc:score".into()), ("productType", "in:game,pack".into()), ("locale", "en-US".into())];
        if !query.is_empty() {
            params.push(("query", format!("like:{query}")));
        }
        let url = url::Url::parse_with_params(CATALOG, &params).map_err(|e| e.to_string())?;
        let text = downloads::fetch_text(&Transport::new(), url.as_str(), MAX_BYTES).map_err(|e| format!("GOG: {e}"))?;
        parse(&text)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn parse(text: &str) -> Result<Vec<StoreGame>, String> {
    let page: Page = serde_json::from_str(text).map_err(|e| format!("unexpected response from GOG: {e}"))?;
    Ok(page
        .products
        .into_iter()
        // The UI opens these links, so only gog.com pages and GOG's image host are passed on.
        .filter(|p| p.store_link.starts_with("https://www.gog.com/"))
        .map(|p| StoreGame {
            id: p.id,
            title: p.title,
            store_link: p.store_link,
            cover: p.cover_horizontal.filter(|c| c.starts_with("https://images.gog-statics.com/")),
            price: p.price.as_ref().and_then(|pr| pr.final_.clone()),
            discount: p.price.and_then(|pr| pr.discount),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_catalog_and_drops_foreign_links() {
        let page = r#"{"pages":1,"products":[
            {"id":"1213176929","title":"Reigns: The Witcher","storeLink":"https://www.gog.com/en/game/reigns_the_witcher",
             "coverHorizontal":"https://images.gog-statics.com/a.png","price":{"final":"$4.19","discount":"-30%"},"extra":[]},
            {"id":"2","title":"Free","storeLink":"https://www.gog.com/en/game/free","coverHorizontal":"https://evil.example/a.png","price":null},
            {"id":"3","title":"Elsewhere","storeLink":"https://evil.example/game"}]}"#;
        let games = parse(page).unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!((games[0].price.as_deref(), games[0].discount.as_deref()), (Some("$4.19"), Some("-30%")));
        assert_eq!(games[1].cover, None);
        assert!(parse("<html>").is_err());
    }

    /// Hits gog.com: `cargo test -- --ignored live_gog_search`.
    #[test]
    #[ignore]
    fn live_gog_search() {
        let games = tauri::async_runtime::block_on(search_gog("witcher".into())).unwrap();
        assert!(games.iter().any(|g| g.title.contains("Witcher")));
    }
}
