extern crate crypto;
extern crate dirs;

use crypto::digest::Digest;
use crypto::md5::Md5;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use sysinfo::{ProcessExt, System, SystemExt};
use tokio::{fs::File, time::sleep};

use anyhow::bail;
use log::{error, info, warn, debug};
use serde::Serialize;
use tokio::sync::broadcast::Sender;

use crate::constants;

pub async fn retriable_write<T: Serialize>(
    writer: Sender<String>,
    data: T,
    retry_time: Option<u8>,
) -> anyhow::Result<()> {
    let retry_time = retry_time.unwrap_or(constants::DEFAULT_WRITE_WS_RETRY_TIME);
    let data = serde_json::to_string(&data)?;
    debug!("write message to ws: {}", data);
    for _ in 0..retry_time {
        match writer.send(data.clone()) {
            Ok(_) => break,
            Err(e) => {
                error!("send message to ws failed: {}", e);
            }
        };
    }
    Ok(())
}

pub async fn retriable_open_file(
    filename_seq: Vec<PathBuf>,
    retry_time: u32,
) -> anyhow::Result<File> {
    let mut wait = Duration::from_secs(1);
    for _ in 0..retry_time {
        for filename in &filename_seq {
            if let Ok(f) = File::open(filename).await {
                return Ok(f);
            }
        }
        warn!(
            "open file failed. will wait {} seconds and retry",
            wait.as_secs()
        );
        sleep(wait).await;
        wait *= 2;
    }

    bail!(
        "open file {:?} with {} times failed",
        filename_seq,
        retry_time
    )
}

pub fn get_filename(path: &Path) -> anyhow::Result<String> {
    match path.file_name() {
        Some(fs) => match fs.to_str() {
            Some(f) => Ok(f.to_string()),
            None => {
                bail!("file_path[{}] contains invalid UTF8 char", path.display())
            }
        },
        None => bail!(
            "file_path[{}] does not contain a filename and extensions",
            path.display()
        ),
    }
}

pub async fn get_file_maybe_gzip_decompress(url: String) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .user_agent(constants::USER_AGENT)
        .gzip(true)
        .build()?;
    let resp = client.get(url).send().await?;
    Ok(Vec::from(resp.bytes().await?))
}

pub fn calculate_md5(blob: &Vec<u8>) -> String {
    let mut hasher = Md5::new();
    hasher.input(blob);
    let mut output = [0; 16]; // An MD5 is 16 bytes
    hasher.result(&mut output);
    output
        .into_iter()
        .map(|i| format!("{:02X}", i).to_string())
        .collect::<Vec<String>>()
        .join("")
}

pub fn get_wechat_document_dir() -> anyhow::Result<PathBuf> {
    let basedir = match dirs::document_dir() {
        Some(d) => d,
        None => bail!("get current user document dir failed"),
    };

    #[cfg(any(target_os = "windows"))]
    let basedir = get_wechat_document_dir_from_win_reg().unwrap_or(basedir);

    Ok(basedir.join("WeChat Files"))
}

#[cfg(any(target_os = "windows"))]
pub fn get_wechat_document_dir_from_win_reg() -> anyhow::Result<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let cur_ver = hkcu.open_subkey("SOFTWARE\\Tencent\\WeChat")?;
    let fsp: String = cur_ver.get_value("FileSavePath")?;
    if fsp != "MyDocument:" && fsp != "" {
        return Ok(Path::new(&fsp).to_path_buf());
    }
    error!("get wechat FileSavePath from registry key failed");
    bail!("get wechat FileSavePath from registry key failed")
}

pub fn kill_by_name(name: &str) {
    let mut s = System::new_all();
    s.refresh_processes();
    for p in s.processes_by_name(name) {
        match p.kill() {
            true => info!("kill process {} successfully", p.name()),
            false => warn!("kill process {} failed, please manually kill it", p.name()),
        };
    }
}
