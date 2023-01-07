use crate::wechat::{WechatMessage, WechatMessageAppType, WechatMessageType};
use crate::ws::{MatrixMessageDataBlob, MatrixMessageDataField, MatrixMessageDataLink};
use anyhow::bail;
use chrono::Utc;
use futures_util::StreamExt;
use log::{error, info};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LinesCodec};

use crate::ws::send::{EventType, ReplyInfo, WebsocketEvent, WebsocketEventBase};
use crate::{constants, utils};

use std::path::Path;

use super::WechatManager;

impl WechatManager {
    ///
    /// handle events sended by wechat and send them to matrix
    ///
    pub async fn start_server(&self) {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.message_hook_port))
            .await
            .expect(format!("bind to port[{}] failed", self.message_hook_port).as_str());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let local_self = self.clone();
            tokio::spawn(async move {
                if let Err(e) = local_self.process(stream).await {
                    error!("{}", e);
                }
            });
        }
    }

    async fn process(&self, stream: TcpStream) -> anyhow::Result<()> {
        let mut lines = Framed::new(stream, LinesCodec::new());
        let mut err_cnt = 0;
        loop {
            match lines.next().await {
                Some(Ok(line)) => {
                    let msg = serde_json::from_str::<WechatMessage>(line.as_str()).unwrap();
                    if let Err(e) = self.handle_wechat_callback(msg).await {
                        error!("handle wechat callback failed: {}", e);
                        err_cnt += 1;
                    };

                    if err_cnt > constants::MAX_FAIL_COUNT {
                        bail!(
                            "handle wechat callback failed: failure time exceeds {} the max failure time: {}",
                            err_cnt,
                            constants::MAX_FAIL_COUNT
                        )
                    }
                }
                Some(Err(_)) => todo!(),
                // The stream has been exhausted.
                None => break,
            }
        }

        Ok(())
    }

    async fn handle_wechat_callback(&self, msg: WechatMessage) -> anyhow::Result<()> {
        // deduplicate message by msg_id

        let ins = self.get_instance_by_pid(msg.pid).unwrap();
        let mut base = WebsocketEventBase {
            mxid: ins.mxid.clone(),
            id: msg.message_id,
            event_type: EventType::Text,
            timestamp: Utc::now(),
            sender: msg.self_id.clone(),
            target: msg.sender.clone(),
            content: msg.message.clone(),
            replay: None,
        };

        if msg.is_send_message == 0 {
            base.sender = msg.wechat_id.clone();
            if !msg.sender.ends_with("@chatroom") {
                base.target = msg.self_id.clone();
            }
        }
        let mut event = WebsocketEvent::<MatrixMessageDataField> { base, extra: None };

        match msg.msg_type {
            WechatMessageType::Unknown => info!("recv unknown wechat message"),

            WechatMessageType::Text => {
                event.extra = self.get_mentions(msg.extra_info).await?;
            }

            WechatMessageType::Image => match self.fetch_image(msg.self_id, msg.file_path).await {
                Ok(blob) => {
                    event.base.event_type = EventType::Image;
                    event.extra = Some(blob);
                }
                Err(e) => {
                    error!("download image failed: {} msg_id: {}", e, msg.message_id);
                    event.base.content = "[图片下载失败]".to_string();
                }
            },

            WechatMessageType::Voice => match self.fetch_voice(msg.self_id, msg.message).await {
                Ok(blob) => {
                    event.base.event_type = EventType::Audio;
                    event.extra = Some(blob);
                }
                Err(e) => {
                    error!("download voice failed: {} msg_id: {}", e, msg.message_id);
                    event.base.content = "[语音下载失败]".to_string();
                }
            },

            WechatMessageType::Video => match self.fetch_video(msg.file_path, msg.thumb_path).await
            {
                Ok(blob) => {
                    event.base.event_type = EventType::Video;
                    event.extra = Some(blob);
                }
                Err(e) => {
                    error!("download video failed: {} msg_id: {}", e, msg.message_id);
                    event.base.content = "[视频下载失败]".to_string();
                }
            },

            WechatMessageType::Sticker => match self.fetch_sticker(msg.message).await {
                Ok(blob) => {
                    event.base.event_type = EventType::Image;
                    event.extra = Some(blob);
                }
                Err(e) => {
                    error!("download sticker failed: {} msg_id: {}", e, msg.message_id);
                    event.base.content = "[表情下载失败]".to_string();
                }
            },

            WechatMessageType::Location => match self.parse_location(msg.message).await {
                Ok(location) => {
                    event.base.event_type = EventType::Location;
                    event.extra = Some(location);
                }
                Err(e) => {
                    error!("parse location failed: {} msg_id: {}", e, msg.message_id);
                    event.base.content = "[位置解析失败]".to_string();
                }
            },

            WechatMessageType::App => match self.parse_app(msg.message.clone()).await {
                Ok(EnumAppMessage::File) => match self.fetch_file(msg.file_path).await {
                    Ok(blob) => {
                        event.base.event_type = EventType::File;
                        event.extra = Some(blob);
                    }
                    Err(e) => {
                        error!("download file failed: {} msg_id: {}", e, msg.message_id);
                        event.base.content = "[文件下载失败]".to_string();
                    }
                },
                Ok(EnumAppMessage::Sticker) => match self.fetch_sticker(msg.message).await {
                    Ok(blob) => {
                        event.base.event_type = EventType::Image;
                        event.extra = Some(blob);
                    }
                    Err(e) => {
                        error!("download sticker failed: {} msg_id: {}", e, msg.message_id);
                        event.base.content = "[表情下载失败]".to_string();
                    }
                },
                Ok(EnumAppMessage::Reply(r)) => {
                    event.base.content = r.content;
                    let sender = r.chat_sender.or_else(|| r.user_sender);
                    if sender.is_none() {
                        bail!("cannot find sender. msg_id: {}", msg.message_id)
                    }
                    event.base.replay = Some(ReplyInfo {
                        id: r.refer_msg_id,
                        sender: sender.unwrap(),
                    })
                }
                Ok(EnumAppMessage::Announcement(a)) => {
                    event.base.event_type = EventType::Notice;
                    event.base.content = a;
                }
                Ok(EnumAppMessage::Link(l)) => {
                    event.base.event_type = EventType::App;
                    event.extra = Some(MatrixMessageDataField::Link(l));
                }
                Ok(EnumAppMessage::Article(a)) => {
                    event.base.content = format!(
                        "#{}\nauthor: {}\n{}\n\n{}",
                        a.category.item.title,
                        a.category.name,
                        a.category.item.summary,
                        a.category.item.digest
                    );
                }
                _ => {
                    error!("parse app failed. msg_id: {}", msg.message_id);
                    event.base.content = "[应用解析失败]".to_string();
                }
            },

            WechatMessageType::PrivateVoIP => match self.parse_private_voip(msg.message).await {
                Ok(status) => {
                    event.base.event_type = EventType::VoIP;
                    event.base.content = status;
                }
                Err(e) => {
                    error!("parse voip failed: {} msg_id: {}", e, msg.message_id);
                    event.base.content = "[VoIP状态解析失败]".to_string();
                }
            },

            WechatMessageType::LastMessage => info!("recv last wechat message"),

            WechatMessageType::Revoke => match self.parse_revoke(msg.message).await {
                Ok(status) => {
                    event.base.event_type = EventType::Revoke;
                    event.base.content = status;
                }
                Err(e) => {
                    error!("parse revoke failed: {} msg_id: {}", e, msg.message_id);
                    event.base.content = "[撤回消息解析失败]".to_string();
                }
            },

            WechatMessageType::System => match msg.sender == "weixin" || msg.is_send_message == 1 {
                true => info!("skip wechat system message msg_id: {}", msg.message_id),
                false => match self.parse_system_message(msg.message).await {
                    Ok(status) => {
                        event.base.event_type = EventType::System;
                        event.base.content = status;

                        if (event.base.content == "You recalled a message"
                            || event.base.content == "你撤回了一条消息")
                            && !msg.sender.ends_with("@chatroom")
                        {
                            event.base.target = msg.wechat_id;
                        }
                    }
                    Err(e) => {
                        error!("parse system failed: {} msg_id: {}", e, msg.message_id);
                        event.base.content = "[系统消息解析失败]".to_string();
                    }
                },
            },
        }

        self.write_event_resp(event).await
    }
}

impl WechatManager {
    async fn get_mentions(&self, extra: String) -> anyhow::Result<Option<MatrixMessageDataField>> {
        #[derive(serde::Deserialize)]
        struct Mentions {
            #[serde(rename = "atuserlist")]
            at_user_list: String,
        }

        if extra.len() == 0 {
            bail!("no data in extra info")
        }

        let mentions: Mentions = quick_xml::de::from_reader(extra.as_bytes())?;
        let mentions = mentions.at_user_list.trim();
        if mentions.len() == 0 {
            return Ok(None);
        }

        Ok(Some(MatrixMessageDataField::Mentions(
            mentions
                .split(",")
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
        )))
    }

    async fn fetch_image(
        &self,
        self_id: String,
        file_path: String,
    ) -> anyhow::Result<MatrixMessageDataField> {
        let path = Path::new(&file_path);
        let filename = utils::get_filename(path)?;

        let file_ext = match path.extension() {
            Some(fe) => match fe.to_str() {
                Some(f) => f.to_string(),
                None => {
                    bail!("file_path[{}] contains invalid UTF8 char", file_path)
                }
            },
            None => "".to_string(),
        };

        let base_image = Path::new(&self.save_path)
            .join(self_id)
            .join(filename.clone())
            .join(file_ext)
            .display()
            .to_string();
        let png_image = base_image.clone() + ".png";
        let gif_image = base_image.clone() + ".gif";
        let jpg_image = base_image.clone() + ".jpg";

        let mut file: File;
        if let Ok(f) = File::open(base_image).await {
            file = f;
        } else if let Ok(f) = File::open(png_image).await {
            file = f;
        } else if let Ok(f) = File::open(gif_image).await {
            file = f;
        } else if let Ok(f) = File::open(jpg_image).await {
            file = f;
        } else {
            bail!("image file {} not found", file_path)
        }

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;
        // image_path.exists();

        Ok(MatrixMessageDataField::Blob(MatrixMessageDataBlob {
            name: filename,
            binary: buffer,
        }))
    }

    async fn fetch_voice(
        &self,
        self_id: String,
        msg: String,
    ) -> anyhow::Result<MatrixMessageDataField> {
        #[derive(serde::Deserialize)]
        struct Message {
            #[serde(rename = "voicemsg")]
            message: VoiceMessage,
        }
        #[derive(serde::Deserialize)]
        struct VoiceMessage {
            #[serde(rename = "@clientmsgid")]
            client_message_id: String,
        }

        if msg.len() == 0 {
            bail!("no data in extra info")
        }

        let msg: Message = quick_xml::de::from_reader(msg.as_bytes())?;

        let voice_path = String::from(msg.message.client_message_id);
        let path = Path::new(&self.save_path)
            .join(self_id)
            .join(voice_path + ".amr");
        let filename = utils::get_filename(path.as_path())?;
        if !path.exists() {
            bail!("voice file {} not found", path.display())
        }

        let mut file = File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        Ok(MatrixMessageDataField::Blob(MatrixMessageDataBlob {
            name: filename,
            binary: buffer,
        }))
    }

    async fn fetch_video(
        &self,
        file_path: String,
        thumbnail: String,
    ) -> anyhow::Result<MatrixMessageDataField> {
        let path = match file_path.len() {
            0 => Path::new(&self.save_path)
                .join(thumbnail)
                .with_extension("mp4"),
            _ => Path::new(&self.save_path).join(file_path),
        };
        let filename = utils::get_filename(path.as_path())?;

        if !path.exists() {
            bail!("video file {} not found", path.display())
        }

        let mut file = File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        Ok(MatrixMessageDataField::Blob(MatrixMessageDataBlob {
            name: filename,
            binary: buffer,
        }))
    }

    async fn fetch_file(&self, file_path: String) -> anyhow::Result<MatrixMessageDataField> {
        let path = Path::new(&self.save_path).join(file_path);
        let filename = utils::get_filename(path.as_path())?;

        if !path.exists() {
            bail!("file {} not found", path.display())
        }

        let mut file = File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        Ok(MatrixMessageDataField::Blob(MatrixMessageDataBlob {
            name: filename,
            binary: buffer,
        }))
    }

    async fn fetch_sticker(&self, msg: String) -> anyhow::Result<MatrixMessageDataField> {
        #[derive(serde::Deserialize)]
        struct Message {
            #[serde(rename = "emoji")]
            message: EmojiMessage,
        }
        #[derive(serde::Deserialize)]
        struct EmojiMessage {
            #[serde(rename = "@cdnurl")]
            cnd_url: String,
            #[serde(rename = "@aeskey")]
            key: String,
        }

        if msg.len() == 0 {
            bail!("no data in extra info")
        }

        let msg: Message = quick_xml::de::from_reader(msg.as_bytes())?;

        Ok(MatrixMessageDataField::Blob(MatrixMessageDataBlob {
            name: msg.message.key,
            binary: utils::get_file_maybe_gzip_decompress(msg.message.cnd_url).await?,
        }))
    }

    async fn parse_location(&self, msg: String) -> anyhow::Result<MatrixMessageDataField> {
        #[derive(serde::Deserialize)]
        struct Message {
            #[serde(rename = "location")]
            message: LocationMessage,
        }
        #[derive(serde::Deserialize)]
        struct LocationMessage {
            #[serde(rename = "@x")]
            x: String,
            #[serde(rename = "@y")]
            y: String,
            #[serde(rename = "@poiname")]
            position_name: String,
            #[serde(rename = "@label")]
            label: String,
        }

        if msg.len() == 0 {
            bail!("no data in extra info")
        }
        let msg: Message = quick_xml::de::from_reader(msg.as_bytes())?;

        Ok(MatrixMessageDataField::Location {
            name: msg.message.position_name,
            address: msg.message.label,
            longitude: msg.message.y.parse::<f64>()?,
            latitude: msg.message.x.parse::<f64>()?,
        })
    }

    async fn parse_app(&self, msg: String) -> anyhow::Result<EnumAppMessage> {
        if msg.len() == 0 {
            bail!("no data in extra info")
        }
        let msg: AppMessage = quick_xml::de::from_reader(msg.as_bytes())?;
        match msg.message.message_type {
            WechatMessageAppType::Article if msg.message.article.is_some() => {
                Ok(EnumAppMessage::Article(msg.message.article.unwrap()))
            }
            WechatMessageAppType::File => Ok(EnumAppMessage::File),
            WechatMessageAppType::Sticker => Ok(EnumAppMessage::Sticker),
            WechatMessageAppType::Reply if msg.message.reply.is_some() => {
                let mut reply = msg.message.reply.unwrap();
                reply.content = msg.message.title;
                Ok(EnumAppMessage::Reply(reply))
            }
            WechatMessageAppType::Notice if msg.message.announcement.is_some() => Ok(
                EnumAppMessage::Announcement(msg.message.announcement.unwrap()),
            ),
            _ => Ok(EnumAppMessage::Link(MatrixMessageDataLink {
                title: msg.message.title,
                desc: msg.message.des,
                url: msg.message.url,
            })),
        }
    }

    // TODO(xylonx): parse message and extract voip detail status from it
    async fn parse_private_voip(&self, _: String) -> anyhow::Result<String> {
        Ok("private voip".to_string())
    }

    async fn parse_revoke(&self, msg: String) -> anyhow::Result<String> {
        if msg.len() == 0 {
            bail!("no data in extra info")
        }
        Ok(quick_xml::de::from_reader(msg.as_bytes())?)
    }

    async fn parse_system_message(&self, msg: String) -> anyhow::Result<String> {
        #[derive(serde::Deserialize)]
        struct Message {
            #[serde(rename = "voipmt")]
            message: VoIPMessage,
        }
        #[derive(serde::Deserialize)]
        struct VoIPMessage {
            #[serde(rename = "invite")]
            invite: Option<String>,
            #[serde(rename = "banner")]
            banner: Option<String>,
        }

        if msg.len() == 0 {
            bail!("no data in extra info")
        }
        let msg: Message = quick_xml::de::from_reader(msg.as_bytes())?;

        Ok(msg
            .message
            .invite
            .or(msg.message.banner)
            .map(|status| format!("VoIP: {}", status))
            .unwrap_or("".to_string()))
    }
}

// FIXME(xylonx): move below wechat message type definition to another module
#[derive(serde::Deserialize)]
#[serde(rename = "msg")]
struct AppMessage {
    #[serde(rename = "appmsg")]
    message: AppMessageContent,
}

enum EnumAppMessage {
    File,
    Sticker,
    Article(AppArticle),
    Announcement(String),
    Reply(AppReply),
    Link(MatrixMessageDataLink),
}

#[derive(serde::Deserialize)]
#[serde_with::serde_as]
struct AppMessageContent {
    #[serde(rename = "type")]
    message_type: WechatMessageAppType,

    title: String,
    url: String,
    des: String,

    #[serde(rename = "mmreader")]
    article: Option<AppArticle>,

    #[serde(rename = "textannouncement")]
    announcement: Option<String>,

    #[serde(rename = "refermsg")]
    reply: Option<AppReply>,
}

#[derive(serde::Deserialize)]
struct AppArticle {
    category: AppArticleCategory,
}
#[derive(serde::Deserialize)]
struct AppArticleCategory {
    name: String,
    item: AppArticleCategoryItem,
}
#[derive(serde::Deserialize)]
struct AppArticleCategoryItem {
    title: String,
    digest: String,
    summary: String,
}

#[derive(serde::Deserialize)]
#[serde_with::serde_as]
struct AppReply {
    #[serde(skip_deserializing)]
    content: String,
    #[serde(rename = "svrid")]
    refer_msg_id: u64,
    #[serde(rename = "chatusr")]
    chat_sender: Option<String>,
    #[serde(rename = "fromusr")]
    user_sender: Option<String>,
}
