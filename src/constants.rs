#![allow(dead_code)]

pub fn wechat_api(port: u32, msg_type: u32) -> String {
    format!("http://127.0.0.1:{}/api/?type={}", port, msg_type)
}

pub const DB_MICRO_MSG: &str = "MicroMsg.db";
pub const DB_OPEN_IM_CONTACT: &str = "OpenIMContact.db";

// login check
pub const WECHAT_IS_LOGIN: u32 = 0; // 登录检查

// self info
pub const WECHAT_GET_SELF_INFO: u32 = 1; // 获取个人信息

// send message
pub const WECHAT_MSG_SEND_TEXT: u32 = 2; // 发送文本
pub const WECHAT_MSG_SEND_AT: u32 = 3; // 发送群艾特
pub const WECHAT_MSG_SEND_CARD: u32 = 4; // 分享好友名片
pub const WECHAT_MSG_SEND_IMAGE: u32 = 5; // 发送图片
pub const WECHAT_MSG_SEND_FILE: u32 = 6; // 发送文件
pub const WECHAT_MSG_SEND_ARTICLE: u32 = 7; // 发送xml文章
pub const WECHAT_MSG_SEND_APP: u32 = 8; // 发送小程序

// receive message
pub const WECHAT_MSG_START_HOOK: u32 = 9; // 开启接收消息HOOK，只支持socket监听
pub const WECHAT_MSG_STOP_HOOK: u32 = 10; // 关闭接收消息HOOK
pub const WECHAT_MSG_START_IMAGE_HOOK: u32 = 11; // 开启图片消息HOOK
pub const WECHAT_MSG_STOP_IMAGE_HOOK: u32 = 12; // 关闭图片消息HOOK
pub const WECHAT_MSG_START_VOICE_HOOK: u32 = 13; // 开启语音消息HOOK
pub const WECHAT_MSG_STOP_VOICE_HOOK: u32 = 14; // 关闭语音消息HOOK

// contact
pub const WECHAT_CONTACT_GET_LIST: u32 = 15; // 获取联系人列表
pub const WECHAT_CONTACT_CHECK_STATUS: u32 = 16; // 检查是否被好友删除
pub const WECHAT_CONTACT_DEL: u32 = 17; // 删除好友
pub const WECHAT_CONTACT_SEARCH_BY_CACHE: u32 = 18; // 从内存中获取好友信息
pub const WECHAT_CONTACT_SEARCH_BY_NET: u32 = 19; // 网络搜索用户信息
pub const WECHAT_CONTACT_ADD_BY_WXID: u32 = 20; // wxid加好友
pub const WECHAT_CONTACT_ADD_BY_V3: u32 = 21; // v3数据加好友
pub const WECHAT_CONTACT_ADD_BY_PUBLIC_ID: u32 = 22; // 关注公众号
pub const WECHAT_CONTACT_VERIFY_APPLY: u32 = 23; // 通过好友请求
pub const WECHAT_CONTACT_EDIT_REMARK: u32 = 24; // 修改备注

// chatroom
pub const WECHAT_CHATROOM_GET_MEMBER_LIST: u32 = 25; // 获取群成员列表
pub const WECHAT_CHATROOM_GET_MEMBER_NICKNAME: u32 = 26; // 获取指定群成员昵称
pub const WECHAT_CHATROOM_DEL_MEMBER: u32 = 27; // 删除群成员
pub const WECHAT_CHATROOM_ADD_MEMBER: u32 = 28; // 添加群成员
pub const WECHAT_CHATROOM_SET_ANNOUNCEMENT: u32 = 29; // 设置群公告
pub const WECHAT_CHATROOM_SET_CHATROOM_NAME: u32 = 30; // 设置群聊名称
pub const WECHAT_CHATROOM_SET_SELF_NICKNAME: u32 = 31; // 设置群内个人昵称

// database
pub const WECHAT_DATABASE_GET_HANDLES: u32 = 32; // 获取数据库句柄
pub const WECHAT_DATABASE_BACKUP: u32 = 33; // 备份数据库
pub const WECHAT_DATABASE_QUERY: u32 = 34; // 数据库查询

// version
pub const WECHAT_SET_VERSION: u32 = 35; // 修改微信版本号

// log
pub const WECHAT_LOG_START_HOOK: u32 = 36; // 开启日志信息HOOK
pub const WECHAT_LOG_STOP_HOOK: u32 = 37; // 关闭日志信息HOOK

// browser
pub const WECHAT_BROWSER_OPEN_WITH_URL: u32 = 38; // 打开微信内置浏览器
pub const WECHAT_GET_PUBLIC_MSG: u32 = 39; // 获取公众号历史消息
pub const WECHAT_MSG_FORWARD_MESSAGE: u32 = 40; // 转发消息
pub const WECHAT_GET_QRCODE_IMAGE: u32 = 41; // 获取二维码
pub const WECHAT_GET_A8KEY: u32 = 42; // 获取A8Key
pub const WECHAT_MSG_SEND_XML: u32 = 43; // 发送xml消息
pub const WECHAT_LOGOUT: u32 = 44; // 退出登录
pub const WECHAT_GET_TRANSFER: u32 = 45; // 收款

pub const DEFAULT_WRITE_WS_RETRY_TIME: u8 = 3;
pub const MAX_WECHAT_CALLBACK_FAIL_COUNT: u8 = 0;
pub const MAX_WS_RECONNECT_COUNT: u32 = 5;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36 Edg/87.0.664.66";
