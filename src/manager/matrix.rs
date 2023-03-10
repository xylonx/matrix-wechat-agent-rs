use anyhow::bail;
use log::info;

use crate::{
    wechat::WechatInstance,
    ws::{recv::MatrixRequestDataField, recv::WebsocketMatrixRequest, CommandType},
};
use std::sync::atomic::Ordering;

use super::WechatManager;

impl WechatManager {
    ///
    /// handle matrix events(WebsocketMatrixRequest) and send resp(WebsocketCommand) to websocket
    ///
    pub async fn handle_matrix_events(&self, msg: WebsocketMatrixRequest) -> anyhow::Result<()> {
        let mxid = msg.mxid.clone();
        let req_id = msg.req_id;
        if let Err(e) = self._handle_matrix_events(msg).await {
            self.write_command_error(mxid, req_id, e.to_string())
                .await?;
        }

        Ok(())
    }

    pub async fn _handle_matrix_events(&self, msg: WebsocketMatrixRequest) -> anyhow::Result<()> {
        // let ins = self.get_instance_by_mxid(msg.mxid)?;
        let mxid = msg.mxid;
        let req_id = msg.req_id;

        info!("recv matrix event: command type: {:?}", msg.command);

        match msg.command {
            CommandType::Connect => {
               let ins = match self.get_instance_by_mxid(mxid.clone()) {
                    Ok(ins) => ins,
                    Err(_) => {
                        let port = self.wechat_listen_port.fetch_add(1, Ordering::SeqCst);
                        WechatInstance::new(
                            port,
                            self.save_path.clone(),
                            self.message_hook_port,
                            mxid.clone(),
                        )?
                    }
                };
                ins.hook_wechat_message(self.save_path.clone()).await?;
                self.store_instance(mxid.clone(), ins)?;

                self.write_command_resp::<String>(mxid, req_id, None)
                    .await?;
            }

            CommandType::Disconnect => {
                self.drop_instance(mxid.clone())?;
                self.write_command_resp::<String>(mxid, req_id, None)
                    .await?;
            }

            CommandType::LoginWithQRCode => {
                self.write_command_resp(
                    mxid.clone(),
                    req_id,
                    Some(self.get_instance_by_mxid(mxid)?.get_login_qrcode().await?),
                )
                .await?;
            }

            CommandType::IsLogin => {
                self.write_command_resp(
                    mxid.clone(),
                    req_id,
                    Some(serde_json::json!({
                        "status": self.get_instance_by_mxid(mxid)?.is_login().await.unwrap_or(false) ,
                    })),
                )
                .await?
            }

            CommandType::GetSelf => {
                let ins = self.get_instance_by_mxid(mxid.clone())?;
                self.write_command_resp(mxid, req_id, Some(ins.get_self().await?))
                    .await?;
            }

            CommandType::GetUserInfo => match msg.data {
                Some(MatrixRequestDataField::Query(q)) => {
                    self.write_command_resp(
                        mxid.clone(),
                        req_id,
                        Some(
                            self.get_instance_by_mxid(mxid)?
                                .get_user_info(q.wechat_id)
                                .await?,
                        ),
                    )
                    .await?
                }
                _ => bail!("deserialize matrix message failed"),
            },

            CommandType::GetGroupInfo => match msg.data {
                Some(MatrixRequestDataField::Query(q)) => {
                    self.write_command_resp(
                        mxid.clone(),
                        req_id,
                        Some(
                            self.get_instance_by_mxid(mxid)?
                                .get_group_info(q.group_id)
                                .await?,
                        ),
                    )
                    .await?
                }
                _ => bail!("deserialize matrix message failed"),
            },

            CommandType::GetGroupMembers => match msg.data {
                Some(MatrixRequestDataField::Query(q)) => {
                    self.write_command_resp(
                        mxid.clone(),
                        req_id,
                        Some(
                            self.get_instance_by_mxid(mxid)?
                                .get_group_members(q.group_id)
                                .await?,
                        ),
                    )
                    .await?
                }
                _ => bail!("deserialize matrix message failed"),
            },

            CommandType::GetGroupMemberNickname => match msg.data {
                Some(MatrixRequestDataField::Query(q)) => {
                    self.write_command_resp(
                        mxid.clone(),
                        req_id,
                        Some(
                            self.get_instance_by_mxid(mxid)?
                                .get_group_member_nickname(q.group_id, q.wechat_id)
                                .await?,
                        ),
                    )
                    .await?
                }
                _ => bail!("deserialize matrix message failed"),
            },

            CommandType::GetFriendList => {
                self.write_command_resp(
                    mxid.clone(),
                    req_id,
                    Some(self.get_instance_by_mxid(mxid)?.get_friend_list().await?),
                )
                .await?
            }

            CommandType::GetGroupList => {
                self.write_command_resp(
                    mxid.clone(),
                    req_id,
                    Some(self.get_instance_by_mxid(mxid)?.get_group_list().await?),
                )
                .await?
            }

            CommandType::SendMessage => match msg.data {
                Some(MatrixRequestDataField::Message(msg)) => {
                    self.write_command_resp(
                        mxid.clone(),
                        req_id,
                        Some(self.get_instance_by_mxid(mxid)?.send_message(msg).await?),
                    )
                    .await?
                }

                _ => bail!("deserialize matrix message failed"),
            },

            _ => bail!("deserialize matrix message failed"),
        }

        Ok(())
    }
}
