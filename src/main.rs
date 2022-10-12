use std::{collections::HashMap, env};

use serde_json::Value;
use serenity::{
    async_trait,
    client::Context,
    client::{Client, EventHandler},
    framework::{
        standard::{
            macros::{command, group},
            Args, CommandResult,
        },
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready},
    prelude::GatewayIntents,
    Result as SerenityResult,
};

use songbird::{input::Restartable, SerenityInit};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[group]
#[commands(play, skip, playtop)]
struct General;

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("token");
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("$"))
        .group(&GENERAL_GROUP);
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("An error occurred: ${:?}", why);
    }
}

#[command]
#[only_in(guilds)]
async fn play(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let query = match  msg.content.get(6..) {
        Some(query) => query,
        None => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide a query")
                    .await,
            );
            return Ok(());
        }
    };

    let url = if query.starts_with("http") {
        query.to_string()
    } else {
        let json = match search_youtube(query).await {
            Ok(video_id) => video_id,
            Err(_) => {
                check_msg(
                    msg.channel_id
                        .say(&ctx.http, "There was an error searching YouTube")
                        .await,
                );
                return Ok(());
            }
        };
        match get_video_from_json(&json) {
            Some(id) => id,
            None => {
                check_msg(
                    msg.channel_id
                        .say(&ctx.http, "There was an error parsing YouTube results")
                        .await,
                );
                return Ok(());
            }
        }
    }; 

    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone(); 

    let handler_lock = match manager.get(guild_id) {
        Some(handler_lock) => handler_lock,
        None => {
            let channel_id = guild
                .voice_states
                .get(&msg.author.id)
                .and_then(|voice_state| voice_state.channel_id);

            let channel = match channel_id {
                Some(channel) => channel,
                None => {
                    check_msg(
                        msg.channel_id
                            .say(&ctx.http, "Not in a voice channel")
                            .await,
                    );
                    return Ok(());
                }
            };

            let (handler_lock, _) = manager.join(guild_id, channel).await;
            handler_lock
        }
    };

    let mut handler = handler_lock.lock().await;

    let source = match Restartable::ytdl(url, true).await {
        Ok(source) => source,
        Err(why) => {
            println!("Err starting source: {:?}", why);

            check_msg(msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await);

            return Ok(());
        }
    };

    handler.enqueue_source(source.into());

    check_msg(
        msg.channel_id
            .say(
                &ctx.http,
                format!("Added song to queue: position {}", handler.queue().len()),
            )
            .await,
    );

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn skip(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let handler_lock = match manager.get(guild_id) {
        Some(handler_lock) => handler_lock,
        None => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Not in a voice channel")
                    .await,
            );
            return Ok(());
        }
    };

    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    match queue.skip() {
        Ok(_) => (),
        Err(why) => {
            check_msg(
                msg.channel_id
                    .say(
                        &ctx.http,
                        format!("There was an error skipping the queue: ${:?}", why),
                    )
                    .await,
            );
            return Ok(());
        }
    };
    check_msg(msg.channel_id.say(&ctx.http, "Skipped").await);
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn playtop(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let url = match check_url(msg, args) {
        Ok(url) => url,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide a valid URL")
                    .await,
            );
            return Ok(());
        }
    };

    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let handler_lock = match manager.get(guild_id) {
        Some(handler_lock) => handler_lock,
        None => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Not in a voice channel")
                    .await,
            );
            return Ok(());
        }
    };

    let mut handler = handler_lock.lock().await;

    let source = match Restartable::ytdl(url, true).await {
        Ok(source) => source,
        Err(why) => {
            println!("Err starting source: {:?}", why);

            check_msg(msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await);

            return Ok(());
        }
    };

    handler.enqueue_source(source.into());

    // swap the first and last in queue
    let queue = handler.queue();
    queue.modify_queue(|q| {
        let len = q.len();
        q.swap(1, len - 1);
    });

    check_msg(
        msg.channel_id
            .say(&ctx.http, "Put your song on top of the queue")
            .await,
    );
    Ok(())
}

async fn search_youtube(query: &str) -> Result<serde_json::Value, reqwest::Error> {
    let youtube_token = env::var("YOUTUBE_TOKEN").expect("token");
    let url = format!("https://youtube.googleapis.com/youtube/v3/search?q={}&key={}", query, youtube_token);
    let client = reqwest::Client::new();
    let json = client.get(url)
        .header("Accept", "application/json")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok(json)
}

fn get_video_from_json(json: &Value) -> Option<String> {
    let first_video = json.get("items")?.get(0)?;
    let video_id = first_video.get("id")?.get("videoId")?.to_string();
    Some(format!("https://www.youtube.com/watch?v={}", video_id.trim_matches('\"')))
}

fn check_url(_msg: &Message, mut args: Args) -> Result<String, ()> {
    match args.single::<String>() {
        Ok(url) => {
            if !url.starts_with("http") {
                return Err(());
            }
            Ok(url)
        }
        Err(_) => Err(()),
    }
}

fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
