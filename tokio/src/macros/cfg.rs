#![allow(unused_macros)]

macro_rules! cfg_atomic_waker {
    ($($item:item)*) => {
        $( #[cfg(any(feature = "io-driver", feature = "time"))] $item )*
    }
}

macro_rules! cfg_blocking {
    ($($item:item)*) => {
        $( #[cfg(feature = "blocking")] $item )*
    }
}

/// Enable blocking API internals
macro_rules! cfg_blocking_impl {
    ($($item:item)*) => {
        $(
            #[cfg(any(
                    feature = "blocking",
                    feature = "fs",
                    feature = "dns",
                    feature = "io-std",
                    feature = "rt-threaded",
                    ))]
            $item
        )*
    }
}

/// Enable blocking API internals
macro_rules! cfg_not_blocking_impl {
    ($($item:item)*) => {
        $(
            #[cfg(not(any(
                        feature = "blocking",
                        feature = "fs",
                        feature = "dns",
                        feature = "io-std",
                        feature = "rt-threaded",
                        )))]
            $item
        )*
    }
}

macro_rules! cfg_dns {
    ($($item:item)*) => {
        $( #[cfg(feature = "dns")] $item )*
    }
}

macro_rules! cfg_fs {
    ($($item:item)*) => { $( #[cfg(feature = "fs")] $item )* }
}

macro_rules! cfg_io_blocking {
    ($($item:item)*) => {
        $( #[cfg(any(feature = "io-std", feature = "fs"))] $item )*
    }
}

macro_rules! cfg_io_driver {
    ($($item:item)*) => {
        $( #[cfg(feature = "io-driver")] $item )*
    }
}

macro_rules! cfg_not_io_driver {
    ($($item:item)*) => {
        $( #[cfg(not(feature = "io-driver"))] $item )*
    }
}

macro_rules! cfg_io_std {
    ($($item:item)*) => {
        $( #[cfg(feature = "io-std")] $item )*
    }
}

macro_rules! cfg_io_util {
    ($($item:item)*) => {
        $( #[cfg(feature = "io-util")] $item )*
    }
}

macro_rules! cfg_not_io_util {
    ($($item:item)*) => {
        $( #[cfg(not(feature = "io-util"))] $item )*
    }
}

macro_rules! cfg_loom {
    ($($item:item)*) => {
        $( #[cfg(loom)] $item )*
    }
}

macro_rules! cfg_not_loom {
    ($($item:item)*) => {
        $( #[cfg(not(loom))] $item )*
    }
}

macro_rules! cfg_macros {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "macros")]
            #[doc(inline)]
            $item
        )*
    }
}

macro_rules! cfg_process {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "process")]
            #[cfg(not(loom))]
            $item
        )*
    }
}

macro_rules! cfg_signal {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "signal")]
            #[cfg(not(loom))]
            $item
        )*
    }
}

macro_rules! cfg_stream {
    ($($item:item)*) => {
        $( #[cfg(feature = "stream")] $item )*
    }
}

macro_rules! cfg_sync {
    ($($item:item)*) => {
        $( #[cfg(feature = "sync")] $item )*
    }
}

macro_rules! cfg_not_sync {
    ($($item:item)*) => {
        $( #[cfg(not(feature = "sync"))] $item )*
    }
}

macro_rules! cfg_rt_core {
    ($($item:item)*) => {
        $( #[cfg(feature = "rt-core")] $item )*
    }
}

macro_rules! cfg_not_rt_core {
    ($($item:item)*) => {
        $( #[cfg(not(feature = "rt-core"))] $item )*
    }
}

macro_rules! cfg_rt_threaded {
    ($($item:item)*) => {
        $( #[cfg(feature = "rt-threaded")] $item )*
    }
}

macro_rules! cfg_not_rt_threaded {
    ($($item:item)*) => {
        $( #[cfg(not(feature = "rt-threaded"))] $item )*
    }
}

macro_rules! cfg_tcp {
    ($($item:item)*) => {
        $( #[cfg(feature = "tcp")] $item )*
    }
}

macro_rules! cfg_test_util {
    ($($item:item)*) => {
        $( #[cfg(feature = "test-util")] $item )*
    }
}

macro_rules! cfg_not_test_util {
    ($($item:item)*) => {
        $( #[cfg(not(feature = "test-util"))] $item )*
    }
}

macro_rules! cfg_time {
    ($($item:item)*) => {
        $( #[cfg(feature = "time")] $item )*
    }
}

macro_rules! cfg_not_time {
    ($($item:item)*) => {
        $( #[cfg(not(feature = "time"))] $item )*
    }
}

macro_rules! cfg_udp {
    ($($item:item)*) => {
        $( #[cfg(feature = "udp")] $item )*
    }
}

macro_rules! cfg_uds {
    ($($item:item)*) => {
        $( #[cfg(all(unix, feature = "uds"))] $item )*
    }
}
