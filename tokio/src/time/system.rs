#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct SystemTime {
    std: std::time::SystemTime,
}

pub struct SystemTimeError(std::time::SystemTimeError);

impl SystemTime {
    pub fn now() -> SystemTime {
        variant::now()
    }

    pub fn from_std(std: std::time::SystemTime) -> SystemTime {
        SystemTime { std }
    }
}

#[cfg(not(feature = "test-util"))]
mod variant {
    use super::SystemTime;

    pub(super) fn now() -> SystemTime {
        SystemTime::from_std(std::time::SystemTime::now())
    }
}

#[cfg(feature = "test-util")]
mod variant {
    use super::SystemTime;

    pub(super) fn now() -> SystemTime {
        crate::time::clock::system_now()
    }
}
