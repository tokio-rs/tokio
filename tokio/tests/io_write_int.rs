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

use tokio::io::{AsyncWrite, AsyncWriteExt};

use std::io::{self, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};

mod support {
    pub mod io_coop;
}
use support::io_coop::ByteAtATimeWriter;

#[tokio::test]
async fn write_int_should_err_if_write_count_0() {
    struct Wr {}

    impl AsyncWrite for Wr {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Ok(0).into()
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Ok(()).into()
        }
    }

    let mut wr = Wr {};

    // should be ok just to test these 2, other cases actually expanded by same macro.
    assert!(wr.write_i8(0).await.is_err());
    assert!(wr.write_i32(12).await.is_err());
}

// Test numeric writes, Interrupted retries, and other I/O errors.
macro_rules! number {
    ($name:ident, $write:ident, $value:expr, $bytes:expr) => {
        #[tokio::test]
        async fn $name() {
            let bytes = $bytes;
            let mut b = tokio_test::io::Builder::new();
            b.write_error(ErrorKind::Interrupted.into())
                .write_error(ErrorKind::Interrupted.into());
            for byte in &bytes {
                b.write(&[*byte]).write_error(ErrorKind::Interrupted.into());
            }
            // The last injected error belongs to the next operation.
            let mut writer = b.build();
            // Arc::try_unwrap requires sole ownership; see tokio-test/src/io.rs:259.
            drop(b);
            writer.$write($value).await.unwrap();
            assert_eq!(
                writer.write(&[0]).await.unwrap_err().kind(),
                ErrorKind::Interrupted
            );
            let mut failed = tokio_test::io::Builder::new()
                .write_error(ErrorKind::Interrupted.into())
                .write_error(ErrorKind::PermissionDenied.into())
                .build();
            assert_eq!(
                failed.$write($value).await.unwrap_err().kind(),
                ErrorKind::PermissionDenied
            );
        }
    };
}

number!(u8, write_u8, 42u8, (42u8).to_be_bytes());
number!(i8, write_i8, 42i8, (42i8).to_be_bytes());
number!(u16, write_u16, 42u16, (42u16).to_be_bytes());
number!(u16_le, write_u16_le, 42u16, (42u16).to_le_bytes());
number!(i16, write_i16, 42i16, (42i16).to_be_bytes());
number!(i16_le, write_i16_le, 42i16, (42i16).to_le_bytes());
number!(u32, write_u32, 42u32, (42u32).to_be_bytes());
number!(u32_le, write_u32_le, 42u32, (42u32).to_le_bytes());
number!(i32, write_i32, 42i32, (42i32).to_be_bytes());
number!(i32_le, write_i32_le, 42i32, (42i32).to_le_bytes());
number!(u64, write_u64, 42u64, (42u64).to_be_bytes());
number!(u64_le, write_u64_le, 42u64, (42u64).to_le_bytes());
number!(i64, write_i64, 42i64, (42i64).to_be_bytes());
number!(i64_le, write_i64_le, 42i64, (42i64).to_le_bytes());
number!(u128, write_u128, 42u128, (42u128).to_be_bytes());
number!(i128, write_i128, 42i128, (42i128).to_be_bytes());
number!(f32, write_f32, 1.25f32, (1.25f32).to_be_bytes());
number!(f32_le, write_f32_le, 1.25f32, (1.25f32).to_le_bytes());
number!(f64, write_f64, 1.25f64, (1.25f64).to_be_bytes());
number!(f64_le, write_f64_le, 1.25f64, (1.25f64).to_le_bytes());

number!(u128_le, write_u128_le, 42u128, (42u128).to_le_bytes());
number!(i128_le, write_i128_le, -42i128, (-42i128).to_le_bytes());

// Test that repeated Interrupted errors make numeric writes yield.
macro_rules! cooperative_number {
    ($name:ident, $write:ident, $value:expr, $bytes:expr) => {
        #[tokio::test]
        async fn $name() {
            // Repeated Interrupted errors must yield before completing this number.
            let expected = $bytes;
            let mut writer = ByteAtATimeWriter {
                data: Vec::new(),
                interruptions_remaining: 256,
            };
            let mut operation = tokio_test::task::spawn(writer.$write($value));

            tokio_test::assert_pending!(operation.poll());
            operation.await.unwrap();
            assert_eq!(writer.data, expected);
        }
    };
}

cooperative_number!(
    u8_interrupted_is_cooperative,
    write_u8,
    42u8,
    (42u8).to_be_bytes()
);
cooperative_number!(
    i8_interrupted_is_cooperative,
    write_i8,
    42i8,
    (42i8).to_be_bytes()
);
cooperative_number!(
    u16_interrupted_is_cooperative,
    write_u16,
    42u16,
    (42u16).to_be_bytes()
);
cooperative_number!(
    u16_le_interrupted_is_cooperative,
    write_u16_le,
    42u16,
    (42u16).to_le_bytes()
);
cooperative_number!(
    i16_interrupted_is_cooperative,
    write_i16,
    42i16,
    (42i16).to_be_bytes()
);
cooperative_number!(
    i16_le_interrupted_is_cooperative,
    write_i16_le,
    42i16,
    (42i16).to_le_bytes()
);
cooperative_number!(
    u32_interrupted_is_cooperative,
    write_u32,
    42u32,
    (42u32).to_be_bytes()
);
cooperative_number!(
    u32_le_interrupted_is_cooperative,
    write_u32_le,
    42u32,
    (42u32).to_le_bytes()
);
cooperative_number!(
    i32_interrupted_is_cooperative,
    write_i32,
    42i32,
    (42i32).to_be_bytes()
);
cooperative_number!(
    i32_le_interrupted_is_cooperative,
    write_i32_le,
    42i32,
    (42i32).to_le_bytes()
);
cooperative_number!(
    u64_interrupted_is_cooperative,
    write_u64,
    42u64,
    (42u64).to_be_bytes()
);
cooperative_number!(
    u64_le_interrupted_is_cooperative,
    write_u64_le,
    42u64,
    (42u64).to_le_bytes()
);
cooperative_number!(
    i64_interrupted_is_cooperative,
    write_i64,
    42i64,
    (42i64).to_be_bytes()
);
cooperative_number!(
    i64_le_interrupted_is_cooperative,
    write_i64_le,
    42i64,
    (42i64).to_le_bytes()
);
cooperative_number!(
    u128_interrupted_is_cooperative,
    write_u128,
    42u128,
    (42u128).to_be_bytes()
);
cooperative_number!(
    i128_interrupted_is_cooperative,
    write_i128,
    42i128,
    (42i128).to_be_bytes()
);
cooperative_number!(
    f32_interrupted_is_cooperative,
    write_f32,
    1.25f32,
    (1.25f32).to_be_bytes()
);
cooperative_number!(
    f32_le_interrupted_is_cooperative,
    write_f32_le,
    1.25f32,
    (1.25f32).to_le_bytes()
);
cooperative_number!(
    f64_interrupted_is_cooperative,
    write_f64,
    1.25f64,
    (1.25f64).to_be_bytes()
);
cooperative_number!(
    f64_le_interrupted_is_cooperative,
    write_f64_le,
    1.25f64,
    (1.25f64).to_le_bytes()
);
cooperative_number!(
    u128_le_interrupted_is_cooperative,
    write_u128_le,
    42u128,
    (42u128).to_le_bytes()
);
cooperative_number!(
    i128_le_interrupted_is_cooperative,
    write_i128_le,
    -42i128,
    (-42i128).to_le_bytes()
);

// Test that numeric writes preserve partial progress across a coop yield.
macro_rules! cooperative_partial_number {
    ($name:ident, $write:ident, $value:expr, $bytes:expr) => {
        #[tokio::test]
        async fn $name() {
            let expected = $bytes;
            // Keep the first byte across the yield caused by Interrupted retries.
            let mut writer = {
                let mut builder = tokio_test::io::Builder::new();
                builder.write(&expected[..1]);
                for _ in 0..256 {
                    builder.write_error(ErrorKind::Interrupted.into());
                }
                builder.write(&expected[1..]).build()
            };
            let mut operation = tokio_test::task::spawn(writer.$write($value));

            tokio_test::assert_pending!(operation.poll());
            operation.await.unwrap();
        }
    };
}

cooperative_partial_number!(
    u16_interrupted_after_partial_write_is_cooperative,
    write_u16,
    42u16,
    (42u16).to_be_bytes()
);
cooperative_partial_number!(
    u16_le_interrupted_after_partial_write_is_cooperative,
    write_u16_le,
    42u16,
    (42u16).to_le_bytes()
);
cooperative_partial_number!(
    i16_interrupted_after_partial_write_is_cooperative,
    write_i16,
    42i16,
    (42i16).to_be_bytes()
);
cooperative_partial_number!(
    i16_le_interrupted_after_partial_write_is_cooperative,
    write_i16_le,
    42i16,
    (42i16).to_le_bytes()
);
cooperative_partial_number!(
    u32_interrupted_after_partial_write_is_cooperative,
    write_u32,
    42u32,
    (42u32).to_be_bytes()
);
cooperative_partial_number!(
    u32_le_interrupted_after_partial_write_is_cooperative,
    write_u32_le,
    42u32,
    (42u32).to_le_bytes()
);
cooperative_partial_number!(
    i32_interrupted_after_partial_write_is_cooperative,
    write_i32,
    42i32,
    (42i32).to_be_bytes()
);
cooperative_partial_number!(
    i32_le_interrupted_after_partial_write_is_cooperative,
    write_i32_le,
    42i32,
    (42i32).to_le_bytes()
);
cooperative_partial_number!(
    u64_interrupted_after_partial_write_is_cooperative,
    write_u64,
    42u64,
    (42u64).to_be_bytes()
);
cooperative_partial_number!(
    u64_le_interrupted_after_partial_write_is_cooperative,
    write_u64_le,
    42u64,
    (42u64).to_le_bytes()
);
cooperative_partial_number!(
    i64_interrupted_after_partial_write_is_cooperative,
    write_i64,
    42i64,
    (42i64).to_be_bytes()
);
cooperative_partial_number!(
    i64_le_interrupted_after_partial_write_is_cooperative,
    write_i64_le,
    42i64,
    (42i64).to_le_bytes()
);
cooperative_partial_number!(
    u128_interrupted_after_partial_write_is_cooperative,
    write_u128,
    42u128,
    (42u128).to_be_bytes()
);
cooperative_partial_number!(
    i128_interrupted_after_partial_write_is_cooperative,
    write_i128,
    42i128,
    (42i128).to_be_bytes()
);
cooperative_partial_number!(
    f32_interrupted_after_partial_write_is_cooperative,
    write_f32,
    1.25f32,
    (1.25f32).to_be_bytes()
);
cooperative_partial_number!(
    f32_le_interrupted_after_partial_write_is_cooperative,
    write_f32_le,
    1.25f32,
    (1.25f32).to_le_bytes()
);
cooperative_partial_number!(
    f64_interrupted_after_partial_write_is_cooperative,
    write_f64,
    1.25f64,
    (1.25f64).to_be_bytes()
);
cooperative_partial_number!(
    f64_le_interrupted_after_partial_write_is_cooperative,
    write_f64_le,
    1.25f64,
    (1.25f64).to_le_bytes()
);
cooperative_partial_number!(
    u128_le_interrupted_after_partial_write_is_cooperative,
    write_u128_le,
    42u128,
    (42u128).to_le_bytes()
);
cooperative_partial_number!(
    i128_le_interrupted_after_partial_write_is_cooperative,
    write_i128_le,
    -42i128,
    (-42i128).to_le_bytes()
);
