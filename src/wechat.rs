use anyhow::bail;
use bytes::Bytes;
use serde_repr::Deserialize_repr;

use chrono::{serde::ts_seconds, DateTime, Utc};
use std::{collections::HashMap, os::raw::c_int, path::Path, time::Duration, vec};
use sysinfo::{Pid, PidExt, ProcessExt, ProcessStatus, System, SystemExt};

use log::{error, info};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{fs::File, io::AsyncWriteExt, time::sleep};

use crate::{
    constants::{self, wechat_api},
    utils,
    ws::{
        recv::{MatrixMessageType, MatrixRequestDataMessage},
        MatrixMessageDataField, MatrixMessageDataMedia,
    },
};

#[derive(Debug)]
pub struct WechatInstance {
    pub port: u32,
    pub message_hook_port: u32,
    pub save_path: String,
    pub pid: u32,
    pub client: reqwest::Client,
    pub mxid: String,
}

impl Clone for WechatInstance {
    fn clone(&self) -> Self {
        Self {
            port: self.port,
            message_hook_port: self.message_hook_port,
            save_path: self.save_path.clone(),
            pid: self.pid,
            client: self.client.clone(),
            mxid: self.mxid.clone(),
        }
    }
}

#[derive(Serialize)]
struct WechatNilBodyReq {}

#[derive(Serialize)]
struct WechatJsonSQLReq {
    db_handle: i64,
    sql: String,
}

#[derive(Deserialize)]
struct WechatErrorResp {
    pub msg: String,
    pub result: String,
}

#[derive(Deserialize)]
struct WechatHookResp {
    pub _msg: u32,
    pub _result: String,
}

#[derive(Serialize)]
struct SendTextMessageReq {
    wxid: String,
    msg: String,
}

// load injection lib
impl WechatInstance {
    pub fn new(
        port: u32,
        save_path: String,
        msg_hook_port: u32,
        mxid: String,
    ) -> anyhow::Result<WechatInstance> {
        Ok(WechatInstance {
            pid: WechatInstance::new_wechat_instance(port)?,
            port,
            message_hook_port: msg_hook_port,
            client: reqwest::Client::new(),
            mxid,
            save_path,
        })
    }

    /**
     * inject dll into wechat.exe and return pid
     */
    fn new_wechat_instance(port: u32) -> anyhow::Result<u32> {
        unsafe {
            let driver_lib_path = String::from("wxDriver64.dll");
            let lib = libloading::Library::new(driver_lib_path)?;

            let new_wechat: libloading::Symbol<unsafe extern "C" fn() -> u32> =
                lib.get(b"new_wechat")?;
            let start_listen: libloading::Symbol<
                unsafe extern "C" fn(pid: u32, port: c_int) -> c_int,
            > = lib.get(b"start_listen")?;

            let pid = new_wechat();
            let port: c_int = port.try_into()?;
            let ok = start_listen(pid, port);
            if ok == 0 {
                bail!("start listen failed with return value: {}", ok)
            }

            info!(
                "new wechat instance and start listen it at {} successfully. pid = {}",
                port, pid
            );

            Ok(pid)
        }
    }

    #[allow(dead_code)]
    fn stop_listening(&self) -> anyhow::Result<bool> {
        unsafe {
            let driver_lib_path = String::from("wxDriver64.dll");
            let lib = libloading::Library::new(driver_lib_path)?;

            // TODO(xylonx): determine stop_listen function signature
            let stop_listen: libloading::Symbol<unsafe extern "C" fn() -> c_int> =
                lib.get(b"stop_listen")?;
            Ok(stop_listen() == 1)
        }
    }

    async fn wechat_hook_post_raw<TReq: Serialize>(
        &self,
        msg_type: u32,
        body: TReq,
    ) -> Result<Bytes, reqwest::Error> {
        self.client
            .post(wechat_api(self.port, msg_type))
            .json(&body)
            .send()
            .await?
            .bytes()
            .await
    }

    async fn wechat_hook_post<TReq: Serialize, TResp: DeserializeOwned>(
        &self,
        msg_type: u32,
        body: TReq,
    ) -> Result<TResp, reqwest::Error> {
        self.client
            .post(wechat_api(self.port, msg_type))
            .json(&body)
            .send()
            .await?
            .json()
            .await
    }
}

impl WechatInstance {
    pub async fn hook_wechat_message(&self, save_path: String) -> anyhow::Result<()> {
        self.wechat_hook_post::<serde_json::Value, HashMap<String, serde_json::Value>>(
            constants::WECHAT_MSG_START_HOOK,
            serde_json::json!({"port": self.message_hook_port}),
        )
        .await?;
        info!(
            "hook instance[pid={}] message to port {} successfully",
            self.pid, self.message_hook_port
        );

        self.wechat_hook_post::<serde_json::Value, HashMap<String, serde_json::Value>>(
            constants::WECHAT_MSG_START_IMAGE_HOOK,
            serde_json::json!({ "save_path": save_path }),
        )
        .await?;
        info!(
            "hook instance[pid={}] image to path {} successfully",
            self.pid, save_path
        );

        self.wechat_hook_post::<serde_json::Value, HashMap<String, serde_json::Value>>(
            constants::WECHAT_MSG_START_VOICE_HOOK,
            serde_json::json!({ "save_path": save_path }),
        )
        .await?;
        info!(
            "hook instance[pid={}] voice to path {} successfully",
            self.pid, save_path
        );

        Ok(())
    }
}

#[derive(Serialize)]
struct ContactInfo {
    username: String,
    nickname: String,
    avatar_url: String,
    remark: String,
}

impl Clone for ContactInfo {
    fn clone(&self) -> Self {
        Self {
            username: self.username.clone(),
            nickname: self.nickname.clone(),
            avatar_url: self.avatar_url.clone(),
            remark: self.remark.clone(),
        }
    }
}

// wechat sql query related methods. Just wrap contact query related queries now
impl WechatInstance {
    async fn get_db_handle_by_name(&self, name: String) -> anyhow::Result<i64> {
        #[derive(Deserialize)]
        struct Data {
            db_name: String,
            handle: i64,
        }
        #[derive(Deserialize)]
        struct WechatGetDBHandleResp {
            data: Vec<Data>,
        }

        let resp: WechatGetDBHandleResp = self
            .wechat_hook_post(constants::WECHAT_DATABASE_GET_HANDLES, WechatNilBodyReq {})
            .await?;
        for i in &resp.data {
            if i.db_name == name {
                return Ok(i.handle);
            }
        }

        bail!("db_name[{}] not found", name)
    }

    async fn exec_sql(&self, db_name: String, sql: String) -> anyhow::Result<Vec<Vec<String>>> {
        #[derive(Deserialize)]
        struct ExecSqlResp {
            result: String,
            data: Vec<Vec<String>>,
        }

        let handle = self.get_db_handle_by_name(db_name).await?;
        let resp: ExecSqlResp = self
            .wechat_hook_post(
                constants::WECHAT_DATABASE_QUERY,
                WechatJsonSQLReq {
                    db_handle: handle,
                    sql,
                },
            )
            .await?;
        if resp.result != "OK" {
            bail!("exec sql failed: {}", resp.result)
        }

        Ok(resp.data)
    }

    async fn get_contacts(
        &self,
        db_name: String,
        sql: String,
        filter: Option<String>,
    ) -> anyhow::Result<Vec<ContactInfo>> {
        let query = match filter {
            Some(cond) => format!("{} {}", sql, cond),
            None => sql,
        };
        let resp = self.exec_sql(db_name, query).await?;
        if resp.len() < 2 || resp[1].len() != 5 {
            bail!("no contact found")
        }

        let mut data: Vec<ContactInfo> = vec![];
        for i in &resp[1..] {
            if i.len() < 5 {
                bail!("data shape wrong, want 5 but get {}", i.len())
            }

            data.push(ContactInfo {
                username: i[0].clone(),
                nickname: i[1].clone(),
                avatar_url: match i[2].len() {
                    0 => i[3].clone(),
                    _ => i[2].clone(),
                },
                remark: i[4].clone(),
            });
        }
        Ok(data)
    }

    async fn get_micro_msg_contacts(
        &self,
        filter_id: Option<String>,
    ) -> anyhow::Result<Vec<ContactInfo>> {
        self.get_contacts(
            constants::DB_MICRO_MSG.to_string(),
            String::from("SELECT c.UserName, c.NickName, i.bigHeadImgUrl, i.smallHeadImgUrl, c.Remark FROM Contact AS c LEFT JOIN ContactHeadImgUrl AS i ON c.UserName = i.usrName"),
            filter_id.map(|id| format!("WHERE c.UserName=\"{}\"", id)),
        )
        .await
    }

    async fn get_open_im_contacts(
        &self,
        filter_id: Option<String>,
    ) -> anyhow::Result<Vec<ContactInfo>> {
        self.get_contacts(
            constants::DB_OPEN_IM_CONTACT.to_string(),
            String::from("SELECT UserName, NickName, BigHeadImgUrl, SmallHeadImgUrl, Remark FROM OpenIMContact"),
            filter_id.map(|id| format!("WHERE UserName=\"{}\"", id)),
        )
        .await
    }

    async fn get_contact_by_id(&self, wechat_id: String) -> anyhow::Result<ContactInfo> {
        let contacts = match wechat_id.ends_with("@openim") {
            true => self.get_open_im_contacts(Some(wechat_id)).await?,
            false => self.get_micro_msg_contacts(Some(wechat_id)).await?,
        };

        Ok(contacts[0].clone())
    }
}

impl WechatInstance {
    pub async fn is_login(&self) -> anyhow::Result<bool> {
        #[derive(Deserialize)]
        struct WechatCheckLoginResp {
            is_login: u8,
            result: String,
        }

        let resp: WechatCheckLoginResp = self
            .wechat_hook_post(constants::WECHAT_IS_LOGIN, WechatNilBodyReq {})
            .await?;

        if resp.result != "OK" {
            error!("parse is_login resp failed: {}", resp.result);
            bail!("parse is_login resp failed: {}", resp.result)
        }

        info!("log status: {}", resp.is_login);

        Ok(resp.is_login == 1)
    }

    pub fn is_alive(&self) -> anyhow::Result<bool> {
        let s = System::new_all();
        let proc = match s.process(Pid::from_u32(self.pid)) {
            Some(p) => p,
            None => {
                bail!("cannot find process[{}]", self.pid)
            }
        };
        Ok(proc.status() == ProcessStatus::Run)
    }

    pub fn kill_self_process(&self) -> anyhow::Result<bool> {
        let s = System::new_all();
        let proc = match s.process(Pid::from_u32(self.pid)) {
            Some(p) => p,
            None => {
                bail!("cannot find process[{}]", self.pid)
            }
        };
        Ok(proc.kill())
    }

    pub async fn get_login_qrcode<'a>(&self) -> anyhow::Result<Vec<u8>> {
        // FIXME(duo): skip the first qr code
        sleep(Duration::from_secs(3)).await;

        let resp = self
            .wechat_hook_post_raw(constants::WECHAT_GET_QRCODE_IMAGE, WechatNilBodyReq {})
            .await?;

        match serde_json::from_slice::<WechatErrorResp>(&resp) {
            Ok(r) => {
                error!(
                    "request for get_qrcode_image failed: {} with result {}",
                    r.msg, r.result
                );
                bail!("get qrcode image failed: {}", r.msg)
            }
            Err(_) => Ok(Vec::from(resp)),
        }
    }

    #[allow(dead_code)]
    pub async fn logout(&self) -> anyhow::Result<()> {
        self.wechat_hook_post_raw(constants::WECHAT_LOGOUT, WechatNilBodyReq {})
            .await?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde_with::serde_as]
pub struct WechatUserInfo {
    #[serde(rename = "wxId")]
    pub id: String,
    #[serde(rename = "wxNickName")]
    pub nickname: String,
    #[serde(rename = "wxBigAvatar")]
    pub avatar: String,
    #[serde(rename = "wxRemark")]
    pub remark: Option<String>,
}

impl From<ContactInfo> for WechatUserInfo {
    fn from(contact: ContactInfo) -> Self {
        Self {
            id: contact.username,
            nickname: contact.nickname,
            avatar: contact.avatar_url,
            remark: Some(contact.remark),
        }
    }
}

// warp user related API
impl WechatInstance {
    pub async fn get_self(&self) -> anyhow::Result<WechatUserInfo> {
        #[derive(Deserialize)]
        struct WechatGetSelfResp {
            result: String,
            data: WechatUserInfo,
        }

        let resp: WechatGetSelfResp = self
            .wechat_hook_post(constants::WECHAT_GET_SELF_INFO, WechatNilBodyReq {})
            .await?;

        if resp.result != "OK" {
            bail!("parse get_self resp failed: {}", resp.result)
        }
        Ok(resp.data)
    }

    pub async fn get_user_info(&self, wechat_id: String) -> anyhow::Result<WechatUserInfo> {
        let info = self.get_contact_by_id(wechat_id).await?;
        Ok(WechatUserInfo::from(info))
    }

    pub async fn get_friend_list(&self) -> anyhow::Result<Vec<WechatUserInfo>> {
        let micro_msg_contacts = self.get_micro_msg_contacts(None).await?;
        let open_im_contacts = self.get_open_im_contacts(None).await?;
        Ok(micro_msg_contacts
            .into_iter()
            .chain(
                open_im_contacts
                    .into_iter()
                    .filter(|contact| !contact.username.ends_with("@chatroom")),
            )
            .map(WechatUserInfo::from)
            .collect())
    }
}

#[derive(Serialize, Debug)]
#[serde_with::serde_as]
pub struct WechatGroupInfo {
    #[serde(rename(serialize = "wxId"))]
    pub id: String,

    #[serde(rename(serialize = "wxNickName"))]
    pub nickname: String,

    #[serde(rename(serialize = "wxBigAvatar"))]
    pub avatar: String,

    #[serde(rename(serialize = "notice"))]
    pub notice: String,

    #[serde(rename(serialize = "members"))]
    pub member_ids: Vec<String>,
}

impl From<ContactInfo> for WechatGroupInfo {
    fn from(contact: ContactInfo) -> Self {
        Self {
            id: contact.username,
            nickname: contact.nickname,
            avatar: contact.avatar_url,
            notice: String::new(),
            member_ids: vec![],
        }
    }
}

// warp group related API
impl WechatInstance {
    pub async fn get_group_info(&self, wechat_id: String) -> anyhow::Result<WechatGroupInfo> {
        let info = self.get_contact_by_id(wechat_id.clone()).await?;
        Ok(WechatGroupInfo {
            id: info.username,
            nickname: info.nickname,
            avatar: info.avatar_url,
            notice: String::new(),
            member_ids: self.get_group_members(wechat_id).await?,
        })
    }

    pub async fn get_group_members(&self, group_id: String) -> anyhow::Result<Vec<String>> {
        #[derive(Deserialize)]
        struct WechatGetGroupMembersResp {
            members: String,
            result: String,
        }

        let resp: WechatGetGroupMembersResp = self
            .wechat_hook_post(
                constants::WECHAT_CHATROOM_GET_MEMBER_LIST,
                serde_json::json!({
                    "chatroom_id": group_id,
                }),
            )
            .await?;

        if resp.result != "OK" {
            bail!("parse get group members failed: {}", resp.result)
        }

        Ok(resp
            .members
            .split("^G")
            .map(|str| str.to_string())
            .collect::<Vec<String>>())
    }

    pub async fn get_group_member_nickname(
        &self,
        group_id: String,
        wechat_id: String,
    ) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct WechatGetGroupMemberNicknameResp {
            nickname: String,
        }

        let resp: WechatGetGroupMemberNicknameResp = self
            .wechat_hook_post(
                constants::WECHAT_CHATROOM_GET_MEMBER_NICKNAME,
                serde_json::json!({
                    "chatroom_id": group_id,
                    "wxid": wechat_id,
                }),
            )
            .await?;
        Ok(resp.nickname)
    }

    pub async fn get_group_list(&self) -> anyhow::Result<Vec<WechatGroupInfo>> {
        Ok(self
            .get_micro_msg_contacts(None)
            .await?
            .into_iter()
            .filter(|contact| contact.username.ends_with("@chatroom"))
            .map(WechatGroupInfo::from)
            .collect())
    }
}

// warp message send API including text, at, image and file
impl WechatInstance {
    pub async fn send_message(&self, msg: MatrixRequestDataMessage) -> anyhow::Result<()> {
        match msg {
            MatrixRequestDataMessage {
                target,
                content,
                message_type: MatrixMessageType::Text,
                data: None,
                ..
            } => self.send_text(target, content).await?,

            MatrixRequestDataMessage {
                target,
                content,
                message_type: MatrixMessageType::Text,
                data: Some(MatrixMessageDataField::Mentions(mentions)),
                ..
            } => {
                if mentions.is_empty() {
                    self.send_text(target, content).await?
                } else {
                    self.send_at_text(target, content, mentions).await?
                }
            }

            MatrixRequestDataMessage {
                target,
                message_type: MatrixMessageType::Image,
                data: Some(MatrixMessageDataField::Media(media)),
                ..
            }
            | MatrixRequestDataMessage {
                target,
                message_type: MatrixMessageType::Video,
                data: Some(MatrixMessageDataField::Media(media)),
                ..
            } => {
                let path = self.save_media(media).await?;
                self.send_image(target, path).await?;
            }

            MatrixRequestDataMessage {
                target,
                message_type: MatrixMessageType::File,
                data: Some(MatrixMessageDataField::Media(media)),
                ..
            } => {
                let path = self.save_media(media).await?;
                self.send_file(target, path).await?;
            }

            _ => bail!("message type and data are mismatched"),
        }
        Ok(())
    }

    async fn save_media(&self, media: MatrixMessageDataMedia) -> anyhow::Result<String> {
        let media_blob = utils::get_file_maybe_gzip_decompress(media.url).await?;
        let filepath = match media.name.len() {
            0 => Path::new(&self.save_path)
                .join("matrix_media")
                .join(utils::calculate_md5(&media_blob)),
            _ => Path::new(&self.save_path)
                .join("matrix_media")
                .join(media.name),
        };
        let mut file = File::create(filepath.clone()).await?;
        file.write_all(&media_blob).await?;
        match filepath.into_os_string().into_string() {
            Ok(p) => Ok(p),
            Err(e) => bail!("convert filepath {:?} failed", e),
        }
    }

    pub async fn send_text(&self, recv_wechat_id: String, msg: String) -> anyhow::Result<()> {
        self.wechat_hook_post_raw(
            constants::WECHAT_MSG_SEND_TEXT,
            serde_json::json!({ "wxid": recv_wechat_id, "msg": msg }),
        )
        .await?;
        Ok(())
    }

    pub async fn send_at_text(
        &self,
        recv_wechat_id: String,
        msg: String,
        mentions: Vec<String>,
    ) -> anyhow::Result<()> {
        let wechat_ids = mentions.join(",");
        self.wechat_hook_post_raw(
            constants::WECHAT_MSG_SEND_AT,
            serde_json::json!({
                "chatroom_id": recv_wechat_id,
                "msg": msg,
                "wxids": wechat_ids,
                "auto_nickname": 0,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn send_image(&self, recv_wechat_id: String, img_path: String) -> anyhow::Result<()> {
        self.wechat_hook_post_raw(
            constants::WECHAT_MSG_SEND_IMAGE,
            serde_json::json!({
                "receiver": recv_wechat_id,
                "img_path": img_path,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn send_file(&self, recv_wechat_id: String, file_path: String) -> anyhow::Result<()> {
        self.wechat_hook_post_raw(
            constants::WECHAT_MSG_SEND_FILE,
            serde_json::json!({
                "receiver": recv_wechat_id,
                "file_path": file_path,
            }),
        )
        .await?;
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
#[serde_with::serde_as]
pub struct WechatMessage {
    pub pid: u32,
    #[serde(rename = "msgid")]
    pub message_id: u64,
    #[serde(with = "ts_seconds")]
    pub timestamp: DateTime<Utc>,
    // #[serde_as(as = "TimestampMilliSeconds<String, Flexible>")]
    // pub time: DateTime<Utc>,
    #[serde(rename = "wxid")]
    pub wechat_id: String,
    pub sender: String,
    #[serde(rename = "self")]
    pub self_id: String,
    #[serde(rename = "isSendMsg")]
    pub is_send_message: i8,
    #[serde(rename = "isSendByPhone")]
    pub is_send_by_phone: Option<i8>,
    #[serde(rename = "type")]
    pub msg_type: WechatMessageType,
    pub message: String,
    #[serde(rename = "filepath")]
    #[serde(default = "nil_string")]
    pub file_path: String,
    pub thumb_path: String,
    // extra_info is a xml - json hybrid message
    #[serde(rename = "extrainfo")]
    pub extra_info: String,
}

fn nil_string() -> String {
    "".to_string()
}

#[derive(Deserialize_repr, Debug)]
#[repr(u32)]
pub enum WechatMessageType {
    Unknown = 0,
    Text = 1,
    Image = 3,
    Voice = 34,
    Video = 43,
    Sticker = 47,
    Location = 48,
    App = 49,
    PrivateVoIP = 50,
    LastMessage = 51,
    Hint = 10000, // hint info like revoke or tickle
    System = 10002,
}

#[derive(Debug)]
pub enum WechatMessageAppType {
    File = 6,
    Sticker = 8,
    Reply = 57,
    Notice = 87,
    Other,
}

impl TryFrom<u32> for WechatMessageAppType {
    type Error = ();
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::File as u32 => Ok(Self::File),
            x if x == Self::Sticker as u32 => Ok(Self::Sticker),
            x if x == Self::Reply as u32 => Ok(Self::Reply),
            x if x == Self::Notice as u32 => Ok(Self::Notice),
            _ => Ok(Self::Other),
        }
    }
}

impl<'de> Deserialize<'de> for WechatMessageAppType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match u32::deserialize(deserializer)? {
            x if x == Self::File as u32 => Self::File,
            x if x == Self::Sticker as u32 => Self::Sticker,
            x if x == Self::Reply as u32 => Self::Reply,
            x if x == Self::Notice as u32 => Self::Notice,
            _ => Self::Other,
        })
    }
}
