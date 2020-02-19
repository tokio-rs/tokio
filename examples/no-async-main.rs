//! A simple client that does some work and triggers a callback asynchronously
//! with an API can be called from no-async code and uses async tokio inside
//!
//!  THIS DOES NOT WORK YET 
//! -- it does not compile 
//! -- may not be using the best idioms for what I'm attempting to illustrate
//!
//! To run the example:
//!
//!     cargo run --example no-async-main

use tokio::sync::{oneshot, mpsc};
use tokio::runtime::{Handle, Runtime};
use Command::Increment;

#[derive(Debug)]
enum Command {
    Increment,
    // Other commands can be added here
}

#[derive(Debug)]
enum Response {
  IncrementCompleted(u64)
}

struct Commander {
  runtime: Runtime,
}

#[derive(Clone)]
struct Sender {
  handle: Handle,
  cmd_tx: mpsc::Sender<(Command, oneshot::Sender<Response>)>
}

impl Commander {
  pub fn new() -> Commander {
    Commander {
      runtime: Runtime::new().unwrap(),
    }
  }
  pub fn connect(&mut self, mut ready_callback: impl FnMut(Sender) -> () + Send + 'static) {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(100);
    // Spawn a task to manage the counter
    self.runtime.spawn(async move {
      let mut counter: u64 = 0;
      while let Some((cmd, response)) = cmd_rx.recv().await {
        match cmd {
            Increment => {
                counter += 1;
                response.send(Response::IncrementCompleted(counter)).unwrap();
            }
        }
      }
    });
    ready_callback(Sender::new(cmd_tx, self.runtime.handle().clone()));
  }

}

impl Sender {
  pub fn new(cmd_tx: mpsc::Sender<(Command, oneshot::Sender<Response>)>,
             handle: Handle) -> Self {
    Sender {
      handle,
      cmd_tx
    }
  }

  pub fn send_command(&mut self, cmd: Command, f: impl Fn(Response) -> () + Send + 'static)
  {
    let (resp_tx, resp_rx) = oneshot::channel::<Response>();
    let mut cmd_tx = self.cmd_tx.clone();
    self.handle.spawn(async move {

      cmd_tx.send((cmd, resp_tx)).await.ok().unwrap();
      let res: Response = resp_rx.await.unwrap();

      println!("  => {:?}", res);
      f(res);
    });
  }
} // impl Sender


fn main() {
  let mut c = Commander::new();
  c.connect(move |mut sender| {
      println!("Yay!");
      sender.send_command(Increment, |response| {
        println!("  ==> received response {:?}", response);
      })
  });

  let mut input = String::new();
  println!("press return to quit");
  std::io::stdin().read_line(&mut input).expect("stdio read_line");

}
