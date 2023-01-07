use crate::wechat::WechatInstance;
use crate::ws::send::{WebsocketCommand, WebsocketMessage};
use anyhow::bail;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::utils;
use crate::ws::{send::WebsocketEvent, CommandType};

mod matrix;
mod wechat;

pub struct WechatManager {
    message_hook_port: u32,
    save_path: String,
    wechat_listen_port: Arc<AtomicU32>,
    pid_instance_map: Arc<Mutex<HashMap<u32, WechatInstance>>>,
    mxid_pid_map: Arc<Mutex<HashMap<String, u32>>>,
    sender_chan: mpsc::Sender<String>,
}

impl Clone for WechatManager {
    fn clone(&self) -> Self {
        Self {
            message_hook_port: self.message_hook_port,
            save_path: self.save_path.clone(),
            wechat_listen_port: self.wechat_listen_port.clone(),
            pid_instance_map: self.pid_instance_map.clone(),
            mxid_pid_map: self.mxid_pid_map.clone(),
            sender_chan: self.sender_chan.clone(),
        }
    }
}

impl WechatManager {
    pub fn new(
        msg_hook_port: u32,
        save_path: String,
        sender_chan: mpsc::Sender<String>,
    ) -> WechatManager {
        WechatManager {
            message_hook_port: msg_hook_port,
            save_path: save_path,
            wechat_listen_port: Arc::new(AtomicU32::new(msg_hook_port + 1)),
            pid_instance_map: Arc::new(Mutex::new(HashMap::new())),
            mxid_pid_map: Arc::new(Mutex::new(HashMap::new())),
            sender_chan,
        }
    }
}
///
/// lock related methods
///
impl WechatManager {
    fn get_instance_by_pid(&self, pid: u32) -> anyhow::Result<WechatInstance> {
        let db = match self.pid_instance_map.lock() {
            Ok(db) => db,
            Err(err) => {
                bail!("lock db failed: {}", err)
            }
        };
        let instance = match db.get(&pid) {
            Some(ins) => ins.clone(),
            None => {
                bail!("cannot get wechat instance by pid: {}", pid)
            }
        };
        Ok(instance)
    }

    fn get_instance_by_mxid(&self, mxid: String) -> anyhow::Result<WechatInstance> {
        let mxid_map = match self.mxid_pid_map.lock() {
            Ok(db) => db,
            Err(err) => {
                bail!("lock db failed: {}", err)
            }
        };
        let pid = match mxid_map.get(&mxid) {
            Some(ins) => ins,
            None => {
                bail!("cannot get wechat pid by mxid: {}", mxid)
            }
        };

        self.get_instance_by_pid(pid.clone())
    }

    fn store_instance(&self, mxid: String, instance: WechatInstance) -> anyhow::Result<()> {
        let mut db = match self.pid_instance_map.lock() {
            Ok(db) => db,
            Err(err) => {
                bail!("lock db failed: {}", err)
            }
        };
        let mut mxid_map = match self.mxid_pid_map.lock() {
            Ok(db) => db,
            Err(err) => {
                bail!("lock db failed: {}", err)
            }
        };

        mxid_map.insert(mxid, instance.pid);
        db.insert(instance.pid, instance);

        Ok(())
    }

    fn drop_instance(&self, mxid: String) -> anyhow::Result<()> {
        let mut db = match self.pid_instance_map.lock() {
            Ok(db) => db,
            Err(err) => {
                bail!("lock db failed: {}", err)
            }
        };
        let mut mxid_map = match self.mxid_pid_map.lock() {
            Ok(db) => db,
            Err(err) => {
                bail!("lock db failed: {}", err)
            }
        };

        let pid = mxid_map.get(&mxid);
        if let None = pid {
            bail!("get pid by mxid[{}] failed", mxid)
        }
        let pid = pid.unwrap();

        db.remove(pid);
        mxid_map.remove(&mxid);

        Ok(())
    }
}

// utils methods
impl WechatManager {
    async fn write_to_ws<T: Serialize>(&self, data: T) -> anyhow::Result<()> {
        let writer = self.sender_chan.clone();
        utils::retriable_write(writer, data, None).await
    }

    async fn write_event_resp<T: Serialize>(&self, event: WebsocketEvent<T>) -> anyhow::Result<()> {
        self.write_to_ws(event).await
    }

    async fn write_command_resp<T: Serialize>(
        &self,
        mxid: String,
        req_id: i32,
        data: Option<T>,
    ) -> anyhow::Result<()> {
        let resp = self
            .write_to_ws(WebsocketMessage::Command(WebsocketCommand {
                mxid: mxid.clone(),
                req_id,
                command: CommandType::Response,
                data,
            }))
            .await;
        if let Err(e) = resp {
            self.write_command_error(mxid, req_id, e.to_string())
                .await?
        }
        Ok(())
    }

    async fn write_command_error(
        &self,
        mxid: String,
        req_id: i32,
        message: String,
    ) -> anyhow::Result<()> {
        self.write_to_ws(WebsocketMessage::Command(WebsocketCommand {
            mxid,
            req_id,
            command: CommandType::Error,
            data: serde_json::json!({ "message": message }),
        }))
        .await
    }
}
