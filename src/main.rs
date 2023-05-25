#tls conflet need reslove
use discord::model::{Event, Message};
use discord::Discord;
use std::env;
use std::str::FromStr;
use rand::Rng;
use native_tls::{TlsConnector, TlsStream};
use std::net::TcpStream;
use std::io::{BufReader, BufRead, Write};
use std::time::Duration;
use log::{info, error};
use dotenv::dotenv;
use tokio::time::timeout;
use tokio::runtime::Runtime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const ALLOWED_DOMAINS: [&str; 1] = ["pausd.us"]; // replace with the domains you want to allow

fn generate_verification_code() -> String {
    let mut rng = rand::thread_rng();
    rng.gen_range(100000..=999999).to_string()
}

async fn send_verification_email(email: &str, verification_code: &str) -> Result<(), String> {
    if !email.contains('@') || !email.ends_with(ALLOWED_DOMAINS[0]) {
        return Err(format!(
            "Error: {} is not a valid email address for verification. Please use @pausd.us instead, or contact an admin.",
            email
        ));
    }

    let smtp_server = "smtp.gmail.com";
    let smtp_port = 587;

    let tls_connector = TlsConnector::new().map_err(|e| e.to_string())?;
    let tcp_stream = TcpStream::connect(format!("{}:{}", smtp_server, smtp_port))
        .map_err(|e| e.to_string())?;
    tcp_stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp_stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    let mut tls_stream = tls_connector
        .connect("smtp.gmail.com", tcp_stream)
        .await
        .map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(&mut tls_stream);
    let mut writer = &mut tls_stream;

    let mut buffer = Vec::new();
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"220") {
        return Err("SMTP server not ready".to_string());
    }
    buffer.clear();

    writer.write_all(b"EHLO example.com\r\n").await.map_err(|e| e.to_string())?;
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"250") {
        return Err("EHLO command failed".to_string());
    }
    buffer.clear();

    writer
        .write_all(format!("AUTH LOGIN {}\r\n", base64::encode(SMTP_USERNAME)).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"334") {
        return Err("AUTH LOGIN failed".to_string());
    }
    buffer.clear();

    writer
        .write_all(format!("{}\r\n", base64::encode(SMTP_PASSWORD)).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"235") {
        return Err("Authentication failed".to_string());
    }
    buffer.clear();

    writer
        .write_all(format!("MAIL FROM: <{}>\r\n", SMTP_USERNAME).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"250") {
        return Err("MAIL FROM command failed".to_string());
    }
    buffer.clear();

    writer
        .write_all(format!("RCPT TO: <{}>\r\n", email).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"250") {
        return Err(format!("RCPT TO command failed for email: {}", email));
    }
    buffer.clear();

    writer.write_all(b"DATA\r\n").await.map_err(|e| e.to_string())?;
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"354") {
        return Err("DATA command failed".to_string());
    }
    buffer.clear();

    let message = format!("Subject: Verification Code\r\n\r\nYour verification code is {}\r\n.\r\n", verification_code);
    writer.write_all(message.as_bytes()).await.map_err(|e| e.to_string())?;

    writer.write_all(b"QUIT\r\n").await.map_err(|e| e.to_string())?;
    reader.read_until(b'\n', &mut buffer).await.map_err(|e| e.to_string())?;
    if !buffer.starts_with(b"221") {
        return Err("QUIT command failed".to_string());
    }

    Ok(())
}

async fn send_verification_code(ctx: &discord::Context) {
    let channel = ctx
        .http
        .get_channel(ctx.message.channel_id.0)
        .await
        .unwrap()
        .guild()
        .unwrap();
    channel
        .send_message(&ctx.http, |m| {
            m.content("Please enter your email address to receive a verification code.")
        })
        .await
        .unwrap();

    let email = loop {
        let message = match timeout(Duration::from_secs(30), ctx.wait_for::<Message>(&ctx).fuse()).await {
            Ok(Ok(message)) => message,
            _ => {
                channel
                    .send_message(&ctx.http, |m| {
                        m.content("Verification timed out. Please try again.")
                    })
                    .await
                    .unwrap();
                return;
            }
        };

        let content = message.content.clone();
        if content.contains('@') && content.ends_with(ALLOWED_DOMAINS[0]) {
            break content;
        } else {
            channel
                .send_message(&ctx.http, |m| {
                    m.content(format!(
                        "Error: {} is not a valid email address for verification. Please use @pausd.us instead, or contact an admin.",
                        content
                    ))
                })
                .await
                .unwrap();
        }
    };

    let verification_code = generate_verification_code();
    if let Err(err) = send_verification_email(&email, &verification_code).await {
        channel
            .send_message(&ctx.http, |m| m.content(err))
            .await
            .unwrap();
        return;
    }

    channel
        .send_message(&ctx.http, |m| {
            m.content(format!("Please enter the verification code sent to {}.", email))
        })
        .await
        .unwrap();

    loop {
        let message = match timeout(Duration::from_secs(30), ctx.wait_for::<Message>(&ctx).fuse()).await {
            Ok(Ok(message)) => message,
            _ => {
                channel
                    .send_message(&ctx.http, |m| {
                        m.content("Verification timed out. Please try again.")
                    })
                    .await
                    .unwrap();
                return;
            }
        };

        if message.content == verification_code {
            let guild = ctx.message.guild(&ctx.cache).await.unwrap();
            let member = guild.member(&ctx.cache, ctx.message.author.id).await.unwrap();
            let verified_role = guild.role_by_name("Verified").unwrap();
            let member_role = guild.role_by_name("Member").unwrap();
            member.add_role(&ctx.http, verified_role).await.unwrap();
            member.add_role(&ctx.http, member_role).await.unwrap();

            channel.send_message(&ctx.http, |m| m.content("You have been verified!")).await.unwrap();
            break;
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init();

    let discord_token = env::var("BOT_TOKEN").expect("Expected BOT_TOKEN in the environment");
    let smtp_username = env::var("SMTP_USERNAME").expect("Expected SMTP_USERNAME in the environment");
    let smtp_password = env::var("SMTP_PASSWORD").expect("Expected SMTP_PASSWORD in the environment");

    let discord = Discord::from_bot_token(&discord_token).expect("Failed to create Discord client");
    let mut runtime = Runtime::new().unwrap();

    runtime.block_on(async {
        let (mut connection, _) = discord.connect().await.expect("Failed to connect to Discord");

        while let Ok(Some(event)) = connection.recv_event().await {
            match event {
                Event::MessageCreate(message) => {
                    if let Some(content) = message.content.as_ref() {
                        if content.starts_with("~verify") {
                            let ctx = discord.get_context(&message).await.unwrap();
                            send_verification_code(&ctx).await;
                        }
                    }
                }
                Event::Ready(_) => {
                    info!("Bot is ready.");
                }
                Event::ServerMemberAdd(member) => {
                    let user = member.user;
                    if let Some(private_channel) = discord.create_private_channel(user.id) {
                        if let Err(err) = private_channel.send_message(&discord.http, |m| {
                            m.content(format!(
                                "Welcome to the server, {}! Please verify yourself by using the `~verify` command.",
                                user.mention()
                            ))
                        }).await {
                            error!("Failed to send message to {}: {}", user.name, err);
                        }
                    }
                }
                _ => {}
            }
        }
    });
}
