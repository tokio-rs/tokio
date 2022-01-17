use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main(worker_threads = 4)]
async fn main() {
    let listener = TcpListener::bind("0.0.0.0:33733").await.unwrap();
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            let mut s: f64 = 20181218.333333;
            loop {
                let loopcnt = socket.read_u64_le().await;
                if let Ok(loopcnt) = loopcnt {
                    for i in 1..=loopcnt {
                        let i = i as f64;
                        let i = i * i;
                        let s1 = s * i;
                        s = s1 / (s + 7.7);
                    }
                } else {
                    break;
                }
                socket.write_u64_le(s as u64).await.unwrap();
            }
        });
    }
}
