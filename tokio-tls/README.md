# tokio-tls

An implementation of TLS/SSL streams for Tokio built on top of the [`native-tls`
crate]

[Documentation](https://docs.rs/tokio-tls)

[`native-tls` crate]: https://github.com/sfackler/rust-native-tls

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
native-tls = "0.2"
tokio-tls = "0.2"
```

Next, add this to your crate:

```rust
extern crate native_tls;
extern crate tokio_tls;

use tokio_tls::{TlsConnector, TlsAcceptor};
```

You can find few examples how to use this crate in examples directory (using TLS in
hyper server or client).

By default the `native-tls` crate currently uses the "platform appropriate"
backend for a TLS implementation. This means:

* On Windows, [SChannel] is used
* On OSX, [SecureTransport] is used
* Everywhere else, [OpenSSL] is used

[SChannel]: https://msdn.microsoft.com/en-us/library/windows/desktop/aa380123%28v=vs.85%29.aspx?f=255&MSPPError=-2147217396
[SecureTransport]: https://developer.apple.com/reference/security/1654508-secure_transport
[OpenSSL]: https://www.openssl.org/

Typically these selections mean that you don't have to worry about a portability
when using TLS, these libraries are all normally installed by default.

## License

This project is licensed under the [MIT license](./LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
