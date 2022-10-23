use std::env;

use serde::Deserialize;
use serenity::model::prelude::Message;

#[derive(Deserialize)]
pub struct VideoId {
    #[serde(rename = "videoId")]
    pub video_id: String,
}

#[derive(Deserialize)]
pub struct Video {
    pub id: VideoId,
}

#[derive(Deserialize)]
pub struct YoutubeResponse {
    pub items: Vec<Video>,
}

pub async fn get_url_from_msg(msg: &Message) -> Result<String, &'static str> {
    let query = match msg.content.get(6..) {
        Some(query) => query,
        None => return Err("Must provide a query"),
    };

    if query.starts_with("http") {
        return Ok(query.to_string());
    }

    let resp = match request_youtube(query).await {
        Ok(resp) => resp,
        Err(why) => {
            println!("{}", why);
            return Err("Error requesting the YouTube API");
        }
    };

    let video_id = match resp.items.get(0) {
        Some(item) => &item.id.video_id,
        None => return Err("No results found"),
    };

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    Ok(url)
}

async fn request_youtube(query: &str) -> Result<YoutubeResponse, reqwest::Error> {
    let youtube_token = env::var("YOUTUBE_TOKEN").expect("Youtube token");
    let url = format!(
        "https://youtube.googleapis.com/youtube/v3/search?q={}&type=video&key={}",
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
