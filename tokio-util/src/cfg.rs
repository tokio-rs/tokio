macro_rules! cfg_codec {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "codec")]
            #[cfg_attr(docsrs, doc(cfg(feature = "codec")))]
            $item
        )*
    }
}

macro_rules! cfg_compat {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "compat")]
            #[cfg_attr(docsrs, doc(cfg(feature = "compat")))]
            $item
        )*
    }
}

/*
macro_rules! cfg_udp {
    ($($item:item)*) => {
        $(
            #[cfg(all(feature = "udp", feature = "codec"))]
            #[cfg_attr(docsrs, doc(cfg(all(feature = "udp", feature = "codec"))))]
            $item
        )*
    }
}
*/

macro_rules! cfg_io {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "io")]
            #[cfg_attr(docsrs, doc(cfg(feature = "io")))]
            $item
        )*
    }
}

macro_rules! cfg_rt {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "rt")]
            #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
            $item
        )*
    }
}
