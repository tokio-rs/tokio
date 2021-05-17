use autocfg::AutoCfg;

fn main() {
    match AutoCfg::new() {
        Ok(ac) => {
            // The #[track_caller] attribute was stabilized in rustc 1.46.0.
            if ac.probe_rustc_version(1, 46) {
                autocfg::emit("tokio_track_caller")
            }
        }

        Err(e) => {
            // If we couldn't detect the compiler version and features, just
            // print a warning. This isn't a fatal error: we can still build
            // Tokio, we just can't enable cfgs automatically.
            println!(
                "cargo:warning=tokio: failed to detect compiler features: {}",
                e
            );
        }
    }

    assert_platform_minimums();
}

/// Assert platform-minimum requirements for Tokio to work correctly. This might
/// be feasible to do as a constant fn, once MSRC is bumped a bit more.
fn assert_platform_minimums() {
    use std::mem;
    use std::sync::atomic::{AtomicIsize, AtomicUsize};

    if mem::size_of::<usize>() < 4 {
        panic!("Tokio only works correctly if usize is at least 4 bytes.");
    }

    if mem::size_of::<isize>() < 4 {
        panic!("Tokio only works correctly if isize is at least 4 bytes.");
    }

    if mem::size_of::<AtomicUsize>() < 4 {
        panic!("Tokio only works correctly if AtomicUsize is at least 4 bytes.");
    }

    if mem::size_of::<AtomicIsize>() < 4 {
        panic!("Tokio only works correctly if AtomicIsize is at least 4 bytes.");
    }
}
