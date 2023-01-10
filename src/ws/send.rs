use serde::Serialize;
use serde_with::formats::Flexible;
use serde_with::TimestampMilliSeconds;

use ::chrono::{DateTime, Utc};

use super::CommandType;

#[derive(serde::Serialize)]
#[serde_with::serde_as]
#[serde(untagged)]
pub enum WebsocketMessage<T: Serialize> {
    Command(WebsocketCommand<T>),
    Event(WebsocketEvent<T>),
}

impl<T: Serialize> WebsocketMessage<T> {
    pub fn mxid(&self) -> String {
        match self {
            WebsocketMessage::Command(cmd) => cmd.mxid.clone(),
            WebsocketMessage::Event(e) => e.base.mxid.clone(),
        }
    }
}

#[serde_with::serde_as]
#[derive(serde::Serialize, Debug)]
pub struct WebsocketCommand<T: Serialize> {
    pub mxid: String,
    #[serde(rename = "req")]
    pub req_id: i32,
    pub command: CommandType,
    pub data: T,
}

#[serde_with::serde_as]
#[derive(serde::Serialize)]
pub struct WebsocketEvent<T: Serialize> {
    #[serde(flatten)]
    pub base: WebsocketEventBase,
    pub extra: Option<T>,
}

#[serde_with::serde_as]
#[derive(serde::Serialize)]
pub struct WebsocketEventBase {
    pub mxid: String,
    pub id: u64,
    #[serde(rename = "type")]
    pub event_type: EventType,
    #[serde_as(as = "TimestampMilliSeconds<i64, Flexible>")]
    #[serde(rename = "ts")]
    pub timestamp: DateTime<Utc>,
    pub sender: String,
    pub target: String,
    pub content: String,
    pub reply: Option<ReplyInfo>,
}

#[derive(serde::Serialize)]
pub struct ReplyInfo {
    pub id: u64,
    pub sender: String,
}

#[derive(Serialize)]
#[serde_with::serde_as]
pub enum EventType {
    #[serde(rename = "m.text")]
    Text,
    #[serde(rename = "m.image")]
    Image,
    #[serde(rename = "m.audio")]
    Audio,
    #[serde(rename = "m.video")]
    Video,
    #[serde(rename = "m.file")]
    File,
    #[serde(rename = "m.location")]
    Location,
    #[serde(rename = "m.notice")]
    Notice,
    #[serde(rename = "m.app")]
    App,
    #[serde(rename = "m.revoke")]
    Revoke,
    #[serde(rename = "m.voip")]
    VoIP,
    #[serde(rename = "m.system")]
    System,
}
