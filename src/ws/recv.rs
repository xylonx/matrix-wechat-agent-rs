use serde::Deserialize;

use super::{CommandType, MatrixMessageDataField};

#[derive(serde::Deserialize, Debug)]
#[serde_with::serde_as]
pub struct WebsocketMatrixRequest {
    pub mxid: String,
    #[serde(rename(deserialize = "req"))]
    pub req_id: i32,
    pub command: CommandType,
    pub data: Option<MatrixRequestDataField>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(untagged)]
pub enum MatrixRequestDataField {
    Query(MatrixRequestDataQuery),
    Message(MatrixRequestDataMessage),
}

#[derive(serde::Deserialize, Debug)]
pub struct MatrixRequestDataQuery {
    #[serde(rename(deserialize = "wxId"))]
    pub wechat_id: String,
    #[serde(rename(deserialize = "groupId"))]
    pub group_id: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct MatrixRequestDataMessage {
    pub target: String,
    #[serde(rename(deserialize = "type"))]
    pub message_type: MatrixMessageType,
    pub content: String,
    pub data: Option<MatrixMessageDataField>,
}

#[derive(Deserialize, Debug)]
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
