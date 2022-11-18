#![cfg(all(feature = "full", windows))] // Wasi does not support direct socket operations
use std::io;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
use tokio::sync::oneshot::{channel, Sender};
use tokio::time::sleep;

static PATH: &str = r"\\.\pipe\Tokio inbound bug MCVE";
static MSG: &str = "Hello from server!\n";
const NUM_CLIENTS: usize = 16;
const INBOUND: bool = false;

pub async fn server(snd: Sender<()>) -> io::Result<()> {
    async fn handle_conn(mut conn: NamedPipeServer) -> io::Result<()> {
        conn.write_all(MSG.as_bytes()).await?;
        drop(conn);

        Ok(())
    }

    let mut srv = ServerOptions::new()
        .access_inbound(INBOUND)
        .access_outbound(true)
        .first_pipe_instance(true)
        .create(PATH)?;

    let _ = snd.send(());

    let mut tasks = Vec::with_capacity(NUM_CLIENTS);

    for _ in 0..NUM_CLIENTS {
        srv.connect().await?;
        let new_srv = ServerOptions::new()
            .access_inbound(INBOUND)
            .access_outbound(true)
            .create(PATH)?;
        let old_srv = std::mem::replace(&mut srv, new_srv);
        let task = tokio::spawn(handle_conn(old_srv));
        tasks.push(task);
    }
    for task in tasks {
        task.await??;
    }

    Ok(())
}
pub async fn client() -> io::Result<()> {
    let mut buffer = String::with_capacity(128);

    let mut conn = loop {
        match ClientOptions::new().write(false).open(PATH) {
            Err(e) if e.raw_os_error() == Some(winapi::shared::winerror::ERROR_PIPE_BUSY as _) => {
                sleep(Duration::from_millis(10)).await;
                continue;
            }
            not_busy => break not_busy,
        }
    }
    .map(BufReader::new)?;

    conn.read_line(&mut buffer).await?;

    assert_eq!(buffer, MSG);

    Ok(())
}

#[tokio::test]
async fn test_several_clients() {
    let (tx, rx) = channel();
    let srv = tokio::spawn(server(tx));
    let mut tasks = Vec::with_capacity(NUM_CLIENTS as _);
    let _ = rx.await;
    for _ in 0..NUM_CLIENTS {
        tasks.push(tokio::spawn(client()));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    srv.await.unwrap().unwrap();
}
