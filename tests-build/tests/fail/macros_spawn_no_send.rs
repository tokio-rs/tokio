use std::rc::Rc;

use tests_build::tokio;

async fn send_f() {}

#[tokio::test(spawn = true)]
async fn no_send_test() {
    let _value = Rc::new(0);
    send_f().await;
}

#[tokio::main(spawn = true)]
async fn no_send_main() {
    let _value = Rc::new(0);
    send_f().await;
}

fn main() {}
