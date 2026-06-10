use std::time::Duration;
use std::{env, process::Stdio};

use serenity::all::{CreateAllowedMentions, CreateAttachment, CreateMessage};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PREAMBLE: &str = r#"
#import "@preview/catppuccin:1.0.0": catppuccin, flavors;
#show: catppuccin.with(flavors.mocha);
#set page(height: auto, width: auto, margin: 28pt);
#set text(size: 44pt);
"#;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let Some(content) = msg.content.strip_prefix(",typ") else {
            return;
        };

        let mut content = content.trim().to_owned();

        if content.is_empty() {
            msg.reply_ping(
                &ctx,
                "You must provide code to typeset. Usage: `,typ [code]`",
            )
            .await
            .unwrap();
        }

        if content.starts_with("```") {
            let mut lines = content.lines();
            lines.next().unwrap();
            lines.next_back().unwrap();

            content = lines.collect::<String>();
        }

        let mut child = tokio::process::Command::new("typst")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(["compile", "-", "-", "--format", "png"])
            .spawn()
            .unwrap();

        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(format!("{PREAMBLE}\n{content}").as_bytes())
            .await
            .unwrap();
        drop(stdin);

        let mut buf = vec![];

        let mut stdout = child.stdout.take().unwrap();
        if tokio::time::timeout(Duration::from_secs(25), stdout.read_to_end(&mut buf))
            .await
            .is_err()
        {
            msg.reply_ping(&ctx, "Your code took too long (>25s) to run")
                .await
                .unwrap();
        };

        let mut stderr = child.stderr.take().unwrap();
        stderr.read_to_end(&mut buf).await.unwrap();

        let stat = child.wait().await.unwrap();

        if !stat.success() {
            let err = String::from_utf8_lossy(&buf).into_owned();

            msg.reply_ping(&ctx, format!("```typ\n{err}\n```"))
                .await
                .unwrap();
        } else {
            let attachment = CreateAttachment::bytes(buf, "typst.png");
            let mentions = CreateAllowedMentions::new().replied_user(true);
            let message = CreateMessage::new()
                .files([attachment])
                .reference_message(&msg)
                .allowed_mentions(mentions);

            msg.channel_id.send_message(&ctx, message).await.unwrap();
        };
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
