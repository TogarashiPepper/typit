use std::time::Duration;
use std::{env, process::Stdio};

use serenity::all::{
    ActionRowComponent, CreateActionRow, CreateAllowedMentions, CreateAttachment, CreateInputText,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, CreateModal,
    EditAttachments, EditInteractionResponse, InputTextStyle, Interaction,
};
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

async fn run_typst(content: &str) -> (String, Vec<CreateAttachment>) {
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
        return ("Your code took too long (>25s) to run".to_owned(), vec![]);
    };

    let mut stderr = child.stderr.take().unwrap();
    stderr.read_to_end(&mut buf).await.unwrap();

    let stat = child.wait().await.unwrap();

    if !stat.success() {
        let err = String::from_utf8_lossy(&buf).into_owned();

        (format!("```typ\n{err}\n```"), vec![])
    } else {
        let attachment = CreateAttachment::bytes(buf, "typst.png");

        (String::new(), vec![attachment])
    }
}

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

        let resp = run_typst(&content).await;
        let resp = CreateMessage::new()
            .content(resp.0)
            .reference_message(&msg)
            .files(resp.1);

        msg.channel_id.send_message(&ctx, resp).await.unwrap();
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let mentions = CreateAllowedMentions::new().replied_user(true);

        match interaction {
            Interaction::Command(cmd) => {
                if cmd.data.name != "typ" {
                    return;
                }

                if cmd.data.options.is_empty() {
                    let txt_inp =
                        CreateInputText::new(InputTextStyle::Paragraph, "code", "typst_doc_body")
                            .placeholder("$ 1 + 2 = 3 $")
                            .required(true);
                    let action_row = CreateActionRow::InputText(txt_inp);
                    let modal = CreateModal::new("typst_modal_id", "Input your code")
                        .components(vec![action_row]);

                    let resp = CreateInteractionResponse::Modal(modal);
                    cmd.create_response(&ctx, resp).await.unwrap();
                } else {
                    let code = cmd.data.options[0].value.as_str().unwrap();
                    cmd.create_response(
                        &ctx,
                        CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()),
                    )
                    .await
                    .unwrap();

                    let msg = run_typst(code).await;

                    let mut attachments = EditAttachments::new();

                    for atch in msg.1 {
                        attachments = attachments.add(atch);
                    }

                    let msg = EditInteractionResponse::new()
                        .content(msg.0)
                        .attachments(attachments)
                        .allowed_mentions(mentions);

                    cmd.edit_response(&ctx, msg).await.unwrap();
                }
            }
            Interaction::Modal(modal_int) => {
                let ActionRowComponent::InputText(in_text) =
                    modal_int.data.components[0].components[0].clone()
                else {
                    unreachable!();
                };

                let code = in_text.value.unwrap();
                modal_int
                    .create_response(
                        &ctx,
                        CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new()),
                    )
                    .await
                    .unwrap();
                let msg = run_typst(&code).await;

                let mut attachments = EditAttachments::new();

                for atch in msg.1 {
                    attachments = attachments.add(atch);
                }

                let msg = EditInteractionResponse::new()
                    .content(msg.0)
                    .attachments(attachments)
                    .allowed_mentions(mentions);

                modal_int.edit_response(&ctx, msg).await.unwrap();
            }
            _ => todo!(),
        }
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
