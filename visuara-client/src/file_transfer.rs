//! File transfer: drag a file onto the controller's viewer window to send
//! it to the host machine, chunked over the (reliable, ordered) data
//! channel. v1 scope is one direction only (controller -> host), matching
//! the plan — no remote file browser yet.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use uuid::Uuid;
use webrtc::data_channel::RTCDataChannel;

use visuara_common::control::{ControlMessage, FileTransferMessage};

/// Comfortably under typical SCTP/data-channel message-size limits.
const CHUNK_SIZE: usize = 16 * 1024;

/// Controller side: reads a local file and streams it to the host.
pub async fn send_file(dc: &RTCDataChannel, path: &Path) -> Result<()> {
    let data = tokio::fs::read(path).await.with_context(|| format!("read {}", path.display()))?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let transfer_id = Uuid::new_v4().to_string();

    send(
        dc,
        ControlMessage::File(FileTransferMessage::Offer {
            transfer_id: transfer_id.clone(),
            file_name,
            size_bytes: data.len() as u64,
        }),
    )
    .await?;

    for (sequence, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
        send(
            dc,
            ControlMessage::File(FileTransferMessage::Chunk {
                transfer_id: transfer_id.clone(),
                sequence: sequence as u32,
                data: chunk.to_vec(),
            }),
        )
        .await?;
    }

    send(dc, ControlMessage::File(FileTransferMessage::Complete { transfer_id })).await?;
    Ok(())
}

async fn send(dc: &RTCDataChannel, msg: ControlMessage) -> Result<()> {
    let bytes = msg.to_bytes().context("encode control message")?;
    dc.send(&bytes.into()).await.context("send over data channel")?;
    Ok(())
}

/// Host side: receives incoming file transfers and writes them to disk.
pub struct FileReceiver {
    dest_dir: PathBuf,
    in_progress: Mutex<HashMap<String, (PathBuf, File)>>,
}

impl FileReceiver {
    pub fn new(dest_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dest_dir)
            .with_context(|| format!("create destination directory {}", dest_dir.display()))?;
        Ok(Self { dest_dir, in_progress: Mutex::new(HashMap::new()) })
    }

    /// Picks a sensible default: the user's Downloads folder if resolvable,
    /// otherwise a `visuara-received` folder next to the current directory.
    pub fn default_destination() -> PathBuf {
        dirs::download_dir().unwrap_or_else(|| PathBuf::from("visuara-received"))
    }

    pub fn handle(&self, msg: FileTransferMessage) -> Result<()> {
        match msg {
            FileTransferMessage::Offer { transfer_id, file_name, .. } => {
                let path = unique_destination_path(&self.dest_dir, &file_name);
                let file = File::create(&path).with_context(|| format!("create {}", path.display()))?;
                self.in_progress.lock().unwrap().insert(transfer_id, (path, file));
            }
            FileTransferMessage::Chunk { transfer_id, data, .. } => {
                if let Some((_, file)) = self.in_progress.lock().unwrap().get_mut(&transfer_id) {
                    file.write_all(&data).context("write file chunk")?;
                }
            }
            FileTransferMessage::Complete { transfer_id } => {
                if let Some((path, mut file)) = self.in_progress.lock().unwrap().remove(&transfer_id) {
                    file.flush().ok();
                    eprintln!("[visuara-host] received file: {}", path.display());
                }
            }
            FileTransferMessage::Cancel { transfer_id } => {
                if let Some((path, _)) = self.in_progress.lock().unwrap().remove(&transfer_id) {
                    let _ = std::fs::remove_file(path);
                }
            }
            FileTransferMessage::Accept { .. } | FileTransferMessage::Reject { .. } => {}
        }
        Ok(())
    }
}

/// Avoids clobbering an existing file of the same name.
fn unique_destination_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(file_name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = Path::new(file_name).extension().map(|e| e.to_string_lossy().to_string());
    for n in 1..10_000 {
        let name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_writes_chunks_in_order_and_finalizes_on_complete() {
        let dir = std::env::temp_dir().join(format!("visuara-file-transfer-test-{}", Uuid::new_v4()));
        let receiver = FileReceiver::new(dir.clone()).expect("create receiver");

        let transfer_id = "test-transfer".to_string();
        receiver
            .handle(FileTransferMessage::Offer {
                transfer_id: transfer_id.clone(),
                file_name: "hello.txt".to_string(),
                size_bytes: 11,
            })
            .expect("handle offer");
        receiver
            .handle(FileTransferMessage::Chunk { transfer_id: transfer_id.clone(), sequence: 0, data: b"hello ".to_vec() })
            .expect("handle chunk 1");
        receiver
            .handle(FileTransferMessage::Chunk { transfer_id: transfer_id.clone(), sequence: 1, data: b"world".to_vec() })
            .expect("handle chunk 2");
        receiver.handle(FileTransferMessage::Complete { transfer_id }).expect("handle complete");

        let contents = std::fs::read_to_string(dir.join("hello.txt")).expect("read written file");
        assert_eq!(contents, "hello world");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unique_destination_path_avoids_clobbering() {
        let dir = std::env::temp_dir().join(format!("visuara-file-transfer-unique-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "first").unwrap();

        let path = unique_destination_path(&dir, "a.txt");
        assert_eq!(path, dir.join("a (1).txt"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
