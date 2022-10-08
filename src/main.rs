use std::env;

use serenity::client::Context;

use serenity::{
    async_trait,
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
use songbird::SerenityInit;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[group]
#[commands(play)]
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

fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
