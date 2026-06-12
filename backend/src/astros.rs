use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AstrosError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
}

#[derive(Debug, Deserialize)]
struct Ll2Response {
    count: u32,
    results: Vec<Ll2Astronaut>,
}

#[derive(Debug, Deserialize)]
struct Ll2Astronaut {
    name: String,
    nationality: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Astronaut {
    pub name: String,
    pub nationality: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AstrosSummary {
    pub count: u32,
    pub people: Vec<Astronaut>,
}

pub async fn fetch_astros(client: &Client) -> Result<AstrosSummary, AstrosError> {
    let resp: Ll2Response = client
        .get("https://ll.thespacedevs.com/2.2.0/astronaut/?in_space=true&limit=20")
        .header(
            "User-Agent",
            "Astraeusio/1.0 (https://astraeusio.com; hello@astraeusio.com)",
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let people = resp
        .results
        .into_iter()
        .map(|a| Astronaut {
            name: a.name,
            nationality: a.nationality,
        })
        .collect();

    Ok(AstrosSummary {
        count: resp.count,
        people,
    })
}
