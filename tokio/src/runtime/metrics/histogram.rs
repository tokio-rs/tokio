use crate::util::metric_atomics::MetricAtomicU64;
use std::sync::atomic::Ordering::Relaxed;

use std::cmp;
use std::ops::Range;

#[derive(Debug)]
pub(crate) struct Histogram {
    /// The histogram buckets
    buckets: Box<[MetricAtomicU64]>,

    /// Bucket scale, linear or log
    scale: HistogramScale,

    /// Minimum resolution
    resolution: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct HistogramBuilder {
    /// Histogram scale
    pub(crate) scale: HistogramScale,

    /// Must be a power of 2
    pub(crate) resolution: u64,

    /// Number of buckets
    pub(crate) num_buckets: usize,
}

#[derive(Debug)]
pub(crate) struct HistogramBatch {
    buckets: Box<[u64]>,
    scale: HistogramScale,
    resolution: u64,
}

cfg_unstable! {
    /// Whether the histogram used to aggregate a metric uses a linear or
    /// logarithmic scale.
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    #[non_exhaustive]
    pub enum HistogramScale {
        /// Linear bucket scale
        Linear,

        /// Logarithmic bucket scale
        Log,
    }
}

impl Histogram {
    pub(crate) fn num_buckets(&self) -> usize {
        self.buckets.len()
    }

    cfg_64bit_metrics! {
        pub(crate) fn get(&self, bucket: usize) -> u64 {
            self.buckets[bucket].load(Relaxed)
        }
    }

    pub(crate) fn bucket_range(&self, bucket: usize) -> Range<u64> {
        match self.scale {
            HistogramScale::Log => Range {
                start: if bucket == 0 {
                    0
                } else {
                    self.resolution << (bucket - 1)
                },
                end: if bucket == self.buckets.len() - 1 {
                    u64::MAX
                } else {
                    self.resolution << bucket
                },
            },
            HistogramScale::Linear => Range {
                start: self.resolution * bucket as u64,
                end: if bucket == self.buckets.len() - 1 {
                    u64::MAX
                } else {
                    self.resolution * (bucket as u64 + 1)
                },
            },
        }
    }
}

impl HistogramBatch {
    pub(crate) fn from_histogram(histogram: &Histogram) -> HistogramBatch {
        let buckets = vec![0; histogram.buckets.len()].into_boxed_slice();

        HistogramBatch {
            buckets,
            scale: histogram.scale,
            resolution: histogram.resolution,
        }
    }

    pub(crate) fn measure(&mut self, value: u64, count: u64) {
        self.buckets[self.value_to_bucket(value)] += count;
    }

    pub(crate) fn submit(&self, histogram: &Histogram) {
        debug_assert_eq!(self.scale, histogram.scale);
        debug_assert_eq!(self.resolution, histogram.resolution);
        debug_assert_eq!(self.buckets.len(), histogram.buckets.len());

        for i in 0..self.buckets.len() {
            histogram.buckets[i].store(self.buckets[i], Relaxed);
        }
    }

    fn value_to_bucket(&self, value: u64) -> usize {
        match self.scale {
            HistogramScale::Linear => {
                let max = self.buckets.len() - 1;
                cmp::min(value / self.resolution, max as u64) as usize
            }
            HistogramScale::Log => {
                let max = self.buckets.len() - 1;

                if value < self.resolution {
                    0
                } else {
                    let significant_digits = 64 - value.leading_zeros();
                    let bucket_digits = 64 - (self.resolution - 1).leading_zeros();
                    cmp::min(significant_digits as usize - bucket_digits as usize, max)
                }
            }
        }
    }
}

impl HistogramBuilder {
    pub(crate) fn new() -> HistogramBuilder {
        HistogramBuilder {
            scale: HistogramScale::Linear,
            // Resolution is in nanoseconds.
            resolution: 100_000,
            num_buckets: 10,
        }
    }

    pub(crate) fn build(&self) -> Histogram {
        let mut resolution = self.resolution;

        assert!(resolution > 0);

        if matches!(self.scale, HistogramScale::Log) {
            resolution = resolution.next_power_of_two();
        }

        Histogram {
            buckets: (0..self.num_buckets)
                .map(|_| MetricAtomicU64::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            resolution,
            scale: self.scale,
        }
    }
}

impl Default for HistogramBuilder {
    fn default() -> HistogramBuilder {
        HistogramBuilder::new()
    }
}

#[cfg(all(test, target_has_atomic = "64"))]
mod test {
    use super::*;

    macro_rules! assert_bucket_eq {
        ($h:expr, $bucket:expr, $val:expr) => {{
            assert_eq!($h.buckets[$bucket], $val);
        }};
    }

    #[test]
    fn log_scale_resolution_1() {
        let h = HistogramBuilder {
            scale: HistogramScale::Log,
            resolution: 1,
            num_buckets: 10,
        }
        .build();

        assert_eq!(h.bucket_range(0), 0..1);
        assert_eq!(h.bucket_range(1), 1..2);
        assert_eq!(h.bucket_range(2), 2..4);
        assert_eq!(h.bucket_range(3), 4..8);
        assert_eq!(h.bucket_range(9), 256..u64::MAX);

        let mut b = HistogramBatch::from_histogram(&h);

        b.measure(0, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 0);

        b.measure(1, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 0);

        b.measure(2, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 1);

        b.measure(3, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 2);

        b.measure(4, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 2);
        assert_bucket_eq!(b, 3, 1);

        b.measure(100, 1);
        assert_bucket_eq!(b, 7, 1);

        b.measure(128, 1);
        assert_bucket_eq!(b, 8, 1);

        b.measure(4096, 1);
        assert_bucket_eq!(b, 9, 1);
    }

    #[test]
    fn log_scale_resolution_2() {
        let h = HistogramBuilder {
            scale: HistogramScale::Log,
            resolution: 2,
            num_buckets: 10,
        }
        .build();

        assert_eq!(h.bucket_range(0), 0..2);
        assert_eq!(h.bucket_range(1), 2..4);
        assert_eq!(h.bucket_range(2), 4..8);
        assert_eq!(h.bucket_range(3), 8..16);
        assert_eq!(h.bucket_range(9), 512..u64::MAX);

        let mut b = HistogramBatch::from_histogram(&h);

        b.measure(0, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 0);

        b.measure(1, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 0);

        b.measure(2, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 0);

        b.measure(3, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 0);

        b.measure(4, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 1);

        b.measure(5, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 2);

        b.measure(6, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 3);

        b.measure(7, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 4);

        b.measure(8, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 4);
        assert_bucket_eq!(b, 3, 1);

        b.measure(100, 1);
        assert_bucket_eq!(b, 6, 1);

        b.measure(128, 1);
        assert_bucket_eq!(b, 7, 1);

        b.measure(4096, 1);
        assert_bucket_eq!(b, 9, 1);

        for bucket in h.buckets.iter() {
            assert_eq!(bucket.load(Relaxed), 0);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }
    }

    #[test]
    fn linear_scale_resolution_1() {
        let h = HistogramBuilder {
            scale: HistogramScale::Linear,
            resolution: 1,
            num_buckets: 10,
        }
        .build();

        assert_eq!(h.bucket_range(0), 0..1);
        assert_eq!(h.bucket_range(1), 1..2);
        assert_eq!(h.bucket_range(2), 2..3);
        assert_eq!(h.bucket_range(3), 3..4);
        assert_eq!(h.bucket_range(9), 9..u64::MAX);

        let mut b = HistogramBatch::from_histogram(&h);

        b.measure(0, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 0);

        b.measure(1, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 0);

        b.measure(2, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 1);
        assert_bucket_eq!(b, 3, 0);

        b.measure(3, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 1);
        assert_bucket_eq!(b, 3, 1);

        b.measure(5, 1);
        assert_bucket_eq!(b, 5, 1);

        b.measure(4096, 1);
        assert_bucket_eq!(b, 9, 1);

        for bucket in h.buckets.iter() {
            assert_eq!(bucket.load(Relaxed), 0);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }
    }

    #[test]
    fn linear_scale_resolution_100() {
        let h = HistogramBuilder {
            scale: HistogramScale::Linear,
            resolution: 100,
            num_buckets: 10,
        }
        .build();

        assert_eq!(h.bucket_range(0), 0..100);
        assert_eq!(h.bucket_range(1), 100..200);
        assert_eq!(h.bucket_range(2), 200..300);
        assert_eq!(h.bucket_range(3), 300..400);
        assert_eq!(h.bucket_range(9), 900..u64::MAX);

        let mut b = HistogramBatch::from_histogram(&h);

        b.measure(0, 1);
        assert_bucket_eq!(b, 0, 1);
        assert_bucket_eq!(b, 1, 0);

        b.measure(50, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 0);

        b.measure(100, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 1);
        assert_bucket_eq!(b, 2, 0);

        b.measure(101, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 0);

        b.measure(200, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 1);

        b.measure(299, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 2);

        b.measure(222, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 3);

        b.measure(300, 1);
        assert_bucket_eq!(b, 0, 2);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 3);
        assert_bucket_eq!(b, 3, 1);

        b.measure(888, 1);
        assert_bucket_eq!(b, 8, 1);

        b.measure(4096, 1);
        assert_bucket_eq!(b, 9, 1);

        for bucket in h.buckets.iter() {
            assert_eq!(bucket.load(Relaxed), 0);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }
    }

    #[test]
    fn inc_by_more_than_one() {
        let h = HistogramBuilder {
            scale: HistogramScale::Linear,
            resolution: 100,
            num_buckets: 10,
        }
        .build();

        let mut b = HistogramBatch::from_histogram(&h);

        b.measure(0, 3);
        assert_bucket_eq!(b, 0, 3);
        assert_bucket_eq!(b, 1, 0);

        b.measure(50, 5);
        assert_bucket_eq!(b, 0, 8);
        assert_bucket_eq!(b, 1, 0);

        b.measure(100, 2);
        assert_bucket_eq!(b, 0, 8);
        assert_bucket_eq!(b, 1, 2);
        assert_bucket_eq!(b, 2, 0);

        b.measure(101, 19);
        assert_bucket_eq!(b, 0, 8);
        assert_bucket_eq!(b, 1, 21);
        assert_bucket_eq!(b, 2, 0);

        for bucket in h.buckets.iter() {
            assert_eq!(bucket.load(Relaxed), 0);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }

        b.submit(&h);

        for i in 0..h.buckets.len() {
            assert_eq!(h.buckets[i].load(Relaxed), b.buckets[i]);
        }
    }
}
