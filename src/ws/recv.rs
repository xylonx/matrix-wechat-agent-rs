use serde::Deserialize;

use super::{CommandType, MatrixMessageDataField};

#[derive(serde::Deserialize)]
#[serde_with::serde_as]
pub struct WebsocketMatrixRequest {
    pub mxid: String,
    #[serde(rename(deserialize = "req"))]
    pub req_id: i32,
    pub command: CommandType,
    pub data: MatrixRequestDataField,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum MatrixRequestDataField {
    Query(MatrixRequestDataQuery),
    Message(MatrixRequestDataMessage),
}

#[derive(serde::Deserialize)]
pub struct MatrixRequestDataQuery {
    #[serde(rename(deserialize = "wxId"))]
    pub wechat_id: String,
    #[serde(rename(deserialize = "groupId"))]
    pub group_id: String,
}

#[derive(serde::Deserialize)]
pub struct MatrixRequestDataMessage {
    pub target: String,
    #[serde(rename(deserialize = "type"))]
    pub message_type: MatrixMessageType,
    pub content: String,
    pub data: MatrixMessageDataField,
}

#[derive(Deserialize)]
#[serde_with::serde_as]
pub enum MatrixMessageType {
    #[serde(rename = "m.text")]
    Text,
    #[serde(rename = "m.emote")]
    Emote,
    #[serde(rename = "m.notice")]
    Notice,
    #[serde(rename = "m.image")]
    Image,
    #[serde(rename = "m.location")]
    Location,
    #[serde(rename = "m.video")]
    Video,
    #[serde(rename = "m.audio")]
    Audio,
    #[serde(rename = "m.file")]
    File,
}
