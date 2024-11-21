use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;

pub struct Discord {
    pub name: String,
}

#[async_trait]
impl EventHandler for Discord {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots to prevent potential loops
        if msg.author.bot {
            return;
        }

        // Example command handling
        if msg.content == "!hello" {
            if let Err(why) = msg.channel_id.say(&ctx.http, "Hello there!").await {
                println!("Failed to send message: {:?}", why);
            }
        }

        // Echo command
        if msg.content.starts_with("!echo ") {
            let content = msg.content.replacen("!echo ", "", 1);
            if let Err(why) = msg.channel_id.say(&ctx.http, content).await {
                println!("Failed to send message: {:?}", why);
            }
        }

        // Add new command
        if msg.content == "!status" {
            if let Err(why) = msg
                .channel_id
                .say(&ctx.http, "I'm running and ready to help!")
                .await
            {
                println!("Failed to send message: {:?}", why);
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", self.name);
        println!("Serving {} guilds", ready.guilds.len());
    }
}

impl Discord {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}
