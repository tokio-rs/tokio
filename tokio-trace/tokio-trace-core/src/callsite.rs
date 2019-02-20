//! Callsites represent the source locations from which spans or events
//! originate.
use std::{
    fmt,
    hash::{Hash, Hasher},
    ptr,
    sync::Mutex,
};
use {
    dispatcher::{self, Dispatch},
    subscriber::Interest,
    Metadata,
};

lazy_static! {
    static ref REGISTRY: Mutex<Registry> = Mutex::new(Registry {
        callsites: Vec::new(),
        dispatchers: Vec::new(),
    });
}

struct Registry {
    callsites: Vec<&'static Callsite>,
    dispatchers: Vec<dispatcher::Registrar>,
}

/// Trait implemented by callsites.
pub trait Callsite: Sync {
    /// Adds the [`Interest`] returned by [registering] the callsite with a
    /// [dispatcher].
    ///
    /// If the interest is greater than or equal to the callsite's current
    /// interest, this should change whether or not the callsite is enabled.
    ///
    /// [`Interest`]: ::subscriber::Interest
    /// [registering]: ::subscriber::Subscriber::register_callsite
    /// [dispatcher]: ::Dispatch
    fn add_interest(&self, interest: Interest);

    /// Remove _all_ [`Interest`] from the callsite, disabling it.
    ///
    /// [`Interest`]: ::subscriber::Interest
    fn clear_interest(&self);

    /// Returns the [metadata] associated with the callsite.
    ///
    /// [metadata]: ::Metadata
    fn metadata(&self) -> &Metadata;
}

/// Uniquely identifies a [`Callsite`](::callsite::Callsite).
///
/// Two `Identifier`s are equal if they both refer to the same callsite.
#[derive(Clone)]
pub struct Identifier(
    /// **Warning**: The fields on this type are currently `pub` because it must
    /// be able to be constructed statically by macros. However, when `const
    /// fn`s are available on stable Rust, this will no longer be necessary.
    /// Thus, these fields are *not* considered stable public API, and they may
    /// change warning. Do not rely on any fields on `Identifier`. When
    /// constructing new `Identifier`s, use the `identify_callsite!` macro or
    /// the `Callsite::id` function instead.
    // TODO: When `Callsite::id` is a const fn, this need no longer be `pub`.
    #[doc(hidden)]
    pub &'static Callsite,
);

/// Register a new `Callsite` with the global registry.
///
/// This should be called once per callsite after the callsite has been
/// constructed.
pub fn register(callsite: &'static Callsite) {
    let mut registry = REGISTRY.lock().unwrap();
    let meta = callsite.metadata();
    registry.dispatchers.retain(|registrar| {
        match registrar.try_register(meta) {
            Some(interest) => {
                callsite.add_interest(interest);
                true
            }
            // TODO: if the dispatcher has been dropped, should we invalidate
            // any callsites that it previously enabled?
            None => false,
        }
    });
    registry.callsites.push(callsite);
}

pub(crate) fn register_dispatch(dispatch: &Dispatch) {
    let mut registry = REGISTRY.lock().unwrap();
    registry.dispatchers.push(dispatch.registrar());
    for callsite in &registry.callsites {
        let interest = dispatch.register_callsite(callsite.metadata());
        callsite.add_interest(interest);
    }
}

// ===== impl Identifier =====

impl PartialEq for Identifier {
    fn eq(&self, other: &Identifier) -> bool {
        ptr::eq(self.0, other.0)
    }
}

impl Eq for Identifier {}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Identifier({:p})", self.0)
    }
}

impl Hash for Identifier {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        (self.0 as *const Callsite).hash(state)
    }
}
