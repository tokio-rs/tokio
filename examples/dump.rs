//! This example demonstrates tokio's experimental taskdumping functionality.

#[cfg(all(
    tokio_unstable,
    tokio_taskdump,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
))]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use std::hint::black_box;

    #[inline(never)]
    async fn a() {
        black_box(b()).await
    }

    #[inline(never)]
    async fn b() {
        black_box(c()).await
    }

    #[inline(never)]
    async fn c() {
        black_box(tokio::task::yield_now()).await
    }

    tokio::spawn(a());
    tokio::spawn(b());
    tokio::spawn(c());

    let handle = tokio::runtime::Handle::current();
    let dump = handle.dump();

    for (i, task) in dump.tasks().iter().enumerate() {
        let trace = task.trace();
        println!("task {i} trace:");
        println!("{trace}");
    }
}

#[cfg(not(all(
    tokio_unstable,
    tokio_taskdump,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
)))]
fn main() {
    println!("task dumps are not available")
}
