mod logger;

use chrono::{Duration, TimeZone, Utc};
use futures::StreamExt;
use log::{error, info, Level};
use std::sync::{Arc, RwLock};
use telegram_bot::{
    Api, CanGetChatMemberForChat, ChatId, DeleteMessage, GetMe, Message, MessageChat, MessageKind,
    UpdateKind, User,
};
use tokio::{spawn, time::delay_for};

type ARWLock<T> = Arc<RwLock<T>>;

lazy_static::lazy_static! {
    static ref STICKER_MESSAGES: ARWLock<Vec<Message>> = Arc::default();
    static ref EXITED: Arc<RwLock<bool>> = Arc::default();
}

async fn check_chat_can_delete(
    api: &Api,
    me: &User,
    message: &Message,
) -> Result<bool, telegram_bot::Error> {
    lazy_static::lazy_static! {
        static ref ADMIN_CHAT: ARWLock<Vec<ChatId>> = Arc::default();
        static ref OTHER_CHAT: ARWLock<Vec<ChatId>> = Arc::default();
    }

    let id = message.chat.id();
    let can_delete = if ADMIN_CHAT
        .read()
        .unwrap()
        .iter()
        .find(|chat_id| chat_id == &&id)
        .is_some()
    {
        true
    } else if OTHER_CHAT
        .read()
        .unwrap()
        .iter()
        .find(|chat_id| chat_id == &&id)
        .is_some()
    {
        false
    } else {
        if api
            .send(message.chat.get_member(me))
            .await?
            .can_delete_messages
            .unwrap_or_default()
        {
            ADMIN_CHAT.write().unwrap().push(id);
            true
        } else {
            OTHER_CHAT.write().unwrap().push(id);
            false
        }
    };
    Ok(can_delete)
}

async fn eater(token: String) {
    lazy_static::lazy_static! {
        static ref TWO_MINUTES: Duration = Duration::minutes(2);
    }

    let api = Api::new(token);
    loop {
        let list = STICKER_MESSAGES.read().unwrap().clone();
        let (keep, delete): (Vec<&Message>, Vec<&Message>) = list
            .iter()
            .partition(|m| Utc.timestamp(m.date, 0) + *TWO_MINUTES > Utc::now());
        *STICKER_MESSAGES.write().unwrap() = keep.iter().cloned().cloned().collect::<Vec<_>>();
        for message in delete {
            if let Err(e) = api
                .send(DeleteMessage::new(message.chat.id(), message.id))
                .await
            {
                error!(
                    "Failed to delete message: {}, {}, {}",
                    e,
                    message.chat.id(),
                    message.id,
                );
            }
        }
        if *EXITED.read().unwrap() {
            info!("Eater thread exited.");
            break;
        }
        delay_for(std::time::Duration::from_secs(1)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init_logger(Level::Debug)?;
    let token = std::env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");

    let token_clone = token.clone();
    let eater_handler = spawn(async { eater(token_clone).await });

    let api = Api::new(token);
    let me = api.send(GetMe).await?;

    let mut stream = api.stream();
    while let Some(update) = stream.next().await {
        match update {
            Ok(update) => {
                if let UpdateKind::Message(message) = update.kind {
                    if matches!(message.kind, MessageKind::Sticker { .. })
                        && matches!(
                            message.chat,
                            MessageChat::Group(..) | MessageChat::Supergroup(..)
                        )
                        && check_chat_can_delete(&api, &me, &message).await?
                    {
                        STICKER_MESSAGES.write().unwrap().push(message);
                    }
                }
            }
            Err(e) => {
                error!("Failed to parse message: {}", e);
                *EXITED.write().unwrap() = false;
            }
        }
    }
    eater_handler.await?;
    Ok(())
}
