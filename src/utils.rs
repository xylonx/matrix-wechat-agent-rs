use std::path::Path;
extern crate crypto;

use crypto::digest::Digest;
use crypto::md5::Md5;

use anyhow::bail;
use log::error;
use serde::Serialize;
use tokio::sync::mpsc::Sender;

use crate::constants;

pub async fn retriable_write<T: Serialize>(
    writer: Sender<String>,
    data: T,
    retry_time: Option<u8>,
) -> anyhow::Result<()> {
    let retry_time = retry_time.unwrap_or(constants::DEFAULT_RETRY_TIME);
    let data = serde_json::to_string(&data)?;
    for _ in 0..retry_time {
        match writer.send(data.clone()).await {
            Ok(_) => break,
            Err(e) => {
                error!("send message to ws failed: {}", e);
            }
        };
    }
    Ok(())
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
