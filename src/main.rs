use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use futures_util::{future, pin_mut};
use log::{error, info};
use matrix_wechat_agent::{
    manager::{self, WechatManager},
    ws::recv::WebsocketMatrixRequest,
};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    env_logger::init();

    let connect_addr = String::from("ws://localhost:50000/ws");
    let url = url::Url::parse(&connect_addr).unwrap();
    info!("parse connect_addr[{}] successfully", connect_addr);

    let (tx, mut rx) = mpsc::channel::<String>(10);

    let manager: WechatManager = manager::WechatManager::new(12, "".to_string(), tx);

    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    info!("WebSocket handshake has been successfully completed");

    let (mut writer, reader) = ws_stream.split();

    let write_message = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(err) = writer.send(Message::Text(msg)).await {
                error!("write message to ws failed: {}", err);
            };
        }
    });

    let inner_manager = manager.clone();
    let write_wechat_event = tokio::spawn(async move {
        inner_manager.start_server().await;
    });

    let read_message = {
        reader.for_each(|msg| async {
            recv_message(msg, &manager).await;
        })
    };

    pin_mut!(read_message, write_message, write_wechat_event);
    future::select(
        future::select(read_message, write_message),
        write_wechat_event,
    )
    .await;
}

async fn recv_message(
    message: Result<Message, tokio_tungstenite::tungstenite::Error>,
    manager: &WechatManager,
) {
    let message = match message {
        Ok(msg) => msg,
        Err(err) => {
            error!("recv remote message failed: {}", err);
            return;
        }
    };

    let s = match message.into_text() {
        Ok(s) => {
            info!("convert recv message to text successfully");
            s
        }
        Err(err) => {
            error!("convert recv message to text failed: {}", err);
            return;
        }
    };
    let msg = match serde_json::from_str::<WebsocketMatrixRequest>(s.as_str()) {
        Ok(msg) => {
            info!("parse recv message as json successfully");
            msg
        }
        Err(err) => {
            error!("parse recv message as json failed: {}", err);
            return;
        }
    };

    if let Err(e) = manager.handle_matrix_events(msg).await {
        error!("handle matrix events failed: {}", e);
    };
}
