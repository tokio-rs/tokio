use tests_build::tokio;

async fn cfg_attr_is_rejected() {
    tokio::select! {
        #[cfg_attr(all(), allow(unused))]
        _ = async {} => {},
    }
}

async fn arbitrary_attribute_is_rejected() {
    tokio::select! {
        #[allow(unused)]
        _ = async {} => {},
    }
}

async fn stacked_cfg_is_rejected() {
    tokio::select! {
        #[cfg(all())]
        #[cfg(all())]
        _ = async {} => {},
    }
}

async fn cfg_before_else_is_rejected() {
    tokio::select! {
        #[cfg(all())]
        else => (),
    }
}

async fn cfg_before_biased_is_rejected() {
    tokio::select! {
        #[cfg(all())]
        biased;
        _ = async {} => {},
    }
}

async fn inner_attribute_is_rejected() {
    tokio::select! {
        #![cfg(all())]
        _ = async {} => {},
    }
}

fn main() {}
