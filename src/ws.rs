use serde::{Deserialize, Serialize};

pub mod recv;
pub mod send;

#[derive(Serialize, Deserialize, Debug)]
#[serde_with::serde_as]
pub enum CommandType {
    #[serde(rename = "connect")]
    Connect,
    #[serde(rename = "disconnect")]
    Disconnect,
    #[serde(rename = "login_qr")]
    LoginWithQRCode,
    #[serde(rename = "is_login")]
    IsLogin,
    #[serde(rename = "get_self")]
    GetSelf,
    #[serde(rename = "get_user_info")]
    GetUserInfo,
    #[serde(rename = "get_group_info")]
    GetGroupInfo,
    #[serde(rename = "get_group_members")]
    GetGroupMembers,
    #[serde(rename = "get_group_member_nickname")]
    GetGroupMemberNickname,
    #[serde(rename = "get_friend_list")]
    GetFriendList,
    #[serde(rename = "get_group_list")]
    GetGroupList,
    #[serde(rename = "send_message")]
    SendMessage,
    #[serde(rename = "response")]
    Response,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "__websocket_closed")]
    WebsocketClosed,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde_with::serde_as]
#[serde(untagged)]
pub enum MatrixMessageDataField {
    Mentions(Vec<String>),
    Blob(MatrixMessageDataBlob),
    Location {
        name: String,
        address: String,
        longitude: f64,
        latitude: f64,
    },
    Media(MatrixMessageDataMedia),
    Link(MatrixMessageDataLink),
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde_with::serde_as]
pub struct MatrixMessageDataBlob {
    pub name: Option<String>,
    #[serde_as(as = "Bytes")]
    pub binary: Vec<u8>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct MatrixMessageDataMedia {
    pub name: String,
    pub url: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde_with::serde_as]
pub struct MatrixMessageDataLink {
    pub title: String,
    pub des: String,
    pub url: String,
}
