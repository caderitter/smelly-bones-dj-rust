use std::{env, sync::Arc};

use serenity::{
    async_trait,
    client::Context,
    client::{Client, EventHandler},
    framework::{
        standard::{
            macros::{command, group},
            Args, Command, CommandResult,
        },
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready},
    prelude::{GatewayIntents, Mutex},
    Result as SerenityResult,
};

use songbird::{input::Restartable, SerenityInit, Call, Songbird};
use tokio::sync::MutexGuard;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[group]
#[commands(play, queue, skip)]
struct General;

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("token");
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!"))
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
async fn play(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let url = match args.single::<String>() {
        Ok(url) => url,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "You must provide a URL")
                    .await,
            );
            return Ok(());
        }
    };

    let guild = match msg.guild(&ctx.cache) {
        Some(guild) => guild,
        None => {
            println!("Guild ID could not be found");
            return Ok(());
        }
    };
    let guild_id = guild.id;

    let manager = songbird::get(ctx)
        .await
        .expect("Manager could not be gotten")
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

    let source = match songbird::ytdl(&url).await {
        Ok(source) => source,
        Err(why) => {
            println!("Err starting source: {:?}", why);
            check_msg(msg.channel_id.say(&ctx.http, "Error sourcing ffmpeg").await);
            return Ok(());
        }
    };

    handler.play_source(source);

    check_msg(msg.channel_id.say(&ctx.http, "Now playing").await);

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn queue(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let url = match args.single::<String>() {
        Ok(url) => url,
        Err(_) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, "Must provide a URL to a video or audio")
                    .await,
            );

            return Ok(());
        }
    };

    if !url.starts_with("http") {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Must provide a valid URL")
                .await,
        );

        return Ok(());
    }

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
                    .say(&ctx.http, format!("There was an error skipping the queue: ${:?}", why))
                    .await,
            );
            return Ok(());
        }
    };
    check_msg(msg.channel_id.say(&ctx.http, "Skipped").await);
    Ok(()) 
}

// #[command]
// #[only_in(guilds)]
// async fn playfirst(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
//     let guild = msg.guild(&ctx.cache).unwrap();
//     let guild_id = guild.id;

//     let manager = songbird::get(ctx)
//         .await
//         .expect("Songbird Voice client placed in at initialisation.")
//         .clone();

//     let handler_lock = match manager.get(guild_id) {
//         Some(handler_lock) => handler_lock,
//         None => {
//             check_msg(
//                 msg.channel_id
//                     .say(&ctx.http, "Not in a voice channel")
//                     .await,
//             );
//             return Ok(());
//         }
//     };

//     let handler = handler_lock.lock().await;
//     let queue = handler.queue();
//     queue.modify_queue(|q| q.push_front(value));

// }

fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
