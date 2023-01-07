use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{handshake, http::Request, Message},
};

use futures_util::{future, pin_mut};
use log::{error, info, trace};
use matrix_wechat_agent::{
    manager::{self, WechatManager},
    ws::recv::WebsocketMatrixRequest,
};
use tokio::sync::mpsc;

use clap::Parser;
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    token: String,
    #[arg(short, long)]
    addr: String,
    #[arg(short, long, default_value = "23333")]
    port: u32,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let arg = Args::parse();

    let url = url::Url::parse(&arg.addr).unwrap();
    info!("parse url {} successfully", arg.addr);

    let request = Request::builder()
        .method("GET")
        .header("Host", url.host_str().unwrap())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", handshake::client::generate_key())
        .header("Authorization", format!("Basic {}", arg.token))
        .uri(url.as_str())
        .body(())
        .unwrap();
    info!("construct wss request successfully");

    let (tx, mut rx) = mpsc::channel::<String>(1);

    let manager: WechatManager = manager::WechatManager::new(arg.port, "".to_string(), tx);

    let (ws_stream, _) = connect_async(request).await.expect("Failed to connect");
    info!("WebSocket handshake has been successfully completed");

    let (mut writer, reader) = ws_stream.split();

    let write_message = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            trace!("write matrix command resp to ws: {}", msg);
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

    trace!("recv ws command: {}", s);

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
