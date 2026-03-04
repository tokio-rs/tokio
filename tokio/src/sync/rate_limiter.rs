//! Rate limiting utilities.

use super::Semaphore;
use self::error::AcquireError;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub mod error {
    //! Rate limiter errors.

    use std::error::Error;
    use std::fmt;

    /// Error returned when the rate limiter is closed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AcquireError;

    impl fmt::Display for AcquireError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "rate limiter closed")
        }
    }

    impl Error for AcquireError {}
}

/// Rate limiter based on token bucket algorithm.
///
/// This rate limiter controls the rate at which operations can be performed.
/// It allows for bursts of up to a certain capacity, then refills tokens
/// at a steady rate.
///
/// # Examples
///
/// ```
/// use tokio::sync::rate_limiter::RateLimiter;
/// use tokio::time::{sleep, Duration};
///
/// # #[tokio::main]
/// # async fn main() {
/// // Create a rate limiter that allows 10 requests per second,
/// // with a burst capacity of 20
/// let limiter = RateLimiter::new(10, 20);
///
/// // Acquire a permit (non-blocking)
/// let result = limiter.try_acquire();
/// println!("Acquired: {:?}", result.is_ok());
///
/// // Or acquire with waiting (blocking)
/// // limiter.acquire().await;
///
/// // Check available permits
/// println!("Available: {}", limiter.available_permits());
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    capacity: usize,
    refill_interval: Duration,
    last_refill: Arc<std::sync::Mutex<Instant>>,
}

impl RateLimiter {
    /// Creates a new rate limiter.
    ///
    /// The `rate` parameter specifies how many permits are added per second.
    /// The `capacity` parameter specifies the maximum number of permits
    /// that can be available at any time (for bursts).
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::rate_limiter::RateLimiter;
    ///
    /// let limiter = RateLimiter::new(10, 20); // 10 per second, burst of 20
    /// ```
    pub fn new(rate: u64, capacity: usize) -> Self {
        let refill_interval = Duration::from_secs_f64(1.0 / rate as f64);
        let semaphore = Semaphore::new(capacity);

        Self {
            semaphore: Arc::new(semaphore),
            capacity,
            refill_interval,
            last_refill: Arc::new(std::sync::Mutex::new(Instant::now())),
        }
    }

    /// Attempts to acquire a permit from the rate limiter.
    ///
    /// If no permits are available, this returns immediately with an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::rate_limiter::RateLimiter;
    ///
    /// let limiter = RateLimiter::new(10, 20);
    ///
    /// if let Ok(_permit) = limiter.try_acquire() {
    ///     println!("Acquired permit!");
    /// } else {
    ///     println!("No permits available");
    /// }
    /// ```
    pub fn try_acquire(&self) -> Result<RateLimiterPermit, AcquireError> {
        self.refill_tokens();
        
        match self.semaphore.try_acquire() {
            Ok(_permit) => Ok(RateLimiterPermit::new()),
            Err(_) => Err(AcquireError),
        }
    }

    /// Acquires a permit from the rate limiter, waiting until one is available.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::rate_limiter::RateLimiter;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let limiter = RateLimiter::new(10, 20);
    ///
    /// // Wait for a permit to become available
    /// limiter.acquire().await.unwrap();
    /// println!("Acquired permit!");
    /// # }
    /// ```
    #[cfg(feature = "rt")]
    pub async fn acquire(&self) -> Result<RateLimiterPermit, AcquireError> {
        self.refill_tokens();
        
        match self.semaphore.acquire().await {
            Ok(_permit) => Ok(RateLimiterPermit::new()),
            Err(_) => Err(AcquireError),
        }
    }

    /// Returns the number of available permits.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::rate_limiter::RateLimiter;
    ///
    /// let limiter = RateLimiter::new(10, 20);
    /// println!("Available: {}", limiter.available_permits());
    /// ```
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Returns the capacity of the rate limiter.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    fn refill_tokens(&self) {
        let mut last_refill = self.last_refill.lock().unwrap();
        let now = Instant::now();
        
        let elapsed = now.duration_since(*last_refill);
        let tokens_to_add = (elapsed.as_secs_f64() / self.refill_interval.as_secs_f64()) as usize;
        
        if tokens_to_add > 0 {
            let available = self.semaphore.available_permits();
            let new_permits = (tokens_to_add).min(self.capacity - available);
            
            if new_permits > 0 {
                self.semaphore.add_permits(new_permits);
            }
            
            *last_refill = now;
        }
    }
}

/// A permit acquired from the rate limiter.
///
/// Dropping this permit effectively "releases" it back to the bucket,
/// allowing more operations to proceed.
#[derive(Debug)]
pub struct RateLimiterPermit {
    _private: (),
}

impl RateLimiterPermit {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }
}

impl Drop for RateLimiterPermit {
    fn drop(&mut self) {
        // Permit is automatically released when dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(10, 20);
        assert_eq!(limiter.capacity(), 20);
        assert_eq!(limiter.available_permits(), 20);
    }

    #[test]
    fn test_try_acquire() {
        let limiter = RateLimiter::new(10, 2);
        
        let result1 = limiter.try_acquire();
        assert!(result1.is_ok());
        
        let result2 = limiter.try_acquire();
        assert!(result2.is_ok());
        
        let result3 = limiter.try_acquire();
        assert!(result3.is_err());
    }

    #[test]
    fn test_available_permits() {
        let limiter = RateLimiter::new(10, 5);
        assert_eq!(limiter.available_permits(), 5);
        
        let _ = limiter.try_acquire();
        assert_eq!(limiter.available_permits(), 4);
    }

    #[tokio::test]
    async fn test_acquire() {
        let limiter = RateLimiter::new(10, 1);
        
        let permit = limiter.acquire().await;
        assert!(permit.is_ok());
    }
}
