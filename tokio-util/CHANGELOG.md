# 0.3.0 (February 28, 2020)

Breaking changes:

- Change codec::Encoder trait to take a generic Item parameter (#1746), which allows
  codec writers to pass references into `Framed` and `FramedWrite` types.

Other additions:

- Add futures-io/tokio::io compatibility layer (#2117)

# 0.2.0 (November 26, 2019)

- Initial release
