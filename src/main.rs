use std::env;

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

mod youtube;
mod util;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[group]
#[commands(play, skip, playtop, remove)]
struct General;

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Discord token");
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
    let url = match youtube::get_url_from_msg(msg).await {
        Ok(url) => url,
        Err(why) => {
            check_msg(msg.channel_id.say(&ctx.http, why).await);
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
async fn playtop(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let url = match youtube::get_url_from_msg(msg).await {
        Ok(url) => url,
        Err(why) => {
            check_msg(msg.channel_id.say(&ctx.http, why).await);
            return Ok(());
        }
    };

    let util::GuildData { handler_lock, .. } = match util::get_guild_data(ctx, msg).await {
        Ok(guild_data) => guild_data,
        Err(why) => {
            check_msg(msg.channel_id.say(&ctx.http, why).await);
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

    let queue = handler.queue();

    // 0th index is currently playing
    // if we queue a song while one is playing, our queue is now size two - don't do anything
    // else, pop the newly queued track and put it in the "front" - behind the currently playing track
    if queue.len() > 2 {
        queue.modify_queue(|q| {
            match q.pop_back() {
                Some(track) => {
                    q.insert(1, track)
                },
                None => ()
            }
        });
    }

    check_msg(
        msg.channel_id
            .say(&ctx.http, "Put your song on top of the queue")
            .await,
    );
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let index = match args.single::<i32>() {
        Ok(arg) => arg,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "You need to provide a valid index")
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

    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    let track = match queue.modify_queue(|q| q.remove(index as usize)) {
        Some(track) => track,
        None => {
            check_msg(msg.channel_id.say(&ctx.http, "Index out of bounds").await);
            return Ok(());
        }
    };

    track.stop().expect("Error while trying to stop track");

    Ok(())
}

#[command]
#[num_args(2)]
#[aliases("move")]
#[only_in(guilds)]
async fn move_track(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let first_index = match args.single::<usize>() {
        Ok(arg) => arg,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "You need to provide a first index")
                    .await,
            );
            return Ok(());
        }
    };

    let second_index = match args.single::<usize>() {
        Ok(arg) => arg,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "You need to provide a second index")
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

    let handler = handler_lock.lock().await;
    let queue = handler.queue();
    
    queue.modify_queue(|q| q.swap(first_index, second_index));

    Ok(())
}

fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
