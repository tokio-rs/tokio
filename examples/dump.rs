#![allow(unknown_lints, unexpected_cfgs)]

//! This example demonstrates tokio's experimental task dumping functionality.
//! This application deadlocks. Input CTRL+C to display traces of each task, or
//! input CTRL+C twice within 1 second to quit.

#[cfg(all(
    tokio_unstable,
    tokio_taskdump,
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;
    use tokio::sync::Barrier;

    #[inline(never)]
    async fn a(barrier: Arc<Barrier>) {
        b(barrier).await
    }

    #[inline(never)]
    async fn b(barrier: Arc<Barrier>) {
        c(barrier).await
    }

    #[inline(never)]
    async fn c(barrier: Arc<Barrier>) {
        barrier.wait().await;
    }

    // Prints a task dump upon receipt of CTRL+C, or returns if CTRL+C is
    // inputted twice within a second.
    async fn dump_or_quit() {
        use tokio::time::{timeout, Duration, Instant};
        let handle = tokio::runtime::Handle::current();
        let mut last_signal: Option<Instant> = None;
        // wait for CTRL+C
        while let Ok(_) = tokio::signal::ctrl_c().await {
            // exit if a CTRL+C is inputted twice within 1 second
            if let Some(time_since_last_signal) = last_signal.map(|i| i.elapsed()) {
                if time_since_last_signal < Duration::from_secs(1) {
                    return;
                }
            }
            last_signal = Some(Instant::now());

            // capture a dump, and print each trace
            println!("{:-<80}", "");
            if let Ok(dump) = timeout(Duration::from_secs(2), handle.dump()).await {
                for task in dump.tasks().iter() {
                    let id = task.id();
                    let trace = task.trace();
                    println!("TASK {id}:");
                    println!("{trace}\n");
                }
            } else {
                println!("Task dumping timed out. Use a native debugger (like gdb) to debug the deadlock.");
            }
            println!("{:-<80}", "");
            println!("Input CTRL+C twice within 1 second to exit.");
        }
    }

    println!("This program has a deadlock.");
    println!("Input CTRL+C to print a task dump.");
    println!("Input CTRL+C twice within 1 second to exit.");

    // oops! this barrier waits for one more task than will ever come.
    let barrier = Arc::new(Barrier::new(3));

    let task_1 = tokio::spawn(a(barrier.clone()));
    let task_2 = tokio::spawn(a(barrier));

    tokio::select!(
        _ = dump_or_quit() => {},
        _ = task_1 => {},
        _ = task_2 => {},
    );

    Ok(())
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
