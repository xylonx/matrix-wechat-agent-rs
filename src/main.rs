use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use log::{warn, LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller;
use log4rs::append::rolling_file::policy::compound::trigger::size::SizeTrigger;
use log4rs::append::rolling_file::policy::compound::CompoundPolicy;
use log4rs::append::rolling_file::RollingFileAppender;
use log4rs::config::{Appender, Config, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;

use futures_util::{SinkExt, StreamExt};
use matrix_wechat_agent::utils;
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, handshake, http::Request, Message},
};

use futures_util::{future, pin_mut};
use log::{debug, error, info};
use matrix_wechat_agent::{
    constants,
    manager::{self, WechatManager},
    ws::recv::WebsocketMatrixRequest,
};
use tokio::sync::broadcast::{self, Receiver};

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
    #[arg(
        short,
        long,
        help = "image save path default to $CURRENT_DIR/hook_media"
    )]
    save_path: Option<String>,
    #[arg(short, long, default_value = "5")]
    buffer_size: u32,
}

#[tokio::main]
async fn main() {
    utils::kill_by_name("WeChat");
    init_logger();

    let arg = Args::parse();
    let url = url::Url::parse(&arg.addr).unwrap();
    info!("parse url {} successfully", arg.addr);

    info!("construct wss request successfully");

    let (tx, _) = broadcast::channel::<String>(arg.buffer_size.try_into().unwrap());

    let default_save_path = std::env::current_dir()
        .unwrap()
        .join("hook_media")
        .into_os_string()
        .into_string()
        .unwrap();
    let save_path = arg.save_path.unwrap_or(default_save_path);
    let manager: WechatManager = manager::WechatManager::new(arg.port, save_path, tx.clone());
    let inner_manager = manager.clone();

    let ws = tokio::spawn(async move {
        let inner_tx = tx;
        let mut err_cnt = 0;
        let mut last_err = Utc::now();
        let wait = Duration::from_secs(5);
        loop {
            connect_ws(
                url.clone(),
                arg.token.clone(),
                &inner_manager,
                inner_tx.subscribe(),
            )
            .await;
            if Utc::now() - last_err < chrono::Duration::minutes(5) {
                err_cnt += 1;
            } else {
                err_cnt = 1;
            };

            if err_cnt > constants::MAX_WS_RECONNECT_COUNT {
                error!(
                    "err cnt {} exceeds max ws reconnect count {} in last 5 minutes",
                    err_cnt,
                    constants::MAX_WS_RECONNECT_COUNT
                );
                return;
            }

            last_err = Utc::now();
            warn!(
                "websocket connection closed with error. will reconnect after {} seconds",
                (wait * err_cnt).as_secs()
            );
            sleep(wait * err_cnt).await;
        }
    });

    let write_wechat_event = tokio::spawn(async move {
        manager.start_server().await;
    });

    future::select(ws, write_wechat_event).await;
}

async fn connect_ws(
    url: url::Url,
    token: String,
    manager: &WechatManager,
    mut rx: Receiver<String>,
) {
    let request = Request::builder()
        .method("GET")
        .header("Host", url.host_str().unwrap())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", handshake::client::generate_key())
        .header("Authorization", format!("Basic {}", token))
        .uri(url.as_str())
        .body(())
        .unwrap();
    let (ws_stream, _) = connect_async(request).await.expect("Failed to connect");
    info!("WebSocket handshake has been successfully completed");
    let (mut writer, reader) = ws_stream.split();

    let write_message = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            match writer.send(Message::Text(msg)).await {
                Ok(_) => debug!("write message to ws successfully"),
                Err(err) => match err {
                    tungstenite::Error::ConnectionClosed
                    | tungstenite::Error::AlreadyClosed
                    | tungstenite::Error::Io(_) => {
                        error!("write message to ws failed: {}", err);
                        return;
                    }
                    _ => error!("write message to ws failed: {}", err),
                },
            };
        }
    });

    let read_message = {
        reader.for_each_concurrent(32, |msg| async {
            recv_message(msg, manager).await;
        })
    };
    pin_mut!(read_message, write_message);
    future::select(read_message, write_message).await;
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

    debug!("recv ws command: {}", s);

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

fn init_logger() {
    let pattern =
        PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} | {I:5.5} | {({l}):5.5} | {f}:{L} — {m}{n}");
    let rollingfile = RollingFileAppender::builder()
        .encoder(Box::new(pattern.clone()))
        .build(
            Path::new("log").join("matrix_wechat_agent.log"),
            Box::new(CompoundPolicy::new(
                Box::new(SizeTrigger::new(16 * 1024 * 1024)), // max size is 16M for each log file
                Box::new(
                    FixedWindowRoller::builder()
                        .base(1)
                        .build(
                            Path::new("log")
                                .join("matrix_wechat_agent_{}.log")
                                .to_str()
                                .unwrap(),
                            5,
                        )
                        .unwrap(),
                ),
            )),
        )
        .unwrap();
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(pattern))
        .build();
    let config = Config::builder()
        .appender(Appender::builder().build("console", Box::new(stdout)))
        .appender(Appender::builder().build("rollingfile", Box::new(rollingfile)))
        .logger(
            Logger::builder()
                .appender("console")
                .build("stdout", LevelFilter::Info),
        )
        .logger(
            Logger::builder()
                .appender("rollingfile")
                .build("roll_file", LevelFilter::Debug),
        )
        .build(
            Root::builder()
                .appender("console")
                .appender("rollingfile")
                .build(LevelFilter::Debug),
        )
        .unwrap();
    log4rs::init_config(config).unwrap();
}
