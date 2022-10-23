use std::sync::Arc;

use serenity::{
    client::Context,
    model::prelude::{Guild, GuildId, Message},
    Result as SerenityResult,
};
use songbird::{Call, Songbird};
use tokio::sync::Mutex;

pub struct GuildData {
    pub guild: Guild,
    pub guild_id: GuildId,
    pub manager: Arc<Songbird>,
    pub handler_lock: Arc<Mutex<Call>>,
}

pub async fn get_guild_data(ctx: &Context, msg: &Message) -> Result<GuildData, String> {
    let guild = msg.guild(&ctx.cache).unwrap();
    let guild_id = guild.id;

    let manager = match songbird::get(ctx).await {
        Some(manager) => manager,
        None => return Err("Error getting voice connection".to_string()),
    };

    let handler_lock = match manager.get(guild_id) {
        Some(handler_lock) => handler_lock,
        None => {
            return Err("Not in a voice channel".to_string());
        }
    };

    Ok(GuildData {
        guild,
        guild_id,
        manager,
        handler_lock,
    })
}

pub fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
