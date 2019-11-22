macro_rules! cfg_codec {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "codec")]
            #[cfg_attr(docsrs, doc(cfg(feature = "codec")))]
            $item
        )*
    }
}

macro_rules! cfg_udp {
    ($($item:item)*) => {
        $(
            #[cfg(all(feature = "udp", feature = "codec"))]
            #[cfg_attr(docsrs, doc(cfg(all(feature = "udp", feature = "codec"))))]
            $item
        )*
    }
}
