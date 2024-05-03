use std::path::PathBuf;

use crate::io;
use crate::sync::mpsc;
use crate::sync::mpsc::Receiver;

const WALKER_CHANNEL_BUFFER_SIZE: usize = 32;

/// Search for all files under that 'path' recursively, and send the file paths over the channel
/// # Example:
/// use tokio::fs::walk_dir;
/// let mut rx = walk_dir("./").await.unwrap();
/// while let Some(item) = rx.recv().await {
///    println!("{:?}", item);
/// }
pub async fn walk_dir(path: impl AsRef<str>) -> io::Result<Receiver<PathBuf>> {
    let path = PathBuf::from(path.as_ref());

    let (tx, rx) = mpsc::channel::<PathBuf>(WALKER_CHANNEL_BUFFER_SIZE);

    crate::spawn(async move {
        let mut dirs = Vec::<PathBuf>::with_capacity(1000);
        dirs.push(path);

        while let Some(dir) = dirs.pop() {
            let mut ls = super::read_dir(dir).await?;

            while let Ok(Some(item)) = ls.next_entry().await {
                if item.metadata().await?.is_dir() {
                    dirs.push(item.path());
                } else {
                    tx.send(item.path()).await.unwrap_or(());
                }
            }
        }
        Ok::<(), std::io::Error>(())
    });

    Ok(rx)
}
