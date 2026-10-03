#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "macros",
        feature = "io-util"
    )
))]

use std::io::ErrorKind;
use tokio::io::AsyncReadExt;

mod support {
    pub mod io_coop;
}
use support::io_coop::ByteAtATimeReader;

// Test numeric reads, Interrupted retries, and other I/O errors.
macro_rules! number {
    ($name:ident, $read:ident, $value:expr, $bytes:expr) => {
        #[tokio::test]
        async fn $name() {
            let bytes = $bytes;
            let mut b = tokio_test::io::Builder::new();
            b.read_error(ErrorKind::Interrupted.into())
                .read_error(ErrorKind::Interrupted.into());
            for byte in &bytes {
                b.read(&[*byte]).read_error(ErrorKind::Interrupted.into());
            }
            // The last injected error belongs to the next operation.
            let mut reader = b.build();
            // Arc::try_unwrap requires sole ownership; see tokio-test/src/io.rs
            drop(b);
            assert_eq!(reader.$read().await.unwrap(), $value);
            assert_eq!(
                reader.read(&mut [0]).await.unwrap_err().kind(),
                ErrorKind::Interrupted
            );

            let mut short = tokio_test::io::Builder::new()
                .read_error(ErrorKind::Interrupted.into())
                .build();
            assert_eq!(
                short.$read().await.unwrap_err().kind(),
                ErrorKind::UnexpectedEof
            );
            let mut failed = tokio_test::io::Builder::new()
                .read_error(ErrorKind::Interrupted.into())
                .read_error(ErrorKind::PermissionDenied.into())
                .build();
            assert_eq!(
                failed.$read().await.unwrap_err().kind(),
                ErrorKind::PermissionDenied
            );
        }
    };
}

number!(u8, read_u8, 42u8, (42u8).to_be_bytes());
number!(i8, read_i8, 42i8, (42i8).to_be_bytes());
number!(u16, read_u16, 42u16, (42u16).to_be_bytes());
number!(u16_le, read_u16_le, 42u16, (42u16).to_le_bytes());
number!(i16, read_i16, 42i16, (42i16).to_be_bytes());
number!(i16_le, read_i16_le, 42i16, (42i16).to_le_bytes());
number!(u32, read_u32, 42u32, (42u32).to_be_bytes());
number!(u32_le, read_u32_le, 42u32, (42u32).to_le_bytes());
number!(i32, read_i32, 42i32, (42i32).to_be_bytes());
number!(i32_le, read_i32_le, 42i32, (42i32).to_le_bytes());
number!(u64, read_u64, 42u64, (42u64).to_be_bytes());
number!(u64_le, read_u64_le, 42u64, (42u64).to_le_bytes());
number!(i64, read_i64, 42i64, (42i64).to_be_bytes());
number!(i64_le, read_i64_le, 42i64, (42i64).to_le_bytes());
number!(u128, read_u128, 42u128, (42u128).to_be_bytes());
number!(i128, read_i128, 42i128, (42i128).to_be_bytes());
number!(f32, read_f32, 1.25f32, (1.25f32).to_be_bytes());
number!(f32_le, read_f32_le, 1.25f32, (1.25f32).to_le_bytes());
number!(f64, read_f64, 1.25f64, (1.25f64).to_be_bytes());
number!(f64_le, read_f64_le, 1.25f64, (1.25f64).to_le_bytes());

number!(u128_le, read_u128_le, 42u128, (42u128).to_le_bytes());
number!(i128_le, read_i128_le, -42i128, (-42i128).to_le_bytes());

// Test that repeated Interrupted errors make numeric reads yield.
macro_rules! cooperative_number {
    ($name:ident, $read:ident, $value:expr, $bytes:expr) => {
        #[tokio::test]
        async fn $name() {
            // Repeated Interrupted errors must yield before completing this number.
            let expected = $bytes;
            let mut reader = ByteAtATimeReader {
                data: &expected,
                interruptions_remaining: 256,
            };
            let mut operation = tokio_test::task::spawn(reader.$read());

            tokio_test::assert_pending!(operation.poll());
            assert_eq!(operation.await.unwrap(), $value);
        }
    };
}

cooperative_number!(
    u8_interrupted_is_cooperative,
    read_u8,
    42u8,
    (42u8).to_be_bytes()
);
cooperative_number!(
    i8_interrupted_is_cooperative,
    read_i8,
    42i8,
    (42i8).to_be_bytes()
);
cooperative_number!(
    u16_interrupted_is_cooperative,
    read_u16,
    42u16,
    (42u16).to_be_bytes()
);
cooperative_number!(
    u16_le_interrupted_is_cooperative,
    read_u16_le,
    42u16,
    (42u16).to_le_bytes()
);
cooperative_number!(
    i16_interrupted_is_cooperative,
    read_i16,
    42i16,
    (42i16).to_be_bytes()
);
cooperative_number!(
    i16_le_interrupted_is_cooperative,
    read_i16_le,
    42i16,
    (42i16).to_le_bytes()
);
cooperative_number!(
    u32_interrupted_is_cooperative,
    read_u32,
    42u32,
    (42u32).to_be_bytes()
);
cooperative_number!(
    u32_le_interrupted_is_cooperative,
    read_u32_le,
    42u32,
    (42u32).to_le_bytes()
);
cooperative_number!(
    i32_interrupted_is_cooperative,
    read_i32,
    42i32,
    (42i32).to_be_bytes()
);
cooperative_number!(
    i32_le_interrupted_is_cooperative,
    read_i32_le,
    42i32,
    (42i32).to_le_bytes()
);
cooperative_number!(
    u64_interrupted_is_cooperative,
    read_u64,
    42u64,
    (42u64).to_be_bytes()
);
cooperative_number!(
    u64_le_interrupted_is_cooperative,
    read_u64_le,
    42u64,
    (42u64).to_le_bytes()
);
cooperative_number!(
    i64_interrupted_is_cooperative,
    read_i64,
    42i64,
    (42i64).to_be_bytes()
);
cooperative_number!(
    i64_le_interrupted_is_cooperative,
    read_i64_le,
    42i64,
    (42i64).to_le_bytes()
);
cooperative_number!(
    u128_interrupted_is_cooperative,
    read_u128,
    42u128,
    (42u128).to_be_bytes()
);
cooperative_number!(
    i128_interrupted_is_cooperative,
    read_i128,
    42i128,
    (42i128).to_be_bytes()
);
cooperative_number!(
    f32_interrupted_is_cooperative,
    read_f32,
    1.25f32,
    (1.25f32).to_be_bytes()
);
cooperative_number!(
    f32_le_interrupted_is_cooperative,
    read_f32_le,
    1.25f32,
    (1.25f32).to_le_bytes()
);
cooperative_number!(
    f64_interrupted_is_cooperative,
    read_f64,
    1.25f64,
    (1.25f64).to_be_bytes()
);
cooperative_number!(
    f64_le_interrupted_is_cooperative,
    read_f64_le,
    1.25f64,
    (1.25f64).to_le_bytes()
);
cooperative_number!(
    u128_le_interrupted_is_cooperative,
    read_u128_le,
    42u128,
    (42u128).to_le_bytes()
);
cooperative_number!(
    i128_le_interrupted_is_cooperative,
    read_i128_le,
    -42i128,
    (-42i128).to_le_bytes()
);

// Test that numeric reads preserve partial progress across a coop yield.
macro_rules! cooperative_partial_number {
    ($name:ident, $read:ident, $value:expr, $bytes:expr) => {
        #[tokio::test]
        async fn $name() {
            let expected = $bytes;
            // Keep the first byte across the yield caused by Interrupted retries.
            let mut reader = (&expected[..1]).chain(ByteAtATimeReader {
                data: &expected[1..],
                interruptions_remaining: 256,
            });
            let mut operation = tokio_test::task::spawn(reader.$read());

            tokio_test::assert_pending!(operation.poll());
            assert_eq!(operation.await.unwrap(), $value);
        }
    };
}

cooperative_partial_number!(
    u16_interrupted_after_partial_read_is_cooperative,
    read_u16,
    42u16,
    (42u16).to_be_bytes()
);
cooperative_partial_number!(
    u16_le_interrupted_after_partial_read_is_cooperative,
    read_u16_le,
    42u16,
    (42u16).to_le_bytes()
);
cooperative_partial_number!(
    i16_interrupted_after_partial_read_is_cooperative,
    read_i16,
    42i16,
    (42i16).to_be_bytes()
);
cooperative_partial_number!(
    i16_le_interrupted_after_partial_read_is_cooperative,
    read_i16_le,
    42i16,
    (42i16).to_le_bytes()
);
cooperative_partial_number!(
    u32_interrupted_after_partial_read_is_cooperative,
    read_u32,
    42u32,
    (42u32).to_be_bytes()
);
cooperative_partial_number!(
    u32_le_interrupted_after_partial_read_is_cooperative,
    read_u32_le,
    42u32,
    (42u32).to_le_bytes()
);
cooperative_partial_number!(
    i32_interrupted_after_partial_read_is_cooperative,
    read_i32,
    42i32,
    (42i32).to_be_bytes()
);
cooperative_partial_number!(
    i32_le_interrupted_after_partial_read_is_cooperative,
    read_i32_le,
    42i32,
    (42i32).to_le_bytes()
);
cooperative_partial_number!(
    u64_interrupted_after_partial_read_is_cooperative,
    read_u64,
    42u64,
    (42u64).to_be_bytes()
);
cooperative_partial_number!(
    u64_le_interrupted_after_partial_read_is_cooperative,
    read_u64_le,
    42u64,
    (42u64).to_le_bytes()
);
cooperative_partial_number!(
    i64_interrupted_after_partial_read_is_cooperative,
    read_i64,
    42i64,
    (42i64).to_be_bytes()
);
cooperative_partial_number!(
    i64_le_interrupted_after_partial_read_is_cooperative,
    read_i64_le,
    42i64,
    (42i64).to_le_bytes()
);
cooperative_partial_number!(
    u128_interrupted_after_partial_read_is_cooperative,
    read_u128,
    42u128,
    (42u128).to_be_bytes()
);
cooperative_partial_number!(
    i128_interrupted_after_partial_read_is_cooperative,
    read_i128,
    42i128,
    (42i128).to_be_bytes()
);
cooperative_partial_number!(
    f32_interrupted_after_partial_read_is_cooperative,
    read_f32,
    1.25f32,
    (1.25f32).to_be_bytes()
);
cooperative_partial_number!(
    f32_le_interrupted_after_partial_read_is_cooperative,
    read_f32_le,
    1.25f32,
    (1.25f32).to_le_bytes()
);
cooperative_partial_number!(
    f64_interrupted_after_partial_read_is_cooperative,
    read_f64,
    1.25f64,
    (1.25f64).to_be_bytes()
);
cooperative_partial_number!(
    f64_le_interrupted_after_partial_read_is_cooperative,
    read_f64_le,
    1.25f64,
    (1.25f64).to_le_bytes()
);
cooperative_partial_number!(
    u128_le_interrupted_after_partial_read_is_cooperative,
    read_u128_le,
    42u128,
    (42u128).to_le_bytes()
);
cooperative_partial_number!(
    i128_le_interrupted_after_partial_read_is_cooperative,
    read_i128_le,
    -42i128,
    (-42i128).to_le_bytes()
);
