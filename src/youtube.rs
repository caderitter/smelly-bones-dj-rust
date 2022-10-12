use std::env;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct VideoId {
    #[serde(rename = "videoId")]
    pub video_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct Video {
    pub id: VideoId,
}

#[derive(Serialize, Deserialize)]
pub struct YoutubeResponse {
    pub items: Vec<Video>,
}

pub async fn search_youtube(query: &str) -> Result<YoutubeResponse, reqwest::Error> {
    let youtube_token = env::var("YOUTUBE_TOKEN").expect("token");
    let url = format!(
        "https://youtube.googleapis.com/youtube/v3/search?q={}&key={}",
        query, youtube_token
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await?
        .json::<YoutubeResponse>()
        .await?;
    Ok(resp)
}
