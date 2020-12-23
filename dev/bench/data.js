window.BENCHMARK_DATA = {
  "lastUpdate": 1608743835495,
  "repoUrl": "https://github.com/tokio-rs/tokio",
  "entries": {
    "sync_rwlock": [
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "distinct": true,
          "id": "af032dbf59195f5a637c14fd8805f45cce8c8563",
          "message": "try again",
          "timestamp": "2020-11-13T16:47:49-08:00",
          "tree_id": "2c351a9bf2bc6d1fb70754ee19640da3b69df204",
          "url": "https://github.com/tokio-rs/tokio/commit/af032dbf59195f5a637c14fd8805f45cce8c8563"
        },
        "date": 1605314959564,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1019,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14559,
            "range": "± 3929",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1045,
            "range": "± 69",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14917,
            "range": "± 3866",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 589,
            "range": "± 34",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "97c2c4203cd7c42960cac895987c43a17dff052e",
          "message": "chore: automate running benchmarks (#3140)\n\nUses Github actions to run benchmarks.",
          "timestamp": "2020-11-13T19:30:52-08:00",
          "tree_id": "f4a3cfebafb7afee68d6d4de1748daddcfc070c6",
          "url": "https://github.com/tokio-rs/tokio/commit/97c2c4203cd7c42960cac895987c43a17dff052e"
        },
        "date": 1605324749506,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 924,
            "range": "± 177",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15161,
            "range": "± 4086",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 874,
            "range": "± 176",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14986,
            "range": "± 4366",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 493,
            "range": "± 85",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "masnagam@gmail.com",
            "name": "masnagam",
            "username": "masnagam"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4e39c9b818eb8af064bb9f45f47e3cfc6593de95",
          "message": "net: restore TcpStream::{poll_read_ready, poll_write_ready} (#2743)",
          "timestamp": "2020-11-16T09:51:06-08:00",
          "tree_id": "2222dd2f8638fb64f228badef84814d2f4079a82",
          "url": "https://github.com/tokio-rs/tokio/commit/4e39c9b818eb8af064bb9f45f47e3cfc6593de95"
        },
        "date": 1605549190558,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1052,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 16954,
            "range": "± 8337",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1096,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14894,
            "range": "± 2274",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 611,
            "range": "± 25",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f5cb4c20422a35b51bfba3391744f8bcb54f7581",
          "message": "net: Add send/recv buf size methods to `TcpSocket` (#3145)\n\nThis commit adds `set_{send, recv}_buffer_size` methods to `TcpSocket`\r\nfor setting the size of the TCP send and receive buffers, and `{send,\r\nrecv}_buffer_size` methods for returning the current value. These just\r\ncall into similar methods on `mio`'s `TcpSocket` type, which were added\r\nin tokio-rs/mio#1384.\r\n\r\nRefs: #3082\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-11-16T12:29:03-08:00",
          "tree_id": "fcf642984a21d04533efad0cdde613d294635c4d",
          "url": "https://github.com/tokio-rs/tokio/commit/f5cb4c20422a35b51bfba3391744f8bcb54f7581"
        },
        "date": 1605558676197,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1058,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15094,
            "range": "± 3743",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1022,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14201,
            "range": "± 3791",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 545,
            "range": "± 77",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "zaharidichev@gmail.com",
            "name": "Zahari Dichev",
            "username": "zaharidichev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d0ebb4154748166a4ba07baa4b424a1c45efd219",
          "message": "sync: add `Notify::notify_waiters` (#3098)\n\nThis PR makes `Notify::notify_waiters` public. The method\r\nalready exists, but it changes the way `notify_waiters`,\r\nis used. Previously in order for the consumer to\r\nregister interest, in a notification triggered by\r\n`notify_waiters`, the `Notified` future had to be\r\npolled. This introduced friction when using the api\r\nas the future had to be pinned before polled.\r\n\r\nThis change introduces a counter that tracks how many\r\ntimes `notified_waiters` has been called. Upon creation of\r\nthe future the number of times is loaded. When first\r\npolled the future compares this number with the count\r\nstate of the `Notify` type. This avoids the need for\r\nregistering the waiter upfront.\r\n\r\nFixes: #3066",
          "timestamp": "2020-11-16T12:49:35-08:00",
          "tree_id": "5ea4d611256290f62baea1a9ffa3333b254181df",
          "url": "https://github.com/tokio-rs/tokio/commit/d0ebb4154748166a4ba07baa4b424a1c45efd219"
        },
        "date": 1605559893134,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 981,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15833,
            "range": "± 8715",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1067,
            "range": "± 210",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 16713,
            "range": "± 13609",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 568,
            "range": "± 131",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0ea23076503c5151d68a781a3d91823396c82949",
          "message": "net: add UdpSocket readiness and non-blocking ops (#3138)\n\nAdds `ready()`, `readable()`, and `writable()` async methods for waiting\r\nfor socket readiness. Adds `try_send`, `try_send_to`, `try_recv`, and\r\n`try_recv_from` for performing non-blocking operations on the socket.\r\n\r\nThis is the UDP equivalent of #3130.",
          "timestamp": "2020-11-16T15:44:01-08:00",
          "tree_id": "1e49d7dc0bb3cee6271133d942ba49c5971fde29",
          "url": "https://github.com/tokio-rs/tokio/commit/0ea23076503c5151d68a781a3d91823396c82949"
        },
        "date": 1605570348676,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1005,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13752,
            "range": "± 3698",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 927,
            "range": "± 136",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13180,
            "range": "± 3450",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 541,
            "range": "± 63",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "9832640+zekisherif@users.noreply.github.com",
            "name": "Zeki Sherif",
            "username": "zekisherif"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d11aa866837eea50a6f1e0ef7e24846a653cbf1",
          "message": "net: add SO_LINGER get/set to TcpStream (#3143)",
          "timestamp": "2020-11-17T09:58:00-08:00",
          "tree_id": "ca0d5edc04a29bbe6e2906c760a22908e032a4c9",
          "url": "https://github.com/tokio-rs/tokio/commit/7d11aa866837eea50a6f1e0ef7e24846a653cbf1"
        },
        "date": 1605636002716,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 905,
            "range": "± 113",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15309,
            "range": "± 5774",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 954,
            "range": "± 138",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15280,
            "range": "± 6096",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 523,
            "range": "± 80",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "34fcef258b84d17f8d418b39eb61fa07fa87c390",
          "message": "io: add vectored writes to `AsyncWrite` (#3149)\n\nThis adds `AsyncWrite::poll_write_vectored`, and implements it for\r\n`TcpStream` and `UnixStream`.\r\n\r\nRefs: #3135.",
          "timestamp": "2020-11-18T10:41:47-08:00",
          "tree_id": "98e37fc2d6fa541a9e499331df86ba3d1b7b6e3a",
          "url": "https://github.com/tokio-rs/tokio/commit/34fcef258b84d17f8d418b39eb61fa07fa87c390"
        },
        "date": 1605725012703,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 913,
            "range": "± 127",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14214,
            "range": "± 4731",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 922,
            "range": "± 161",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15159,
            "range": "± 5870",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 487,
            "range": "± 100",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "479c545c20b2cb44a8f09600733adc8c8dcb5aa0",
          "message": "chore: prepare v0.3.4 release (#3152)",
          "timestamp": "2020-11-18T12:38:13-08:00",
          "tree_id": "df6daba6b2f595de47ada2dd2f518475669ab919",
          "url": "https://github.com/tokio-rs/tokio/commit/479c545c20b2cb44a8f09600733adc8c8dcb5aa0"
        },
        "date": 1605731980234,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 751,
            "range": "± 86",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12495,
            "range": "± 3795",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 821,
            "range": "± 170",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12921,
            "range": "± 4051",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 471,
            "range": "± 120",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84",
          "message": "util: fix typo in udp/frame.rs (#3154)",
          "timestamp": "2020-11-20T15:06:14+09:00",
          "tree_id": "f09954c70e26336bdb1bc525f832916c2d7037bf",
          "url": "https://github.com/tokio-rs/tokio/commit/49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84"
        },
        "date": 1605852495335,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1026,
            "range": "± 197",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 17561,
            "range": "± 5098",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1069,
            "range": "± 373",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 16640,
            "range": "± 8681",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 554,
            "range": "± 107",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f927f01a34d7cedf0cdc820f729a7a6cd56e83dd",
          "message": "macros: fix rustfmt on 1.48.0 (#3160)\n\n## Motivation\r\n\r\nLooks like the Rust 1.48.0 version of `rustfmt` changed some formatting\r\nrules (fixed some bugs?), and some of the code in `tokio-macros` is no\r\nlonger correctly formatted. This is breaking CI.\r\n\r\n## Solution\r\n\r\nThis commit runs rustfmt on Rust 1.48.0. This fixes CI.\r\n\r\nCloses #3158",
          "timestamp": "2020-11-20T10:19:26-08:00",
          "tree_id": "bd0243a653ee49cfc50bf61b00a36cc0fce6a414",
          "url": "https://github.com/tokio-rs/tokio/commit/f927f01a34d7cedf0cdc820f729a7a6cd56e83dd"
        },
        "date": 1605896461076,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 851,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13799,
            "range": "± 2696",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 869,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14280,
            "range": "± 5421",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 493,
            "range": "± 69",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ae67851f11b7cc1f577de8ce21767ce3e2c7aff9",
          "message": "time: use intrusive lists for timer tracking (#3080)\n\nMore-or-less a half-rewrite of the current time driver, supporting the\r\nuse of intrusive futures for timer registration.\r\n\r\nFixes: #3028, #3069",
          "timestamp": "2020-11-23T10:42:50-08:00",
          "tree_id": "be43cb76333b0e9e42a101d659f9b2e41555d779",
          "url": "https://github.com/tokio-rs/tokio/commit/ae67851f11b7cc1f577de8ce21767ce3e2c7aff9"
        },
        "date": 1606157082296,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1046,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 16129,
            "range": "± 5605",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1047,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 16263,
            "range": "± 5265",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 596,
            "range": "± 88",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "driftluo@foxmail.com",
            "name": "漂流",
            "username": "driftluo"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "874fc3320bc000fee20d63b3ad865a1145122640",
          "message": "codec: add read_buffer_mut to FramedRead (#3166)",
          "timestamp": "2020-11-24T09:39:16+01:00",
          "tree_id": "53540b744f6a915cedc1099afe1b0639443b2436",
          "url": "https://github.com/tokio-rs/tokio/commit/874fc3320bc000fee20d63b3ad865a1145122640"
        },
        "date": 1606207265360,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 978,
            "range": "± 169",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 16567,
            "range": "± 9827",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1027,
            "range": "± 238",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15815,
            "range": "± 6190",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 564,
            "range": "± 138",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "de33ee85ce61377b316b630e4355d419cc4abcb7",
          "message": "time: replace 'ouClockTimeide' in internal docs with 'outside' (#3171)",
          "timestamp": "2020-11-24T10:23:20+01:00",
          "tree_id": "5ed85f95ea1846983471a11fe555328e6b0f5f6f",
          "url": "https://github.com/tokio-rs/tokio/commit/de33ee85ce61377b316b630e4355d419cc4abcb7"
        },
        "date": 1606209917511,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1007,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14652,
            "range": "± 3659",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1008,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14781,
            "range": "± 3706",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 584,
            "range": "± 41",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rajiv.chauhan@gmail.com",
            "name": "Rajiv Chauhan",
            "username": "chauhraj"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e406a7a47699d93fa2a77fb72553600cb7abd0f",
          "message": "macros: fix outdated documentation (#3180)\n\n1. Changed 0.2 to 0.3\r\n2. Changed ‘multi’ to ‘single’ to indicate that the behavior is single threaded",
          "timestamp": "2020-11-26T19:46:15+01:00",
          "tree_id": "ac6898684e4b84e4a5d0e781adf42d950bbc9e43",
          "url": "https://github.com/tokio-rs/tokio/commit/5e406a7a47699d93fa2a77fb72553600cb7abd0f"
        },
        "date": 1606416464334,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 840,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13525,
            "range": "± 4041",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 909,
            "range": "± 280",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12807,
            "range": "± 2584",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 511,
            "range": "± 100",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "niklas.fiekas@backscattering.de",
            "name": "Niklas Fiekas",
            "username": "niklasf"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49129434198a96444bc0e9582a14062d3a46e93a",
          "message": "signal: expose CtrlC stream on windows (#3186)\n\n* Make tokio::signal::windows::ctrl_c() public.\r\n* Stop referring to private tokio::signal::windows::Event in module\r\n  documentation.\r\n\r\nCloses #3178",
          "timestamp": "2020-11-27T19:53:17Z",
          "tree_id": "904fb6b1fb539bffe69168c7202ccc3db15321dc",
          "url": "https://github.com/tokio-rs/tokio/commit/49129434198a96444bc0e9582a14062d3a46e93a"
        },
        "date": 1606506890215,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1011,
            "range": "± 164",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14652,
            "range": "± 3856",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1040,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14329,
            "range": "± 3298",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 590,
            "range": "± 33",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0acd06b42a9d1461302388f2a533e86d391d6040",
          "message": "runtime: fix shutdown_timeout(0) blocking (#3174)",
          "timestamp": "2020-11-28T19:31:13+01:00",
          "tree_id": "c17e5d58e10ee419e492cb831843c3f08e1f66d8",
          "url": "https://github.com/tokio-rs/tokio/commit/0acd06b42a9d1461302388f2a533e86d391d6040"
        },
        "date": 1606588372516,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1009,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15584,
            "range": "± 5832",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1051,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14737,
            "range": "± 5383",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 590,
            "range": "± 17",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c55d846f4b248b4a72335d6c57829fa6396ab9a5",
          "message": "util: add rt to tokio-util full feature (#3194)",
          "timestamp": "2020-11-29T09:48:31+01:00",
          "tree_id": "5f27b29cd1018796f0713d6e87e4823920ba5084",
          "url": "https://github.com/tokio-rs/tokio/commit/c55d846f4b248b4a72335d6c57829fa6396ab9a5"
        },
        "date": 1606639807389,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 808,
            "range": "± 137",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13280,
            "range": "± 5149",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 829,
            "range": "± 104",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13293,
            "range": "± 2472",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 499,
            "range": "± 84",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kylekosic@gmail.com",
            "name": "Kyle Kosic",
            "username": "kykosic"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a85fdb884d961bb87a2f3d446c548802868e54cb",
          "message": "runtime: test for shutdown_timeout(0) (#3196)",
          "timestamp": "2020-11-29T21:30:19+01:00",
          "tree_id": "c554597c6596dc6eddc98bfafcc512361ddb5f31",
          "url": "https://github.com/tokio-rs/tokio/commit/a85fdb884d961bb87a2f3d446c548802868e54cb"
        },
        "date": 1606681904630,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 776,
            "range": "± 191",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13802,
            "range": "± 4012",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 873,
            "range": "± 283",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13338,
            "range": "± 3371",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 543,
            "range": "± 91",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pickfire@riseup.net",
            "name": "Ivan Tham",
            "username": "pickfire"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "72d6346c0d43d867002dc0cc5527fbd0b0e23c3f",
          "message": "macros: #[tokio::main] can be used on non-main (#3199)",
          "timestamp": "2020-11-30T17:34:11+01:00",
          "tree_id": "c558d1cb380cc67bfc56ea960a7d9e266259367a",
          "url": "https://github.com/tokio-rs/tokio/commit/72d6346c0d43d867002dc0cc5527fbd0b0e23c3f"
        },
        "date": 1606754147760,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1009,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14631,
            "range": "± 5838",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1052,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14637,
            "range": "± 4532",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 594,
            "range": "± 29",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "73653352+HK416-is-all-you-need@users.noreply.github.com",
            "name": "HK416-is-all-you-need",
            "username": "HK416-is-all-you-need"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5",
          "message": "io: add AsyncFd::with_interest (#3167)\n\nFixes #3072",
          "timestamp": "2020-11-30T11:11:18-08:00",
          "tree_id": "45e9d190af02ab0cdc92c317e3127a1b8227ac3a",
          "url": "https://github.com/tokio-rs/tokio/commit/7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5"
        },
        "date": 1606763576920,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 912,
            "range": "± 245",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 20691,
            "range": "± 6127",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 940,
            "range": "± 224",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 20672,
            "range": "± 6183",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 517,
            "range": "± 131",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "08548583b948a0be04338f1b1462917c001dbf4a",
          "message": "chore: prepare v0.3.5 release (#3201)",
          "timestamp": "2020-11-30T12:57:31-08:00",
          "tree_id": "bc964338ba8d03930d53192a1e2288132330ff97",
          "url": "https://github.com/tokio-rs/tokio/commit/08548583b948a0be04338f1b1462917c001dbf4a"
        },
        "date": 1606770000167,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1013,
            "range": "± 116",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15359,
            "range": "± 5069",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1045,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14598,
            "range": "± 6433",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 590,
            "range": "± 11",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "128495168d390092df2cb8ae8577cfec09f666ff",
          "message": "ci: switch FreeBSD CI environment to 12.2-RELEASE (#3202)\n\n12.1 will be EoL in two months.",
          "timestamp": "2020-12-01T10:19:54+09:00",
          "tree_id": "2a289d5667b3ffca2ebfb747785c380ee7eac034",
          "url": "https://github.com/tokio-rs/tokio/commit/128495168d390092df2cb8ae8577cfec09f666ff"
        },
        "date": 1606785704729,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1091,
            "range": "± 113",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15313,
            "range": "± 5620",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1067,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 16540,
            "range": "± 7543",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 607,
            "range": "± 67",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "353b0544a04214e7d6e828641e2045df1d97cda8",
          "message": "ci: reenable CI on FreeBSD i686 (#3204)\n\nIt was temporarily disabled in 06c473e62842d257ed275497ce906710ea3f8e19\r\nand never reenabled.",
          "timestamp": "2020-12-01T10:20:18+09:00",
          "tree_id": "468f282ba9f5116f5ed9a81abacbb7385aaa9c1e",
          "url": "https://github.com/tokio-rs/tokio/commit/353b0544a04214e7d6e828641e2045df1d97cda8"
        },
        "date": 1606785734965,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1010,
            "range": "± 209",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 22260,
            "range": "± 14628",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 991,
            "range": "± 248",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 21596,
            "range": "± 9496",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 555,
            "range": "± 101",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ae8135b62057be6b1691f04b27eabe285b05efd",
          "message": "process: fix the process_kill_on_drop.rs test on non-Linux systems (#3203)\n\n\"disown\" is a bash builtin, not part of POSIX sh.",
          "timestamp": "2020-12-01T10:20:49+09:00",
          "tree_id": "8b211b0f9807692d77be8a64a4835718355afe7b",
          "url": "https://github.com/tokio-rs/tokio/commit/7ae8135b62057be6b1691f04b27eabe285b05efd"
        },
        "date": 1606785752553,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 978,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15181,
            "range": "± 5247",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1042,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14160,
            "range": "± 3265",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 583,
            "range": "± 33",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8e0f0a919663b210627c132d6af3e19a95d8037",
          "message": "example: add back udp-codec example (#3205)",
          "timestamp": "2020-12-01T12:20:20+09:00",
          "tree_id": "b18851ef95641ab2e2d1f632e2ce39cb1fcb1301",
          "url": "https://github.com/tokio-rs/tokio/commit/a8e0f0a919663b210627c132d6af3e19a95d8037"
        },
        "date": 1606792911579,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 936,
            "range": "± 100",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13776,
            "range": "± 3785",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1013,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13317,
            "range": "± 2943",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 559,
            "range": "± 29",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a6051a61ec5c96113f4b543de3ec55431695347a",
          "message": "sync: make add_permits panic with usize::MAX >> 3 permits (#3188)",
          "timestamp": "2020-12-02T22:58:28+01:00",
          "tree_id": "1a4d4bcc017f6a61a652505b1edd4a3bf36ea1ab",
          "url": "https://github.com/tokio-rs/tokio/commit/a6051a61ec5c96113f4b543de3ec55431695347a"
        },
        "date": 1606946411158,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 861,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13489,
            "range": "± 4529",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 981,
            "range": "± 180",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12841,
            "range": "± 2862",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 518,
            "range": "± 86",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "647299866a2262c8a1183adad73673e5803293ed",
          "message": "util: add writev-aware `poll_write_buf` (#3156)\n\n## Motivation\r\n\r\nIn Tokio 0.2, `AsyncRead` and `AsyncWrite` had `poll_write_buf` and\r\n`poll_read_buf` methods for reading and writing to implementers of\r\n`bytes` `Buf` and `BufMut` traits. In 0.3, these were removed, but\r\n`poll_read_buf` was added as a free function in `tokio-util`. However,\r\nthere is currently no `poll_write_buf`.\r\n\r\nNow that `AsyncWrite` has regained support for vectored writes in #3149,\r\nthere's a lot of potential benefit in having a `poll_write_buf` that\r\nuses vectored writes when supported and non-vectored writes when not\r\nsupported, so that users don't have to reimplement this.\r\n\r\n## Solution\r\n\r\nThis PR adds a `poll_write_buf` function to `tokio_util::io`, analogous\r\nto the existing `poll_read_buf` function.\r\n\r\nThis function writes from a `Buf` to an `AsyncWrite`, advancing the\r\n`Buf`'s internal cursor. In addition, when the `AsyncWrite` supports\r\nvectored writes (i.e. its `is_write_vectored` method returns `true`),\r\nit will use vectored IO.\r\n\r\nI copied the documentation for this functions from the docs from Tokio\r\n0.2's `AsyncWrite::poll_write_buf` , with some minor modifications as\r\nappropriate.\r\n\r\nFinally, I fixed a minor issue in the existing docs for `poll_read_buf`\r\nand `read_buf`, and updated `tokio_util::codec` to use `poll_write_buf`.\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-12-03T11:19:16-08:00",
          "tree_id": "c92df9ae491f0a444e694879858d032c3f6a5373",
          "url": "https://github.com/tokio-rs/tokio/commit/647299866a2262c8a1183adad73673e5803293ed"
        },
        "date": 1607023235998,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 843,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12895,
            "range": "± 2580",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 872,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13423,
            "range": "± 3108",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 493,
            "range": "± 11",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "udoprog@tedro.se",
            "name": "John-John Tedro",
            "username": "udoprog"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a125ebd745f31098aa170cb1009ff0fe34508d37",
          "message": "rt: fix panic in task abort when off rt (#3159)\n\nA call to `JoinHandle::abort` releases a task. When called from outside of the runtime,\r\nthis panics due to the current implementation checking for a thread-local worker context.\r\n\r\nThis change makes accessing the thread-local context optional under release, by falling\r\nback to remotely marking a task remotely as dropped. Behaving the same as if the core\r\nwas stolen by another worker.\r\n\r\nFixes #3157",
          "timestamp": "2020-12-03T21:29:59-08:00",
          "tree_id": "8dab5d17383a5f63f7554ec009cf6e1408c46d96",
          "url": "https://github.com/tokio-rs/tokio/commit/a125ebd745f31098aa170cb1009ff0fe34508d37"
        },
        "date": 1607059898211,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1013,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14503,
            "range": "± 3611",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1043,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14148,
            "range": "± 3276",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 591,
            "range": "± 23",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "razican@protonmail.ch",
            "name": "Iban Eguia",
            "username": "Razican"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0dbba139848de6a8ee88350cc7fc48d0b05016c5",
          "message": "deps: replace lazy_static with once_cell (#3187)",
          "timestamp": "2020-12-04T10:23:13+01:00",
          "tree_id": "73f3366b9c7a0c50d6dd146a2626368cf59b3178",
          "url": "https://github.com/tokio-rs/tokio/commit/0dbba139848de6a8ee88350cc7fc48d0b05016c5"
        },
        "date": 1607073916556,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1013,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14762,
            "range": "± 3372",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1040,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14734,
            "range": "± 3395",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 594,
            "range": "± 16",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "liufuyang@users.noreply.github.com",
            "name": "Fuyang Liu",
            "username": "liufuyang"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0707f4c19210d6dac620c663e94d34834714a7c9",
          "message": "net: add TcpStream::into_std (#3189)",
          "timestamp": "2020-12-06T14:33:04+01:00",
          "tree_id": "a3aff2f279b1e560602b4752435e092b4a22424e",
          "url": "https://github.com/tokio-rs/tokio/commit/0707f4c19210d6dac620c663e94d34834714a7c9"
        },
        "date": 1607261673736,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 890,
            "range": "± 164",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13614,
            "range": "± 4967",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 877,
            "range": "± 180",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13613,
            "range": "± 2858",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 541,
            "range": "± 62",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "62023dffe5396ee1a0380f12c7530bf4ff59fe4a",
          "message": "sync: forward port 0.2 mpsc test (#3225)\n\nForward ports the test included in #3215. The mpsc sempahore has been\r\nreplaced in 0.3 and does not include the bug being fixed.",
          "timestamp": "2020-12-07T11:24:15-08:00",
          "tree_id": "c891a48ce299e6cfd01090a880d1baf16ebe0ad7",
          "url": "https://github.com/tokio-rs/tokio/commit/62023dffe5396ee1a0380f12c7530bf4ff59fe4a"
        },
        "date": 1607369145414,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 755,
            "range": "± 185",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13973,
            "range": "± 4330",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 768,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13814,
            "range": "± 3973",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 430,
            "range": "± 118",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "57dffb9dfe9e4c0f12429246540add3975f4a754",
          "message": "rt: fix deadlock in shutdown (#3228)\n\nPreviously, the runtime shutdown logic would first-hand control over all cores\r\nto a single thread, which would sequentially shut down all tasks on the core\r\nand then wait for them to complete.\r\n\r\nThis could deadlock when one task is waiting for a later core's task to\r\ncomplete. For example, in the newly added test, we have a `block_in_place` task\r\nthat is waiting for another task to be dropped. If the latter task adds its\r\ncore to the shutdown list later than the former, we end up waiting forever for\r\nthe `block_in_place` task to complete.\r\n\r\nAdditionally, there also was a bug wherein we'd attempt to park on the parker\r\nafter shutting it down which was fixed as part of the refactors above.\r\n\r\nThis change restructures the code to bring all tasks to a halt (and do any\r\nparking needed) before we collapse to a single thread to avoid this deadlock.\r\n\r\nThere was also an issue in which canceled tasks would not unpark the\r\noriginating thread, due to what appears to be some sort of optimization gone\r\nwrong. This has been fixed to be much more conservative in selecting when not\r\nto unpark the source thread (this may be too conservative; please take a look\r\nat the changes to `release()`).\r\n\r\nFixes: #2789",
          "timestamp": "2020-12-07T20:55:02-08:00",
          "tree_id": "1890e495daa058f06c8a738de4c88b0aeea52f77",
          "url": "https://github.com/tokio-rs/tokio/commit/57dffb9dfe9e4c0f12429246540add3975f4a754"
        },
        "date": 1607403395902,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 993,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13837,
            "range": "± 2964",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 963,
            "range": "± 137",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14846,
            "range": "± 4440",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 526,
            "range": "± 85",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e01391351bcb0715f737cefe94e1bc99f19af226",
          "message": "Add stress test (#3222)\n\nCreated a simple echo TCP server that on two different runtimes that is\r\ncalled from a GitHub action using Valgrind to ensure that there are\r\nno memory leaks.\r\n\r\nFixes: #3022",
          "timestamp": "2020-12-07T21:12:22-08:00",
          "tree_id": "5575f27e36e49b887062119225e1d61335a01b9a",
          "url": "https://github.com/tokio-rs/tokio/commit/e01391351bcb0715f737cefe94e1bc99f19af226"
        },
        "date": 1607404437161,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 846,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12458,
            "range": "± 1944",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 872,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12887,
            "range": "± 2690",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 492,
            "range": "± 23",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9",
          "message": "chore: fix stress test (#3233)",
          "timestamp": "2020-12-09T07:38:25+09:00",
          "tree_id": "0b92fb11f764a5e88d62a9f79aa2107ebcb75f42",
          "url": "https://github.com/tokio-rs/tokio/commit/fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9"
        },
        "date": 1607467210236,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1003,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14358,
            "range": "± 3937",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1022,
            "range": "± 82",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14079,
            "range": "± 3388",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 590,
            "range": "± 57",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9706ca92a8deb69d6e29265f21424042fea966c5",
          "message": "time: Fix race condition in timer drop (#3229)\n\nDropping a timer on the millisecond that it was scheduled for, when it was on\r\nthe pending list, could result in a panic previously, as we did not record the\r\npending-list state in cached_when.\r\n\r\nHopefully fixes: ZcashFoundation/zebra#1452",
          "timestamp": "2020-12-08T16:42:43-08:00",
          "tree_id": "cd77e2148b7cdf03d0fcb38e8e27cf3f7eed1ed9",
          "url": "https://github.com/tokio-rs/tokio/commit/9706ca92a8deb69d6e29265f21424042fea966c5"
        },
        "date": 1607474653022,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 845,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12602,
            "range": "± 2447",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 902,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13326,
            "range": "± 2678",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 491,
            "range": "± 16",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "473ddaa277f51917aeb2fe743b1582685828d6dd",
          "message": "chore: prepare for Tokio 1.0 work (#3238)",
          "timestamp": "2020-12-09T09:42:05-08:00",
          "tree_id": "7af80d4c2bfffff4b6f04db875779e2f49f31280",
          "url": "https://github.com/tokio-rs/tokio/commit/473ddaa277f51917aeb2fe743b1582685828d6dd"
        },
        "date": 1607535840861,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 932,
            "range": "± 131",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14700,
            "range": "± 4401",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1003,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15454,
            "range": "± 5274",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 544,
            "range": "± 57",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "52cd240053b2e1dd5835186539f563c3496dfd7d",
          "message": "task: add missing feature flags for task_local and spawn_blocking (#3237)",
          "timestamp": "2020-12-09T23:49:28+01:00",
          "tree_id": "bbc90b40091bd716d0269b84da2bafb32288b149",
          "url": "https://github.com/tokio-rs/tokio/commit/52cd240053b2e1dd5835186539f563c3496dfd7d"
        },
        "date": 1607554260519,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 912,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14200,
            "range": "± 5779",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 980,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14644,
            "range": "± 4024",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 548,
            "range": "± 70",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2a30e13f38b864807f9ad92023e91b060a6227a4",
          "message": "net: expose poll_* methods on UnixDatagram (#3223)",
          "timestamp": "2020-12-10T08:36:43+01:00",
          "tree_id": "2ff07d9ed9f82c562e03ae7f13dd05150ffe899f",
          "url": "https://github.com/tokio-rs/tokio/commit/2a30e13f38b864807f9ad92023e91b060a6227a4"
        },
        "date": 1607585890873,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 868,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12580,
            "range": "± 2456",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 877,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13337,
            "range": "± 2394",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 497,
            "range": "± 13",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f60860af7edefef5373d50d77ab605d648d60526",
          "message": "watch: fix spurious wakeup (#3234)\n\nCo-authored-by: @tijsvd",
          "timestamp": "2020-12-10T09:46:01+01:00",
          "tree_id": "44bc86bbaa5393a0dc3a94a2066569dcb1b79df1",
          "url": "https://github.com/tokio-rs/tokio/commit/f60860af7edefef5373d50d77ab605d648d60526"
        },
        "date": 1607590059442,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 837,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13707,
            "range": "± 2829",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 865,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13542,
            "range": "± 2929",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 493,
            "range": "± 7",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "clemens.koza@gmx.at",
            "name": "Clemens Koza",
            "username": "SillyFreak"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9646b4bce342342cc654c4c0834c0bf3627f7aa0",
          "message": "toml: enable test-util feature for the playground (#3224)",
          "timestamp": "2020-12-10T10:39:05+01:00",
          "tree_id": "0c5c06ea6a86a13b9485506cf2066945eaf53189",
          "url": "https://github.com/tokio-rs/tokio/commit/9646b4bce342342cc654c4c0834c0bf3627f7aa0"
        },
        "date": 1607593221533,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 843,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12751,
            "range": "± 2704",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 868,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12773,
            "range": "± 2163",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 492,
            "range": "± 13",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yusuktan@maguro.dev",
            "name": "Yusuke Tanaka",
            "username": "magurotuna"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4b1d76ec8f35052480eb14204d147df658bfdfdd",
          "message": "docs: fix typo in signal module documentation (#3249)",
          "timestamp": "2020-12-10T08:11:45-08:00",
          "tree_id": "46efd6f41cfaf702fb40c62b89800c511309d584",
          "url": "https://github.com/tokio-rs/tokio/commit/4b1d76ec8f35052480eb14204d147df658bfdfdd"
        },
        "date": 1607616785873,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 839,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12902,
            "range": "± 2731",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 866,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12793,
            "range": "± 2158",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 492,
            "range": "± 11",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e",
          "message": "net: Pass SocketAddr by value (#3125)",
          "timestamp": "2020-12-10T14:58:27-05:00",
          "tree_id": "d46d58a79f31dba872aa060ef378743fcedea70e",
          "url": "https://github.com/tokio-rs/tokio/commit/16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e"
        },
        "date": 1607630442786,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 883,
            "range": "± 188",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14999,
            "range": "± 6276",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 915,
            "range": "± 292",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 16015,
            "range": "± 7006",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 494,
            "range": "± 92",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "69e62ef89e481e0fb29ce3ef4ddce1eea4114000",
          "message": "sync: make TryAcquireError public (#3250)\n\nThe [`Semaphore::try_acquire`][1] method currently returns a private error type.\r\n\r\n[1]: https://docs.rs/tokio/0.3/tokio/sync/struct.Semaphore.html#method.try_acquire",
          "timestamp": "2020-12-10T19:56:05-08:00",
          "tree_id": "0784747565f6583a726c85dfedcd0527d8373cc6",
          "url": "https://github.com/tokio-rs/tokio/commit/69e62ef89e481e0fb29ce3ef4ddce1eea4114000"
        },
        "date": 1607659066065,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 995,
            "range": "± 140",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 16535,
            "range": "± 6859",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1048,
            "range": "± 146",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15950,
            "range": "± 8368",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 584,
            "range": "± 98",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cameron.evan@gmail.com",
            "name": "Evan Cameron",
            "username": "leshow"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "68717c7efaced76651915696495dcb04c890be25",
          "message": "net: remove empty udp module (#3260)",
          "timestamp": "2020-12-11T14:45:57-05:00",
          "tree_id": "1b7333194ac78d7ae87c5ca9f423ef830cb486b8",
          "url": "https://github.com/tokio-rs/tokio/commit/68717c7efaced76651915696495dcb04c890be25"
        },
        "date": 1607716048632,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 843,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12795,
            "range": "± 2849",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 885,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12789,
            "range": "± 2131",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 492,
            "range": "± 12",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b01b2dacf2e4136c0237977dac27a3688467d2ea",
          "message": "net: update `TcpStream::poll_peek` to use `ReadBuf` (#3259)\n\nCloses #2987",
          "timestamp": "2020-12-11T20:40:24-08:00",
          "tree_id": "1e0bbb86739731038cc9fd69fe112cad54662d16",
          "url": "https://github.com/tokio-rs/tokio/commit/b01b2dacf2e4136c0237977dac27a3688467d2ea"
        },
        "date": 1607748129246,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1018,
            "range": "± 210",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 19283,
            "range": "± 9824",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1093,
            "range": "± 205",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 18285,
            "range": "± 13324",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 613,
            "range": "± 78",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c1ec469ad2af883b001d54e81dad426c01f918cd",
          "message": "util: add constructors to TokioContext (#3221)",
          "timestamp": "2020-12-11T20:41:22-08:00",
          "tree_id": "cdb1273c1a4eea6c7175578bc8a13f417c3daf00",
          "url": "https://github.com/tokio-rs/tokio/commit/c1ec469ad2af883b001d54e81dad426c01f918cd"
        },
        "date": 1607748192095,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 845,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13237,
            "range": "± 3784",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 871,
            "range": "± 132",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13376,
            "range": "± 3486",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 511,
            "range": "± 74",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sunjay@users.noreply.github.com",
            "name": "Sunjay Varma",
            "username": "sunjay"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "df20c162ae1308c07073b6a67c8ba4202f52d208",
          "message": "sync: add blocking_recv method to UnboundedReceiver, similar to Receiver::blocking_recv (#3262)",
          "timestamp": "2020-12-12T08:47:35-08:00",
          "tree_id": "94fe5abd9735b0c4985d5b38a8d96c51953b0f0b",
          "url": "https://github.com/tokio-rs/tokio/commit/df20c162ae1308c07073b6a67c8ba4202f52d208"
        },
        "date": 1607791744916,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 811,
            "range": "± 218",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13869,
            "range": "± 5217",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 791,
            "range": "± 151",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14101,
            "range": "± 4264",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 474,
            "range": "± 91",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a",
          "message": "net: check for false-positives in TcpStream::ready doc test (#3255)",
          "timestamp": "2020-12-13T15:24:59+01:00",
          "tree_id": "89deb6d808e007e1728a43f0a198afe32a4aae1e",
          "url": "https://github.com/tokio-rs/tokio/commit/be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a"
        },
        "date": 1607869584589,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1008,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14139,
            "range": "± 4379",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1049,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15067,
            "range": "± 4122",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 593,
            "range": "± 27",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40773946+faith@users.noreply.github.com",
            "name": "Aldas",
            "username": "faith"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f26f444f42c9369bcf8afe14ea00ea53dd663cc2",
          "message": "doc: added tracing to the feature flags section (#3254)",
          "timestamp": "2020-12-13T15:26:56+01:00",
          "tree_id": "23e7ae2599b67a6ab6faf0586f568b0ff6e0c72d",
          "url": "https://github.com/tokio-rs/tokio/commit/f26f444f42c9369bcf8afe14ea00ea53dd663cc2"
        },
        "date": 1607869697224,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 877,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12636,
            "range": "± 2543",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 870,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13318,
            "range": "± 2641",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 492,
            "range": "± 12",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4c5541945348c4614c29a13c06f2e996eb419e42",
          "message": "net: add UnixStream readiness and non-blocking ops (#3246)",
          "timestamp": "2020-12-13T16:21:11+01:00",
          "tree_id": "355917f9e8ba45094e1a3e1f465875f51f56e255",
          "url": "https://github.com/tokio-rs/tokio/commit/4c5541945348c4614c29a13c06f2e996eb419e42"
        },
        "date": 1607872991183,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 983,
            "range": "± 155",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 17348,
            "range": "± 7520",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1068,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 17255,
            "range": "± 7712",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 564,
            "range": "± 103",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "6172808+skerkour@users.noreply.github.com",
            "name": "Sylvain Kerkour",
            "username": "skerkour"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9149d7bfae251289cd21aa9ee109b4e2a190d0fa",
          "message": "docs: mention blocking thread timeout in src/lib.rs (#3253)",
          "timestamp": "2020-12-13T16:24:16+01:00",
          "tree_id": "38b69f17cc4644ac6ca081aa1d88d5cfe35825fa",
          "url": "https://github.com/tokio-rs/tokio/commit/9149d7bfae251289cd21aa9ee109b4e2a190d0fa"
        },
        "date": 1607873144227,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1105,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 16126,
            "range": "± 4392",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1090,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15171,
            "range": "± 3583",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 617,
            "range": "± 20",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jacob@jotpot.co.uk",
            "name": "Jacob O'Toole",
            "username": "JOT85"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1f862d2e950bf5f4fb31f202a544e86880f51191",
          "message": "sync: add `watch::Sender::borrow()` (#3269)",
          "timestamp": "2020-12-13T20:46:11-08:00",
          "tree_id": "e203a63415d1b02e7b62f73f8ebeebedeaaef82d",
          "url": "https://github.com/tokio-rs/tokio/commit/1f862d2e950bf5f4fb31f202a544e86880f51191"
        },
        "date": 1607921271801,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 999,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14610,
            "range": "± 4401",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1033,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14369,
            "range": "± 3194",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 575,
            "range": "± 67",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8efa62013b551d5130791c3a79ce8ab5cb0b5abf",
          "message": "Move stream items into `tokio-stream` (#3277)\n\nThis change removes all references to `Stream` from\r\nwithin the `tokio` crate and moves them into a new\r\n`tokio-stream` crate. Most types have had their\r\n`impl Stream` removed as well in-favor of their\r\ninherent methods.\r\n\r\nCloses #2870",
          "timestamp": "2020-12-15T20:24:38-08:00",
          "tree_id": "6da8c41c8e1808bea98fd2d23ee1ec03a1cc7e80",
          "url": "https://github.com/tokio-rs/tokio/commit/8efa62013b551d5130791c3a79ce8ab5cb0b5abf"
        },
        "date": 1608092789668,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1118,
            "range": "± 302",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 21223,
            "range": "± 8076",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1148,
            "range": "± 180",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 22028,
            "range": "± 9063",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 593,
            "range": "± 70",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d74d17307dd53215061c4a8a1f20a0e30461e296",
          "message": "time: remove `Box` from `Sleep` (#3278)\n\nRemoves the box from `Sleep`, taking advantage of intrusive wakers. The\r\n`Sleep` future is now `!Unpin`.\r\n\r\nCloses #3267",
          "timestamp": "2020-12-16T21:51:34-08:00",
          "tree_id": "0cdbf57e4a9b38302ddae0078eb5a1b9a4977aa2",
          "url": "https://github.com/tokio-rs/tokio/commit/d74d17307dd53215061c4a8a1f20a0e30461e296"
        },
        "date": 1608184386228,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 787,
            "range": "± 179",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13193,
            "range": "± 3142",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 823,
            "range": "± 245",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12583,
            "range": "± 4310",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 479,
            "range": "± 81",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "59e4b35f49e6a95a961fd9003db58191de97d3a0",
          "message": "stream: Fix a few doc issues (#3285)",
          "timestamp": "2020-12-17T10:35:49-05:00",
          "tree_id": "757b133c0eac5b1c26d2ba870d0f4b5c198d7505",
          "url": "https://github.com/tokio-rs/tokio/commit/59e4b35f49e6a95a961fd9003db58191de97d3a0"
        },
        "date": 1608219441703,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 902,
            "range": "± 164",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13452,
            "range": "± 3239",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 874,
            "range": "± 204",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13424,
            "range": "± 2857",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 479,
            "range": "± 97",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8",
          "message": "docs: Add more comprehensive stream docs (#3286)\n\n* docs: Add more comprehensive stream docs\r\n\r\n* Apply suggestions from code review\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>\r\n\r\n* Fix doc tests\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-17T11:46:09-05:00",
          "tree_id": "f7afa84006c8629a0d2c058b8e52042c54436203",
          "url": "https://github.com/tokio-rs/tokio/commit/c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8"
        },
        "date": 1608223655270,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1017,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15113,
            "range": "± 4517",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1041,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14619,
            "range": "± 3902",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 592,
            "range": "± 65",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "abd4c0025539f142ec48a09e01430f7ee3b83214",
          "message": "time: enforce `current_thread` rt for time::pause (#3289)\n\nPausing time is a capability added to assist with testing Tokio code\r\ndependent on time. Currently, the capability implicitly requires the\r\ncurrent_thread runtime.\r\n\r\nThis change enforces the requirement by panicking if called from a\r\nmulti-threaded runtime.",
          "timestamp": "2020-12-17T15:37:08-08:00",
          "tree_id": "6c565d6c74dff336ac847cb6463245283d8470d5",
          "url": "https://github.com/tokio-rs/tokio/commit/abd4c0025539f142ec48a09e01430f7ee3b83214"
        },
        "date": 1608248332225,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1011,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14691,
            "range": "± 2297",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1040,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14599,
            "range": "± 1879",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 591,
            "range": "± 26",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3ecaf9fd9a519da9b1decb4c7239b48277040522",
          "message": "codec: write documentation for codec (#3283)\n\nCo-authored-by: Lucio Franco <luciofranco14@gmail.com>",
          "timestamp": "2020-12-18T21:32:27+01:00",
          "tree_id": "f116db611cb8144b9ec3e2e97286aab59cdd9556",
          "url": "https://github.com/tokio-rs/tokio/commit/3ecaf9fd9a519da9b1decb4c7239b48277040522"
        },
        "date": 1608323631748,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 744,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12315,
            "range": "± 2417",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 871,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12239,
            "range": "± 2834",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 487,
            "range": "± 12",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d948ccedfce534953a18acf46c8c6572103567c7",
          "message": "chore: fix stress test (#3297)",
          "timestamp": "2020-12-19T12:11:10+01:00",
          "tree_id": "3c417da4134a45bfff1f2d85b9b8cf410dfd9bf9",
          "url": "https://github.com/tokio-rs/tokio/commit/d948ccedfce534953a18acf46c8c6572103567c7"
        },
        "date": 1608376409018,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 976,
            "range": "± 149",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 16913,
            "range": "± 10530",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1042,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 16996,
            "range": "± 7770",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 566,
            "range": "± 85",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b99b00eb302ae6ff19ca97d32b1e594143f43a60",
          "message": "rt: change `max_threads` to `max_blocking_threads` (#3287)\n\nFixes #2802",
          "timestamp": "2020-12-19T08:04:04-08:00",
          "tree_id": "458d7fb55f921184a1056e766b6d0101fb763579",
          "url": "https://github.com/tokio-rs/tokio/commit/b99b00eb302ae6ff19ca97d32b1e594143f43a60"
        },
        "date": 1608393924714,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 845,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13620,
            "range": "± 2625",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 870,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 12998,
            "range": "± 1853",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 491,
            "range": "± 14",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "flo.huebsch@pm.me",
            "name": "Florian Hübsch",
            "username": "fl9"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e41e6cddbba0cf13403924937ffe02aae6639e28",
          "message": "docs: tokio::main macro is also supported on rt (#3243)\n\nFixes: #3144\r\nRefs: #2225",
          "timestamp": "2020-12-19T19:12:08+01:00",
          "tree_id": "ee1af0c8a3b2ab9c9eaae05f2dca96ff966f42f9",
          "url": "https://github.com/tokio-rs/tokio/commit/e41e6cddbba0cf13403924937ffe02aae6639e28"
        },
        "date": 1608401639798,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 991,
            "range": "± 157",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 16212,
            "range": "± 6268",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 978,
            "range": "± 158",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 16119,
            "range": "± 5598",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 550,
            "range": "± 54",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "78f2340d259487d4470681f97cc4b5eac719e178",
          "message": "tokio: remove prelude (#3299)\n\nCloses: #3257",
          "timestamp": "2020-12-19T11:42:24-08:00",
          "tree_id": "2a093e994a41bae54e324fd8d20c836e46a5947c",
          "url": "https://github.com/tokio-rs/tokio/commit/78f2340d259487d4470681f97cc4b5eac719e178"
        },
        "date": 1608407051512,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 937,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13755,
            "range": "± 4408",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 958,
            "range": "± 107",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14528,
            "range": "± 4390",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 529,
            "range": "± 57",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "arve.knudsen@gmail.com",
            "name": "Arve Knudsen",
            "username": "aknuds1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1b70507894035e066cb488c14ff328bd47ca696d",
          "message": "net: remove {Tcp,Unix}Stream::shutdown() (#3298)\n\n`shutdown()` on `AsyncWrite` performs a TCP shutdown. This avoids method\r\nconflicts.\r\n\r\nCloses #3294",
          "timestamp": "2020-12-19T12:17:52-08:00",
          "tree_id": "dd3c414761b8a3713b63515e7666f108a1391743",
          "url": "https://github.com/tokio-rs/tokio/commit/1b70507894035e066cb488c14ff328bd47ca696d"
        },
        "date": 1608409178108,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1011,
            "range": "± 375",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 17026,
            "range": "± 5600",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1056,
            "range": "± 216",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 17211,
            "range": "± 7207",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 587,
            "range": "± 89",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e5f513542c0f92f49f6acd31021cc056577be6b",
          "message": "chore: remove some left over `stream` feature code (#3300)\n\nRemoves the `stream` feature flag from `Cargo.toml` and removes the\r\n`futures-core` dependency. Once `Stream` lands in `std`, a feature flag\r\nis most likely not needed.",
          "timestamp": "2020-12-19T14:15:00-08:00",
          "tree_id": "bb5486367eec9a81b5d021202eaf8ab0fe34bdc8",
          "url": "https://github.com/tokio-rs/tokio/commit/5e5f513542c0f92f49f6acd31021cc056577be6b"
        },
        "date": 1608416212512,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 974,
            "range": "± 161",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14563,
            "range": "± 3624",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1007,
            "range": "± 150",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14705,
            "range": "± 4088",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 555,
            "range": "± 69",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "28933599888a88e601acbb11fa824b0ee9f98c6e",
          "message": "chore: update to `bytes` 1.0 git branch (#3301)\n\nUpdates the code base to track the `bytes` git branch. This is in\r\npreparation for the 1.0 release.\r\n\r\nCloses #3058",
          "timestamp": "2020-12-19T15:57:16-08:00",
          "tree_id": "2021ef3acf9407fcfa39032e0a493a81f1eb74cc",
          "url": "https://github.com/tokio-rs/tokio/commit/28933599888a88e601acbb11fa824b0ee9f98c6e"
        },
        "date": 1608422312749,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 882,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13368,
            "range": "± 2565",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 890,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13446,
            "range": "± 3377",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 490,
            "range": "± 7",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bIgBV@users.noreply.github.com",
            "name": "Bhargav",
            "username": "bIgBV"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c7671a03840751f58b7a509386b8fe3b5e670a37",
          "message": "io: add _mut variants of methods on AsyncFd (#3304)\n\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-21T22:51:28+01:00",
          "tree_id": "9d3f26151c9ddd292c129f43c0fac83792073f62",
          "url": "https://github.com/tokio-rs/tokio/commit/c7671a03840751f58b7a509386b8fe3b5e670a37"
        },
        "date": 1608587575943,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 972,
            "range": "± 676",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14517,
            "range": "± 10346",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1007,
            "range": "± 445",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15124,
            "range": "± 5513",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 564,
            "range": "± 354",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "564c943309f0a94474e2e71a115ec0bb45f3e7fd",
          "message": "io: rename `AsyncFd::with_io()` and rm `with_poll()` (#3306)",
          "timestamp": "2020-12-21T15:42:38-08:00",
          "tree_id": "6f29310dda04438222fb9581b3cd9581f1abe13e",
          "url": "https://github.com/tokio-rs/tokio/commit/564c943309f0a94474e2e71a115ec0bb45f3e7fd"
        },
        "date": 1608594232365,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 843,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13268,
            "range": "± 2233",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 884,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13600,
            "range": "± 2334",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 493,
            "range": "± 22",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "eee3ca65d6e5b4537915571f663f94184e616e6c",
          "message": "deps: update rand to 0.8, loom to 0.4 (#3307)",
          "timestamp": "2020-12-22T10:28:35+01:00",
          "tree_id": "2f534ebe6cb319f30f72e7a9b2b389825500f051",
          "url": "https://github.com/tokio-rs/tokio/commit/eee3ca65d6e5b4537915571f663f94184e616e6c"
        },
        "date": 1608629406324,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 838,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13113,
            "range": "± 2180",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 867,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13501,
            "range": "± 3263",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 492,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f95ad1898041d2bcc09de3db69228e88f5f4abf8",
          "message": "net: clarify when wakeups are sent (#3310)",
          "timestamp": "2020-12-22T07:38:14-08:00",
          "tree_id": "16968a03d8ca55c53da7658c109e47a5f9a8c4e4",
          "url": "https://github.com/tokio-rs/tokio/commit/f95ad1898041d2bcc09de3db69228e88f5f4abf8"
        },
        "date": 1608651600719,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1011,
            "range": "± 100",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 17898,
            "range": "± 7485",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 951,
            "range": "± 233",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 17861,
            "range": "± 11607",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 602,
            "range": "± 125",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b83b3b8cc61e9d911abec00c33ec3762b2a6437",
          "message": "fs,sync: expose poll_ fns on misc types (#3308)\n\nIncludes methods on:\r\n\r\n* fs::DirEntry\r\n* io::Lines\r\n* io::Split\r\n* sync::mpsc::Receiver\r\n* sync::misc::UnboundedReceiver",
          "timestamp": "2020-12-22T09:28:14-08:00",
          "tree_id": "bd1ae38a53efb2fe78fc8aba0bdd0789cc7a1495",
          "url": "https://github.com/tokio-rs/tokio/commit/0b83b3b8cc61e9d911abec00c33ec3762b2a6437"
        },
        "date": 1608658200133,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 966,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15854,
            "range": "± 6556",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1038,
            "range": "± 93",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14603,
            "range": "± 4034",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 559,
            "range": "± 36",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "carlos.baezruiz@gmail.com",
            "name": "Carlos B",
            "username": "carlosb1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d28e4cdbb63b0ad19e66d1b077690cbebf71353",
          "message": "examples: add futures executor threadpool (#3198)",
          "timestamp": "2020-12-22T20:08:43+01:00",
          "tree_id": "706d894cb4ea97ae1a91b9d2bd42d1800968559c",
          "url": "https://github.com/tokio-rs/tokio/commit/7d28e4cdbb63b0ad19e66d1b077690cbebf71353"
        },
        "date": 1608664212875,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 885,
            "range": "± 159",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14505,
            "range": "± 3621",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 992,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14066,
            "range": "± 3247",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 551,
            "range": "± 31",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608664521412,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 1010,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14886,
            "range": "± 3885",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1046,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14906,
            "range": "± 3229",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 591,
            "range": "± 38",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608666622480,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 896,
            "range": "± 200",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14887,
            "range": "± 3803",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 866,
            "range": "± 93",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14347,
            "range": "± 5260",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 520,
            "range": "± 94",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b",
          "message": "time: make Interval::poll_tick() public (#3316)",
          "timestamp": "2020-12-22T12:31:14-08:00",
          "tree_id": "c06c2c6a1618d8dd177cd844f8f816f06e6033b8",
          "url": "https://github.com/tokio-rs/tokio/commit/be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b"
        },
        "date": 1608669153845,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 838,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 12879,
            "range": "± 3068",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 865,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13381,
            "range": "± 3025",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 497,
            "range": "± 15",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luke.steensen@gmail.com",
            "name": "Luke Steensen",
            "username": "lukesteensen"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8dda19da4e94acd45f34b5eb359b4cffafa833f",
          "message": "chore: update to released `bytes` 1.0 (#3317)",
          "timestamp": "2020-12-22T17:09:26-08:00",
          "tree_id": "c177db0f9bced11086bcb13be4ac2348e6c94469",
          "url": "https://github.com/tokio-rs/tokio/commit/a8dda19da4e94acd45f34b5eb359b4cffafa833f"
        },
        "date": 1608685863414,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 967,
            "range": "± 350",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 15074,
            "range": "± 7342",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1008,
            "range": "± 564",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15016,
            "range": "± 7507",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 565,
            "range": "± 125",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0deaeb84948f253b76b7fe64d7fe9d4527cd4275",
          "message": "chore: remove unused `slab` dependency (#3318)",
          "timestamp": "2020-12-22T21:56:22-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/0deaeb84948f253b76b7fe64d7fe9d4527cd4275"
        },
        "date": 1608703091321,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 885,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14309,
            "range": "± 4532",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 906,
            "range": "± 156",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13981,
            "range": "± 3767",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 511,
            "range": "± 46",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "575938d4579e6fe6a89b700aadb0ae2bbab5483b",
          "message": "chore: use #[non_exhaustive] instead of private unit field (#3320)",
          "timestamp": "2020-12-23T22:48:33+09:00",
          "tree_id": "f2782c6135a26568d5dea4d45b3310baaf062e16",
          "url": "https://github.com/tokio-rs/tokio/commit/575938d4579e6fe6a89b700aadb0ae2bbab5483b"
        },
        "date": 1608731411885,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 898,
            "range": "± 232",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 17026,
            "range": "± 7207",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 884,
            "range": "± 194",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 15673,
            "range": "± 6837",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 498,
            "range": "± 108",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ce0e9c67cfe61c7a91a284331ecc53fa01c32879",
          "message": "chore: Revert \"use #[non_exhaustive] instead of private unit field\" (#3323)\n\nThis reverts commit 575938d4579e6fe6a89b700aadb0ae2bbab5483b.",
          "timestamp": "2020-12-23T08:27:58-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/ce0e9c67cfe61c7a91a284331ecc53fa01c32879"
        },
        "date": 1608740970934,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 965,
            "range": "± 217",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14292,
            "range": "± 5016",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 1008,
            "range": "± 235",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14807,
            "range": "± 4906",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 566,
            "range": "± 129",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@paolo565.org",
            "name": "Paolo Barbolini",
            "username": "paolobarbolini"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "aa6597ba66b10fa9175ccd7b3c0335f4a49b91c5",
          "message": "compat: update traits naming to match tokio 1.0 (#3324)",
          "timestamp": "2020-12-23T09:15:56-08:00",
          "tree_id": "d500c90ca41df0a66e18314a5d6a68c0e7ec2dde",
          "url": "https://github.com/tokio-rs/tokio/commit/aa6597ba66b10fa9175ccd7b3c0335f4a49b91c5"
        },
        "date": 1608743834477,
        "tool": "cargo",
        "benches": [
          {
            "name": "read_concurrent_contended",
            "value": 843,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 13069,
            "range": "± 2766",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 873,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 13138,
            "range": "± 2656",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 490,
            "range": "± 14",
            "unit": "ns/iter"
          }
        ]
      }
    ],
    "rt_multi_threaded": [
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "distinct": true,
          "id": "af032dbf59195f5a637c14fd8805f45cce8c8563",
          "message": "try again",
          "timestamp": "2020-11-13T16:47:49-08:00",
          "tree_id": "2c351a9bf2bc6d1fb70754ee19640da3b69df204",
          "url": "https://github.com/tokio-rs/tokio/commit/af032dbf59195f5a637c14fd8805f45cce8c8563"
        },
        "date": 1605314974927,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 199046,
            "range": "± 55572",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 760862,
            "range": "± 157394",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5490742,
            "range": "± 748448",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20486755,
            "range": "± 2300375",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "97c2c4203cd7c42960cac895987c43a17dff052e",
          "message": "chore: automate running benchmarks (#3140)\n\nUses Github actions to run benchmarks.",
          "timestamp": "2020-11-13T19:30:52-08:00",
          "tree_id": "f4a3cfebafb7afee68d6d4de1748daddcfc070c6",
          "url": "https://github.com/tokio-rs/tokio/commit/97c2c4203cd7c42960cac895987c43a17dff052e"
        },
        "date": 1605324750921,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 178128,
            "range": "± 14270",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 707946,
            "range": "± 48048",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5326109,
            "range": "± 635423",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19949182,
            "range": "± 3024767",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "masnagam@gmail.com",
            "name": "masnagam",
            "username": "masnagam"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4e39c9b818eb8af064bb9f45f47e3cfc6593de95",
          "message": "net: restore TcpStream::{poll_read_ready, poll_write_ready} (#2743)",
          "timestamp": "2020-11-16T09:51:06-08:00",
          "tree_id": "2222dd2f8638fb64f228badef84814d2f4079a82",
          "url": "https://github.com/tokio-rs/tokio/commit/4e39c9b818eb8af064bb9f45f47e3cfc6593de95"
        },
        "date": 1605549203912,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 182327,
            "range": "± 35912",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 649727,
            "range": "± 102719",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4047012,
            "range": "± 839128",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18946145,
            "range": "± 2027569",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f5cb4c20422a35b51bfba3391744f8bcb54f7581",
          "message": "net: Add send/recv buf size methods to `TcpSocket` (#3145)\n\nThis commit adds `set_{send, recv}_buffer_size` methods to `TcpSocket`\r\nfor setting the size of the TCP send and receive buffers, and `{send,\r\nrecv}_buffer_size` methods for returning the current value. These just\r\ncall into similar methods on `mio`'s `TcpSocket` type, which were added\r\nin tokio-rs/mio#1384.\r\n\r\nRefs: #3082\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-11-16T12:29:03-08:00",
          "tree_id": "fcf642984a21d04533efad0cdde613d294635c4d",
          "url": "https://github.com/tokio-rs/tokio/commit/f5cb4c20422a35b51bfba3391744f8bcb54f7581"
        },
        "date": 1605558645947,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 191217,
            "range": "± 14072",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 710720,
            "range": "± 43091",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5226983,
            "range": "± 533736",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19521931,
            "range": "± 2271038",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "zaharidichev@gmail.com",
            "name": "Zahari Dichev",
            "username": "zaharidichev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d0ebb4154748166a4ba07baa4b424a1c45efd219",
          "message": "sync: add `Notify::notify_waiters` (#3098)\n\nThis PR makes `Notify::notify_waiters` public. The method\r\nalready exists, but it changes the way `notify_waiters`,\r\nis used. Previously in order for the consumer to\r\nregister interest, in a notification triggered by\r\n`notify_waiters`, the `Notified` future had to be\r\npolled. This introduced friction when using the api\r\nas the future had to be pinned before polled.\r\n\r\nThis change introduces a counter that tracks how many\r\ntimes `notified_waiters` has been called. Upon creation of\r\nthe future the number of times is loaded. When first\r\npolled the future compares this number with the count\r\nstate of the `Notify` type. This avoids the need for\r\nregistering the waiter upfront.\r\n\r\nFixes: #3066",
          "timestamp": "2020-11-16T12:49:35-08:00",
          "tree_id": "5ea4d611256290f62baea1a9ffa3333b254181df",
          "url": "https://github.com/tokio-rs/tokio/commit/d0ebb4154748166a4ba07baa4b424a1c45efd219"
        },
        "date": 1605559892943,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 191688,
            "range": "± 39550",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 724625,
            "range": "± 52874",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5279089,
            "range": "± 569953",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21143658,
            "range": "± 1850531",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0ea23076503c5151d68a781a3d91823396c82949",
          "message": "net: add UdpSocket readiness and non-blocking ops (#3138)\n\nAdds `ready()`, `readable()`, and `writable()` async methods for waiting\r\nfor socket readiness. Adds `try_send`, `try_send_to`, `try_recv`, and\r\n`try_recv_from` for performing non-blocking operations on the socket.\r\n\r\nThis is the UDP equivalent of #3130.",
          "timestamp": "2020-11-16T15:44:01-08:00",
          "tree_id": "1e49d7dc0bb3cee6271133d942ba49c5971fde29",
          "url": "https://github.com/tokio-rs/tokio/commit/0ea23076503c5151d68a781a3d91823396c82949"
        },
        "date": 1605570348602,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 179160,
            "range": "± 73737",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 618494,
            "range": "± 150449",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4766918,
            "range": "± 1107083",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19012569,
            "range": "± 4448367",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "9832640+zekisherif@users.noreply.github.com",
            "name": "Zeki Sherif",
            "username": "zekisherif"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d11aa866837eea50a6f1e0ef7e24846a653cbf1",
          "message": "net: add SO_LINGER get/set to TcpStream (#3143)",
          "timestamp": "2020-11-17T09:58:00-08:00",
          "tree_id": "ca0d5edc04a29bbe6e2906c760a22908e032a4c9",
          "url": "https://github.com/tokio-rs/tokio/commit/7d11aa866837eea50a6f1e0ef7e24846a653cbf1"
        },
        "date": 1605636009065,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 214451,
            "range": "± 25711",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 790044,
            "range": "± 167979",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5645160,
            "range": "± 704481",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 22209149,
            "range": "± 3825346",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "34fcef258b84d17f8d418b39eb61fa07fa87c390",
          "message": "io: add vectored writes to `AsyncWrite` (#3149)\n\nThis adds `AsyncWrite::poll_write_vectored`, and implements it for\r\n`TcpStream` and `UnixStream`.\r\n\r\nRefs: #3135.",
          "timestamp": "2020-11-18T10:41:47-08:00",
          "tree_id": "98e37fc2d6fa541a9e499331df86ba3d1b7b6e3a",
          "url": "https://github.com/tokio-rs/tokio/commit/34fcef258b84d17f8d418b39eb61fa07fa87c390"
        },
        "date": 1605725037410,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 208313,
            "range": "± 45110",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 736590,
            "range": "± 151516",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5234403,
            "range": "± 831789",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20304040,
            "range": "± 2144433",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "479c545c20b2cb44a8f09600733adc8c8dcb5aa0",
          "message": "chore: prepare v0.3.4 release (#3152)",
          "timestamp": "2020-11-18T12:38:13-08:00",
          "tree_id": "df6daba6b2f595de47ada2dd2f518475669ab919",
          "url": "https://github.com/tokio-rs/tokio/commit/479c545c20b2cb44a8f09600733adc8c8dcb5aa0"
        },
        "date": 1605732017041,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 195866,
            "range": "± 16539",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 727582,
            "range": "± 199697",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5428466,
            "range": "± 908457",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20820491,
            "range": "± 4055596",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84",
          "message": "util: fix typo in udp/frame.rs (#3154)",
          "timestamp": "2020-11-20T15:06:14+09:00",
          "tree_id": "f09954c70e26336bdb1bc525f832916c2d7037bf",
          "url": "https://github.com/tokio-rs/tokio/commit/49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84"
        },
        "date": 1605852489821,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 189192,
            "range": "± 8967",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 702064,
            "range": "± 44051",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5500810,
            "range": "± 1053690",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20865377,
            "range": "± 2857375",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f927f01a34d7cedf0cdc820f729a7a6cd56e83dd",
          "message": "macros: fix rustfmt on 1.48.0 (#3160)\n\n## Motivation\r\n\r\nLooks like the Rust 1.48.0 version of `rustfmt` changed some formatting\r\nrules (fixed some bugs?), and some of the code in `tokio-macros` is no\r\nlonger correctly formatted. This is breaking CI.\r\n\r\n## Solution\r\n\r\nThis commit runs rustfmt on Rust 1.48.0. This fixes CI.\r\n\r\nCloses #3158",
          "timestamp": "2020-11-20T10:19:26-08:00",
          "tree_id": "bd0243a653ee49cfc50bf61b00a36cc0fce6a414",
          "url": "https://github.com/tokio-rs/tokio/commit/f927f01a34d7cedf0cdc820f729a7a6cd56e83dd"
        },
        "date": 1605896473214,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 194070,
            "range": "± 27873",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 694259,
            "range": "± 87017",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5250419,
            "range": "± 1213331",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19153793,
            "range": "± 2780219",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ae67851f11b7cc1f577de8ce21767ce3e2c7aff9",
          "message": "time: use intrusive lists for timer tracking (#3080)\n\nMore-or-less a half-rewrite of the current time driver, supporting the\r\nuse of intrusive futures for timer registration.\r\n\r\nFixes: #3028, #3069",
          "timestamp": "2020-11-23T10:42:50-08:00",
          "tree_id": "be43cb76333b0e9e42a101d659f9b2e41555d779",
          "url": "https://github.com/tokio-rs/tokio/commit/ae67851f11b7cc1f577de8ce21767ce3e2c7aff9"
        },
        "date": 1606157070461,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 173680,
            "range": "± 53274",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 657451,
            "range": "± 137368",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4540370,
            "range": "± 688983",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18223628,
            "range": "± 2718440",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "driftluo@foxmail.com",
            "name": "漂流",
            "username": "driftluo"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "874fc3320bc000fee20d63b3ad865a1145122640",
          "message": "codec: add read_buffer_mut to FramedRead (#3166)",
          "timestamp": "2020-11-24T09:39:16+01:00",
          "tree_id": "53540b744f6a915cedc1099afe1b0639443b2436",
          "url": "https://github.com/tokio-rs/tokio/commit/874fc3320bc000fee20d63b3ad865a1145122640"
        },
        "date": 1606207270323,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 189797,
            "range": "± 31752",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 715813,
            "range": "± 109946",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5154978,
            "range": "± 1056989",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20258373,
            "range": "± 2845256",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "de33ee85ce61377b316b630e4355d419cc4abcb7",
          "message": "time: replace 'ouClockTimeide' in internal docs with 'outside' (#3171)",
          "timestamp": "2020-11-24T10:23:20+01:00",
          "tree_id": "5ed85f95ea1846983471a11fe555328e6b0f5f6f",
          "url": "https://github.com/tokio-rs/tokio/commit/de33ee85ce61377b316b630e4355d419cc4abcb7"
        },
        "date": 1606209893658,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 165011,
            "range": "± 21067",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 627437,
            "range": "± 58843",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5240104,
            "range": "± 1229941",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18931567,
            "range": "± 3824387",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rajiv.chauhan@gmail.com",
            "name": "Rajiv Chauhan",
            "username": "chauhraj"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e406a7a47699d93fa2a77fb72553600cb7abd0f",
          "message": "macros: fix outdated documentation (#3180)\n\n1. Changed 0.2 to 0.3\r\n2. Changed ‘multi’ to ‘single’ to indicate that the behavior is single threaded",
          "timestamp": "2020-11-26T19:46:15+01:00",
          "tree_id": "ac6898684e4b84e4a5d0e781adf42d950bbc9e43",
          "url": "https://github.com/tokio-rs/tokio/commit/5e406a7a47699d93fa2a77fb72553600cb7abd0f"
        },
        "date": 1606416482206,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 178594,
            "range": "± 14184",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 675232,
            "range": "± 139132",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4452171,
            "range": "± 763665",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19263524,
            "range": "± 2608454",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "niklas.fiekas@backscattering.de",
            "name": "Niklas Fiekas",
            "username": "niklasf"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49129434198a96444bc0e9582a14062d3a46e93a",
          "message": "signal: expose CtrlC stream on windows (#3186)\n\n* Make tokio::signal::windows::ctrl_c() public.\r\n* Stop referring to private tokio::signal::windows::Event in module\r\n  documentation.\r\n\r\nCloses #3178",
          "timestamp": "2020-11-27T19:53:17Z",
          "tree_id": "904fb6b1fb539bffe69168c7202ccc3db15321dc",
          "url": "https://github.com/tokio-rs/tokio/commit/49129434198a96444bc0e9582a14062d3a46e93a"
        },
        "date": 1606506895862,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 183354,
            "range": "± 43745",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 692623,
            "range": "± 86906",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5161069,
            "range": "± 1134488",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20082488,
            "range": "± 2735348",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0acd06b42a9d1461302388f2a533e86d391d6040",
          "message": "runtime: fix shutdown_timeout(0) blocking (#3174)",
          "timestamp": "2020-11-28T19:31:13+01:00",
          "tree_id": "c17e5d58e10ee419e492cb831843c3f08e1f66d8",
          "url": "https://github.com/tokio-rs/tokio/commit/0acd06b42a9d1461302388f2a533e86d391d6040"
        },
        "date": 1606588382035,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 194652,
            "range": "± 18392",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 718576,
            "range": "± 95435",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5179289,
            "range": "± 307680",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20068770,
            "range": "± 2233073",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c55d846f4b248b4a72335d6c57829fa6396ab9a5",
          "message": "util: add rt to tokio-util full feature (#3194)",
          "timestamp": "2020-11-29T09:48:31+01:00",
          "tree_id": "5f27b29cd1018796f0713d6e87e4823920ba5084",
          "url": "https://github.com/tokio-rs/tokio/commit/c55d846f4b248b4a72335d6c57829fa6396ab9a5"
        },
        "date": 1606639810489,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 149198,
            "range": "± 38149",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 597869,
            "range": "± 122455",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4995684,
            "range": "± 1660653",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18353613,
            "range": "± 4295590",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kylekosic@gmail.com",
            "name": "Kyle Kosic",
            "username": "kykosic"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a85fdb884d961bb87a2f3d446c548802868e54cb",
          "message": "runtime: test for shutdown_timeout(0) (#3196)",
          "timestamp": "2020-11-29T21:30:19+01:00",
          "tree_id": "c554597c6596dc6eddc98bfafcc512361ddb5f31",
          "url": "https://github.com/tokio-rs/tokio/commit/a85fdb884d961bb87a2f3d446c548802868e54cb"
        },
        "date": 1606681921160,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 181412,
            "range": "± 32541",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 686924,
            "range": "± 150303",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4791050,
            "range": "± 1369253",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 17574576,
            "range": "± 3841532",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pickfire@riseup.net",
            "name": "Ivan Tham",
            "username": "pickfire"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "72d6346c0d43d867002dc0cc5527fbd0b0e23c3f",
          "message": "macros: #[tokio::main] can be used on non-main (#3199)",
          "timestamp": "2020-11-30T17:34:11+01:00",
          "tree_id": "c558d1cb380cc67bfc56ea960a7d9e266259367a",
          "url": "https://github.com/tokio-rs/tokio/commit/72d6346c0d43d867002dc0cc5527fbd0b0e23c3f"
        },
        "date": 1606754165584,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 190514,
            "range": "± 32887",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 721690,
            "range": "± 154815",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5093712,
            "range": "± 1286533",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19983728,
            "range": "± 2824381",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "73653352+HK416-is-all-you-need@users.noreply.github.com",
            "name": "HK416-is-all-you-need",
            "username": "HK416-is-all-you-need"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5",
          "message": "io: add AsyncFd::with_interest (#3167)\n\nFixes #3072",
          "timestamp": "2020-11-30T11:11:18-08:00",
          "tree_id": "45e9d190af02ab0cdc92c317e3127a1b8227ac3a",
          "url": "https://github.com/tokio-rs/tokio/commit/7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5"
        },
        "date": 1606763587212,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 200536,
            "range": "± 62617",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 742339,
            "range": "± 154012",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5566510,
            "range": "± 1874669",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 22165634,
            "range": "± 3394684",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "08548583b948a0be04338f1b1462917c001dbf4a",
          "message": "chore: prepare v0.3.5 release (#3201)",
          "timestamp": "2020-11-30T12:57:31-08:00",
          "tree_id": "bc964338ba8d03930d53192a1e2288132330ff97",
          "url": "https://github.com/tokio-rs/tokio/commit/08548583b948a0be04338f1b1462917c001dbf4a"
        },
        "date": 1606769996263,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 166879,
            "range": "± 26242",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 645360,
            "range": "± 57330",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5110708,
            "range": "± 663855",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19542729,
            "range": "± 3389790",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "128495168d390092df2cb8ae8577cfec09f666ff",
          "message": "ci: switch FreeBSD CI environment to 12.2-RELEASE (#3202)\n\n12.1 will be EoL in two months.",
          "timestamp": "2020-12-01T10:19:54+09:00",
          "tree_id": "2a289d5667b3ffca2ebfb747785c380ee7eac034",
          "url": "https://github.com/tokio-rs/tokio/commit/128495168d390092df2cb8ae8577cfec09f666ff"
        },
        "date": 1606785710771,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 200406,
            "range": "± 21625",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 766030,
            "range": "± 95372",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5786255,
            "range": "± 1080625",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21796476,
            "range": "± 2603441",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "353b0544a04214e7d6e828641e2045df1d97cda8",
          "message": "ci: reenable CI on FreeBSD i686 (#3204)\n\nIt was temporarily disabled in 06c473e62842d257ed275497ce906710ea3f8e19\r\nand never reenabled.",
          "timestamp": "2020-12-01T10:20:18+09:00",
          "tree_id": "468f282ba9f5116f5ed9a81abacbb7385aaa9c1e",
          "url": "https://github.com/tokio-rs/tokio/commit/353b0544a04214e7d6e828641e2045df1d97cda8"
        },
        "date": 1606785722311,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 171799,
            "range": "± 21288",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 669940,
            "range": "± 65145",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5180493,
            "range": "± 631157",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20118298,
            "range": "± 3088825",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ae8135b62057be6b1691f04b27eabe285b05efd",
          "message": "process: fix the process_kill_on_drop.rs test on non-Linux systems (#3203)\n\n\"disown\" is a bash builtin, not part of POSIX sh.",
          "timestamp": "2020-12-01T10:20:49+09:00",
          "tree_id": "8b211b0f9807692d77be8a64a4835718355afe7b",
          "url": "https://github.com/tokio-rs/tokio/commit/7ae8135b62057be6b1691f04b27eabe285b05efd"
        },
        "date": 1606785762830,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 191072,
            "range": "± 79286",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 730697,
            "range": "± 246621",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4885353,
            "range": "± 1002209",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21430533,
            "range": "± 2133099",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8e0f0a919663b210627c132d6af3e19a95d8037",
          "message": "example: add back udp-codec example (#3205)",
          "timestamp": "2020-12-01T12:20:20+09:00",
          "tree_id": "b18851ef95641ab2e2d1f632e2ce39cb1fcb1301",
          "url": "https://github.com/tokio-rs/tokio/commit/a8e0f0a919663b210627c132d6af3e19a95d8037"
        },
        "date": 1606792912609,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 160333,
            "range": "± 15606",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 648572,
            "range": "± 69638",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5151938,
            "range": "± 1114416",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18520279,
            "range": "± 3283676",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a6051a61ec5c96113f4b543de3ec55431695347a",
          "message": "sync: make add_permits panic with usize::MAX >> 3 permits (#3188)",
          "timestamp": "2020-12-02T22:58:28+01:00",
          "tree_id": "1a4d4bcc017f6a61a652505b1edd4a3bf36ea1ab",
          "url": "https://github.com/tokio-rs/tokio/commit/a6051a61ec5c96113f4b543de3ec55431695347a"
        },
        "date": 1606946408351,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 158939,
            "range": "± 17712",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 600250,
            "range": "± 30952",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4960717,
            "range": "± 982395",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18440283,
            "range": "± 3008249",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "647299866a2262c8a1183adad73673e5803293ed",
          "message": "util: add writev-aware `poll_write_buf` (#3156)\n\n## Motivation\r\n\r\nIn Tokio 0.2, `AsyncRead` and `AsyncWrite` had `poll_write_buf` and\r\n`poll_read_buf` methods for reading and writing to implementers of\r\n`bytes` `Buf` and `BufMut` traits. In 0.3, these were removed, but\r\n`poll_read_buf` was added as a free function in `tokio-util`. However,\r\nthere is currently no `poll_write_buf`.\r\n\r\nNow that `AsyncWrite` has regained support for vectored writes in #3149,\r\nthere's a lot of potential benefit in having a `poll_write_buf` that\r\nuses vectored writes when supported and non-vectored writes when not\r\nsupported, so that users don't have to reimplement this.\r\n\r\n## Solution\r\n\r\nThis PR adds a `poll_write_buf` function to `tokio_util::io`, analogous\r\nto the existing `poll_read_buf` function.\r\n\r\nThis function writes from a `Buf` to an `AsyncWrite`, advancing the\r\n`Buf`'s internal cursor. In addition, when the `AsyncWrite` supports\r\nvectored writes (i.e. its `is_write_vectored` method returns `true`),\r\nit will use vectored IO.\r\n\r\nI copied the documentation for this functions from the docs from Tokio\r\n0.2's `AsyncWrite::poll_write_buf` , with some minor modifications as\r\nappropriate.\r\n\r\nFinally, I fixed a minor issue in the existing docs for `poll_read_buf`\r\nand `read_buf`, and updated `tokio_util::codec` to use `poll_write_buf`.\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-12-03T11:19:16-08:00",
          "tree_id": "c92df9ae491f0a444e694879858d032c3f6a5373",
          "url": "https://github.com/tokio-rs/tokio/commit/647299866a2262c8a1183adad73673e5803293ed"
        },
        "date": 1607023275721,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 160561,
            "range": "± 6801",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 626121,
            "range": "± 27061",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4599819,
            "range": "± 919230",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19634997,
            "range": "± 3304098",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "00500d1b35f00c68117d8f4e7320303e967e92e3",
          "message": "util: prepare v0.5.1 release (#3210)\n\n### Added\r\n\r\n- io: `poll_read_buf` util fn (#2972).\r\n- io: `poll_write_buf` util fn with vectored write support (#3156).\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-12-03T15:30:52-08:00",
          "tree_id": "fe18e0f55daa4f26cf53bfe42a713338ac5460d9",
          "url": "https://github.com/tokio-rs/tokio/commit/00500d1b35f00c68117d8f4e7320303e967e92e3"
        },
        "date": 1607038354383,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 188992,
            "range": "± 11669",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 719349,
            "range": "± 45791",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5472948,
            "range": "± 1387124",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20859312,
            "range": "± 3000318",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "udoprog@tedro.se",
            "name": "John-John Tedro",
            "username": "udoprog"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a125ebd745f31098aa170cb1009ff0fe34508d37",
          "message": "rt: fix panic in task abort when off rt (#3159)\n\nA call to `JoinHandle::abort` releases a task. When called from outside of the runtime,\r\nthis panics due to the current implementation checking for a thread-local worker context.\r\n\r\nThis change makes accessing the thread-local context optional under release, by falling\r\nback to remotely marking a task remotely as dropped. Behaving the same as if the core\r\nwas stolen by another worker.\r\n\r\nFixes #3157",
          "timestamp": "2020-12-03T21:29:59-08:00",
          "tree_id": "8dab5d17383a5f63f7554ec009cf6e1408c46d96",
          "url": "https://github.com/tokio-rs/tokio/commit/a125ebd745f31098aa170cb1009ff0fe34508d37"
        },
        "date": 1607059901868,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 199560,
            "range": "± 59640",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 729955,
            "range": "± 120655",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4926021,
            "range": "± 1324623",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20701229,
            "range": "± 2346646",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "razican@protonmail.ch",
            "name": "Iban Eguia",
            "username": "Razican"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0dbba139848de6a8ee88350cc7fc48d0b05016c5",
          "message": "deps: replace lazy_static with once_cell (#3187)",
          "timestamp": "2020-12-04T10:23:13+01:00",
          "tree_id": "73f3366b9c7a0c50d6dd146a2626368cf59b3178",
          "url": "https://github.com/tokio-rs/tokio/commit/0dbba139848de6a8ee88350cc7fc48d0b05016c5"
        },
        "date": 1607073945789,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 187704,
            "range": "± 9813",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 705204,
            "range": "± 58345",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5383020,
            "range": "± 544131",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20944514,
            "range": "± 2285348",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "liufuyang@users.noreply.github.com",
            "name": "Fuyang Liu",
            "username": "liufuyang"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0707f4c19210d6dac620c663e94d34834714a7c9",
          "message": "net: add TcpStream::into_std (#3189)",
          "timestamp": "2020-12-06T14:33:04+01:00",
          "tree_id": "a3aff2f279b1e560602b4752435e092b4a22424e",
          "url": "https://github.com/tokio-rs/tokio/commit/0707f4c19210d6dac620c663e94d34834714a7c9"
        },
        "date": 1607261680258,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 197228,
            "range": "± 35613",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 730582,
            "range": "± 64602",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5785116,
            "range": "± 1138852",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21012384,
            "range": "± 4377927",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "62023dffe5396ee1a0380f12c7530bf4ff59fe4a",
          "message": "sync: forward port 0.2 mpsc test (#3225)\n\nForward ports the test included in #3215. The mpsc sempahore has been\r\nreplaced in 0.3 and does not include the bug being fixed.",
          "timestamp": "2020-12-07T11:24:15-08:00",
          "tree_id": "c891a48ce299e6cfd01090a880d1baf16ebe0ad7",
          "url": "https://github.com/tokio-rs/tokio/commit/62023dffe5396ee1a0380f12c7530bf4ff59fe4a"
        },
        "date": 1607369160047,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 197188,
            "range": "± 19253",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 672718,
            "range": "± 77763",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4734092,
            "range": "± 1352744",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19308954,
            "range": "± 2881973",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "57dffb9dfe9e4c0f12429246540add3975f4a754",
          "message": "rt: fix deadlock in shutdown (#3228)\n\nPreviously, the runtime shutdown logic would first-hand control over all cores\r\nto a single thread, which would sequentially shut down all tasks on the core\r\nand then wait for them to complete.\r\n\r\nThis could deadlock when one task is waiting for a later core's task to\r\ncomplete. For example, in the newly added test, we have a `block_in_place` task\r\nthat is waiting for another task to be dropped. If the latter task adds its\r\ncore to the shutdown list later than the former, we end up waiting forever for\r\nthe `block_in_place` task to complete.\r\n\r\nAdditionally, there also was a bug wherein we'd attempt to park on the parker\r\nafter shutting it down which was fixed as part of the refactors above.\r\n\r\nThis change restructures the code to bring all tasks to a halt (and do any\r\nparking needed) before we collapse to a single thread to avoid this deadlock.\r\n\r\nThere was also an issue in which canceled tasks would not unpark the\r\noriginating thread, due to what appears to be some sort of optimization gone\r\nwrong. This has been fixed to be much more conservative in selecting when not\r\nto unpark the source thread (this may be too conservative; please take a look\r\nat the changes to `release()`).\r\n\r\nFixes: #2789",
          "timestamp": "2020-12-07T20:55:02-08:00",
          "tree_id": "1890e495daa058f06c8a738de4c88b0aeea52f77",
          "url": "https://github.com/tokio-rs/tokio/commit/57dffb9dfe9e4c0f12429246540add3975f4a754"
        },
        "date": 1607403405252,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 159371,
            "range": "± 34182",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 637163,
            "range": "± 82136",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5070358,
            "range": "± 1453852",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18791928,
            "range": "± 2796134",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e01391351bcb0715f737cefe94e1bc99f19af226",
          "message": "Add stress test (#3222)\n\nCreated a simple echo TCP server that on two different runtimes that is\r\ncalled from a GitHub action using Valgrind to ensure that there are\r\nno memory leaks.\r\n\r\nFixes: #3022",
          "timestamp": "2020-12-07T21:12:22-08:00",
          "tree_id": "5575f27e36e49b887062119225e1d61335a01b9a",
          "url": "https://github.com/tokio-rs/tokio/commit/e01391351bcb0715f737cefe94e1bc99f19af226"
        },
        "date": 1607404426355,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 157888,
            "range": "± 2943",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 600894,
            "range": "± 18102",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4390367,
            "range": "± 335539",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18592921,
            "range": "± 2576340",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9",
          "message": "chore: fix stress test (#3233)",
          "timestamp": "2020-12-09T07:38:25+09:00",
          "tree_id": "0b92fb11f764a5e88d62a9f79aa2107ebcb75f42",
          "url": "https://github.com/tokio-rs/tokio/commit/fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9"
        },
        "date": 1607467220358,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 187922,
            "range": "± 8322",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 719102,
            "range": "± 42834",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5665844,
            "range": "± 1610506",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21615448,
            "range": "± 2515932",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9706ca92a8deb69d6e29265f21424042fea966c5",
          "message": "time: Fix race condition in timer drop (#3229)\n\nDropping a timer on the millisecond that it was scheduled for, when it was on\r\nthe pending list, could result in a panic previously, as we did not record the\r\npending-list state in cached_when.\r\n\r\nHopefully fixes: ZcashFoundation/zebra#1452",
          "timestamp": "2020-12-08T16:42:43-08:00",
          "tree_id": "cd77e2148b7cdf03d0fcb38e8e27cf3f7eed1ed9",
          "url": "https://github.com/tokio-rs/tokio/commit/9706ca92a8deb69d6e29265f21424042fea966c5"
        },
        "date": 1607474656366,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 179734,
            "range": "± 31815",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 688149,
            "range": "± 121324",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4546759,
            "range": "± 1065367",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18779271,
            "range": "± 2298570",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "473ddaa277f51917aeb2fe743b1582685828d6dd",
          "message": "chore: prepare for Tokio 1.0 work (#3238)",
          "timestamp": "2020-12-09T09:42:05-08:00",
          "tree_id": "7af80d4c2bfffff4b6f04db875779e2f49f31280",
          "url": "https://github.com/tokio-rs/tokio/commit/473ddaa277f51917aeb2fe743b1582685828d6dd"
        },
        "date": 1607535822882,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 179997,
            "range": "± 27565",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 681081,
            "range": "± 102306",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5364166,
            "range": "± 1533853",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19128441,
            "range": "± 2650473",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "52cd240053b2e1dd5835186539f563c3496dfd7d",
          "message": "task: add missing feature flags for task_local and spawn_blocking (#3237)",
          "timestamp": "2020-12-09T23:49:28+01:00",
          "tree_id": "bbc90b40091bd716d0269b84da2bafb32288b149",
          "url": "https://github.com/tokio-rs/tokio/commit/52cd240053b2e1dd5835186539f563c3496dfd7d"
        },
        "date": 1607554269706,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 192231,
            "range": "± 48706",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 725300,
            "range": "± 123760",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5111998,
            "range": "± 617868",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20899319,
            "range": "± 2797104",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2a30e13f38b864807f9ad92023e91b060a6227a4",
          "message": "net: expose poll_* methods on UnixDatagram (#3223)",
          "timestamp": "2020-12-10T08:36:43+01:00",
          "tree_id": "2ff07d9ed9f82c562e03ae7f13dd05150ffe899f",
          "url": "https://github.com/tokio-rs/tokio/commit/2a30e13f38b864807f9ad92023e91b060a6227a4"
        },
        "date": 1607585907284,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 191892,
            "range": "± 40861",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 699384,
            "range": "± 174835",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4700459,
            "range": "± 932919",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19510477,
            "range": "± 3348179",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f60860af7edefef5373d50d77ab605d648d60526",
          "message": "watch: fix spurious wakeup (#3234)\n\nCo-authored-by: @tijsvd",
          "timestamp": "2020-12-10T09:46:01+01:00",
          "tree_id": "44bc86bbaa5393a0dc3a94a2066569dcb1b79df1",
          "url": "https://github.com/tokio-rs/tokio/commit/f60860af7edefef5373d50d77ab605d648d60526"
        },
        "date": 1607590064827,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 203790,
            "range": "± 83331",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 729613,
            "range": "± 155547",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5307925,
            "range": "± 1032487",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21233943,
            "range": "± 4386394",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "clemens.koza@gmx.at",
            "name": "Clemens Koza",
            "username": "SillyFreak"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9646b4bce342342cc654c4c0834c0bf3627f7aa0",
          "message": "toml: enable test-util feature for the playground (#3224)",
          "timestamp": "2020-12-10T10:39:05+01:00",
          "tree_id": "0c5c06ea6a86a13b9485506cf2066945eaf53189",
          "url": "https://github.com/tokio-rs/tokio/commit/9646b4bce342342cc654c4c0834c0bf3627f7aa0"
        },
        "date": 1607593234007,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 164921,
            "range": "± 90487",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 650699,
            "range": "± 34790",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4716742,
            "range": "± 1296539",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18103512,
            "range": "± 1976302",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yusuktan@maguro.dev",
            "name": "Yusuke Tanaka",
            "username": "magurotuna"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4b1d76ec8f35052480eb14204d147df658bfdfdd",
          "message": "docs: fix typo in signal module documentation (#3249)",
          "timestamp": "2020-12-10T08:11:45-08:00",
          "tree_id": "46efd6f41cfaf702fb40c62b89800c511309d584",
          "url": "https://github.com/tokio-rs/tokio/commit/4b1d76ec8f35052480eb14204d147df658bfdfdd"
        },
        "date": 1607616823099,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 185295,
            "range": "± 24085",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 719594,
            "range": "± 88860",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5002499,
            "range": "± 825069",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19854087,
            "range": "± 3455690",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e",
          "message": "net: Pass SocketAddr by value (#3125)",
          "timestamp": "2020-12-10T14:58:27-05:00",
          "tree_id": "d46d58a79f31dba872aa060ef378743fcedea70e",
          "url": "https://github.com/tokio-rs/tokio/commit/16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e"
        },
        "date": 1607630430834,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 144781,
            "range": "± 19792",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 638851,
            "range": "± 35819",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4538210,
            "range": "± 1067709",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18219426,
            "range": "± 2328981",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "69e62ef89e481e0fb29ce3ef4ddce1eea4114000",
          "message": "sync: make TryAcquireError public (#3250)\n\nThe [`Semaphore::try_acquire`][1] method currently returns a private error type.\r\n\r\n[1]: https://docs.rs/tokio/0.3/tokio/sync/struct.Semaphore.html#method.try_acquire",
          "timestamp": "2020-12-10T19:56:05-08:00",
          "tree_id": "0784747565f6583a726c85dfedcd0527d8373cc6",
          "url": "https://github.com/tokio-rs/tokio/commit/69e62ef89e481e0fb29ce3ef4ddce1eea4114000"
        },
        "date": 1607659075974,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 196728,
            "range": "± 54731",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 736156,
            "range": "± 134368",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5474084,
            "range": "± 876121",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21384401,
            "range": "± 2777800",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cameron.evan@gmail.com",
            "name": "Evan Cameron",
            "username": "leshow"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "68717c7efaced76651915696495dcb04c890be25",
          "message": "net: remove empty udp module (#3260)",
          "timestamp": "2020-12-11T14:45:57-05:00",
          "tree_id": "1b7333194ac78d7ae87c5ca9f423ef830cb486b8",
          "url": "https://github.com/tokio-rs/tokio/commit/68717c7efaced76651915696495dcb04c890be25"
        },
        "date": 1607716039893,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 158153,
            "range": "± 4278",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 618844,
            "range": "± 141191",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4498500,
            "range": "± 571620",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 17725192,
            "range": "± 2698633",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b01b2dacf2e4136c0237977dac27a3688467d2ea",
          "message": "net: update `TcpStream::poll_peek` to use `ReadBuf` (#3259)\n\nCloses #2987",
          "timestamp": "2020-12-11T20:40:24-08:00",
          "tree_id": "1e0bbb86739731038cc9fd69fe112cad54662d16",
          "url": "https://github.com/tokio-rs/tokio/commit/b01b2dacf2e4136c0237977dac27a3688467d2ea"
        },
        "date": 1607748120154,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 158360,
            "range": "± 5448",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 627951,
            "range": "± 22427",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5104077,
            "range": "± 1265134",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18905967,
            "range": "± 2124366",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c1ec469ad2af883b001d54e81dad426c01f918cd",
          "message": "util: add constructors to TokioContext (#3221)",
          "timestamp": "2020-12-11T20:41:22-08:00",
          "tree_id": "cdb1273c1a4eea6c7175578bc8a13f417c3daf00",
          "url": "https://github.com/tokio-rs/tokio/commit/c1ec469ad2af883b001d54e81dad426c01f918cd"
        },
        "date": 1607748177513,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 157855,
            "range": "± 3120",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 614154,
            "range": "± 22418",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4519909,
            "range": "± 292079",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18126176,
            "range": "± 2487209",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sunjay@users.noreply.github.com",
            "name": "Sunjay Varma",
            "username": "sunjay"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "df20c162ae1308c07073b6a67c8ba4202f52d208",
          "message": "sync: add blocking_recv method to UnboundedReceiver, similar to Receiver::blocking_recv (#3262)",
          "timestamp": "2020-12-12T08:47:35-08:00",
          "tree_id": "94fe5abd9735b0c4985d5b38a8d96c51953b0f0b",
          "url": "https://github.com/tokio-rs/tokio/commit/df20c162ae1308c07073b6a67c8ba4202f52d208"
        },
        "date": 1607791762227,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 182885,
            "range": "± 14015",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 677832,
            "range": "± 70031",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5267872,
            "range": "± 739704",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21029742,
            "range": "± 3009567",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a",
          "message": "net: check for false-positives in TcpStream::ready doc test (#3255)",
          "timestamp": "2020-12-13T15:24:59+01:00",
          "tree_id": "89deb6d808e007e1728a43f0a198afe32a4aae1e",
          "url": "https://github.com/tokio-rs/tokio/commit/be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a"
        },
        "date": 1607869622153,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 204141,
            "range": "± 45437",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 754317,
            "range": "± 145312",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5028642,
            "range": "± 860987",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21271901,
            "range": "± 2073747",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40773946+faith@users.noreply.github.com",
            "name": "Aldas",
            "username": "faith"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f26f444f42c9369bcf8afe14ea00ea53dd663cc2",
          "message": "doc: added tracing to the feature flags section (#3254)",
          "timestamp": "2020-12-13T15:26:56+01:00",
          "tree_id": "23e7ae2599b67a6ab6faf0586f568b0ff6e0c72d",
          "url": "https://github.com/tokio-rs/tokio/commit/f26f444f42c9369bcf8afe14ea00ea53dd663cc2"
        },
        "date": 1607869712556,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 158749,
            "range": "± 2437",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 623561,
            "range": "± 20102",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4602012,
            "range": "± 408694",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 17709668,
            "range": "± 1885571",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4c5541945348c4614c29a13c06f2e996eb419e42",
          "message": "net: add UnixStream readiness and non-blocking ops (#3246)",
          "timestamp": "2020-12-13T16:21:11+01:00",
          "tree_id": "355917f9e8ba45094e1a3e1f465875f51f56e255",
          "url": "https://github.com/tokio-rs/tokio/commit/4c5541945348c4614c29a13c06f2e996eb419e42"
        },
        "date": 1607872973975,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 190502,
            "range": "± 14767",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 752208,
            "range": "± 259149",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5579648,
            "range": "± 685497",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21038292,
            "range": "± 2755332",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "6172808+skerkour@users.noreply.github.com",
            "name": "Sylvain Kerkour",
            "username": "skerkour"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9149d7bfae251289cd21aa9ee109b4e2a190d0fa",
          "message": "docs: mention blocking thread timeout in src/lib.rs (#3253)",
          "timestamp": "2020-12-13T16:24:16+01:00",
          "tree_id": "38b69f17cc4644ac6ca081aa1d88d5cfe35825fa",
          "url": "https://github.com/tokio-rs/tokio/commit/9149d7bfae251289cd21aa9ee109b4e2a190d0fa"
        },
        "date": 1607873162553,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 192909,
            "range": "± 62120",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 720879,
            "range": "± 58968",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5383073,
            "range": "± 1599410",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20830210,
            "range": "± 2732160",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jacob@jotpot.co.uk",
            "name": "Jacob O'Toole",
            "username": "JOT85"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1f862d2e950bf5f4fb31f202a544e86880f51191",
          "message": "sync: add `watch::Sender::borrow()` (#3269)",
          "timestamp": "2020-12-13T20:46:11-08:00",
          "tree_id": "e203a63415d1b02e7b62f73f8ebeebedeaaef82d",
          "url": "https://github.com/tokio-rs/tokio/commit/1f862d2e950bf5f4fb31f202a544e86880f51191"
        },
        "date": 1607921257933,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 156581,
            "range": "± 4747",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 627859,
            "range": "± 31671",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4758742,
            "range": "± 559274",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 17592883,
            "range": "± 2594088",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8efa62013b551d5130791c3a79ce8ab5cb0b5abf",
          "message": "Move stream items into `tokio-stream` (#3277)\n\nThis change removes all references to `Stream` from\r\nwithin the `tokio` crate and moves them into a new\r\n`tokio-stream` crate. Most types have had their\r\n`impl Stream` removed as well in-favor of their\r\ninherent methods.\r\n\r\nCloses #2870",
          "timestamp": "2020-12-15T20:24:38-08:00",
          "tree_id": "6da8c41c8e1808bea98fd2d23ee1ec03a1cc7e80",
          "url": "https://github.com/tokio-rs/tokio/commit/8efa62013b551d5130791c3a79ce8ab5cb0b5abf"
        },
        "date": 1608092769784,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 182019,
            "range": "± 15209",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 702811,
            "range": "± 43087",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5258133,
            "range": "± 636777",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20769514,
            "range": "± 3399957",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d74d17307dd53215061c4a8a1f20a0e30461e296",
          "message": "time: remove `Box` from `Sleep` (#3278)\n\nRemoves the box from `Sleep`, taking advantage of intrusive wakers. The\r\n`Sleep` future is now `!Unpin`.\r\n\r\nCloses #3267",
          "timestamp": "2020-12-16T21:51:34-08:00",
          "tree_id": "0cdbf57e4a9b38302ddae0078eb5a1b9a4977aa2",
          "url": "https://github.com/tokio-rs/tokio/commit/d74d17307dd53215061c4a8a1f20a0e30461e296"
        },
        "date": 1608184380207,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 142045,
            "range": "± 21525",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 595646,
            "range": "± 31087",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4495614,
            "range": "± 483148",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 17120102,
            "range": "± 2528035",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "59e4b35f49e6a95a961fd9003db58191de97d3a0",
          "message": "stream: Fix a few doc issues (#3285)",
          "timestamp": "2020-12-17T10:35:49-05:00",
          "tree_id": "757b133c0eac5b1c26d2ba870d0f4b5c198d7505",
          "url": "https://github.com/tokio-rs/tokio/commit/59e4b35f49e6a95a961fd9003db58191de97d3a0"
        },
        "date": 1608219442005,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 162297,
            "range": "± 24699",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 602706,
            "range": "± 62383",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4832604,
            "range": "± 1082051",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18568417,
            "range": "± 2421376",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8",
          "message": "docs: Add more comprehensive stream docs (#3286)\n\n* docs: Add more comprehensive stream docs\r\n\r\n* Apply suggestions from code review\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>\r\n\r\n* Fix doc tests\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-17T11:46:09-05:00",
          "tree_id": "f7afa84006c8629a0d2c058b8e52042c54436203",
          "url": "https://github.com/tokio-rs/tokio/commit/c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8"
        },
        "date": 1608223685399,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 182889,
            "range": "± 33852",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 670790,
            "range": "± 178916",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4696702,
            "range": "± 1032430",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19645796,
            "range": "± 2344371",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "abd4c0025539f142ec48a09e01430f7ee3b83214",
          "message": "time: enforce `current_thread` rt for time::pause (#3289)\n\nPausing time is a capability added to assist with testing Tokio code\r\ndependent on time. Currently, the capability implicitly requires the\r\ncurrent_thread runtime.\r\n\r\nThis change enforces the requirement by panicking if called from a\r\nmulti-threaded runtime.",
          "timestamp": "2020-12-17T15:37:08-08:00",
          "tree_id": "6c565d6c74dff336ac847cb6463245283d8470d5",
          "url": "https://github.com/tokio-rs/tokio/commit/abd4c0025539f142ec48a09e01430f7ee3b83214"
        },
        "date": 1608248331916,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 195853,
            "range": "± 8960",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 754607,
            "range": "± 56057",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5637116,
            "range": "± 764775",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21738311,
            "range": "± 2335831",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3ecaf9fd9a519da9b1decb4c7239b48277040522",
          "message": "codec: write documentation for codec (#3283)\n\nCo-authored-by: Lucio Franco <luciofranco14@gmail.com>",
          "timestamp": "2020-12-18T21:32:27+01:00",
          "tree_id": "f116db611cb8144b9ec3e2e97286aab59cdd9556",
          "url": "https://github.com/tokio-rs/tokio/commit/3ecaf9fd9a519da9b1decb4c7239b48277040522"
        },
        "date": 1608323636973,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 158828,
            "range": "± 5915",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 620849,
            "range": "± 17869",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4751364,
            "range": "± 608090",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19098313,
            "range": "± 2576013",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d948ccedfce534953a18acf46c8c6572103567c7",
          "message": "chore: fix stress test (#3297)",
          "timestamp": "2020-12-19T12:11:10+01:00",
          "tree_id": "3c417da4134a45bfff1f2d85b9b8cf410dfd9bf9",
          "url": "https://github.com/tokio-rs/tokio/commit/d948ccedfce534953a18acf46c8c6572103567c7"
        },
        "date": 1608376392232,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 199990,
            "range": "± 84079",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 717738,
            "range": "± 104746",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5247837,
            "range": "± 387636",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21456208,
            "range": "± 2364932",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b99b00eb302ae6ff19ca97d32b1e594143f43a60",
          "message": "rt: change `max_threads` to `max_blocking_threads` (#3287)\n\nFixes #2802",
          "timestamp": "2020-12-19T08:04:04-08:00",
          "tree_id": "458d7fb55f921184a1056e766b6d0101fb763579",
          "url": "https://github.com/tokio-rs/tokio/commit/b99b00eb302ae6ff19ca97d32b1e594143f43a60"
        },
        "date": 1608393958023,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 243505,
            "range": "± 104057",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 767542,
            "range": "± 147011",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5256694,
            "range": "± 1271588",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21856931,
            "range": "± 3304578",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "flo.huebsch@pm.me",
            "name": "Florian Hübsch",
            "username": "fl9"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e41e6cddbba0cf13403924937ffe02aae6639e28",
          "message": "docs: tokio::main macro is also supported on rt (#3243)\n\nFixes: #3144\r\nRefs: #2225",
          "timestamp": "2020-12-19T19:12:08+01:00",
          "tree_id": "ee1af0c8a3b2ab9c9eaae05f2dca96ff966f42f9",
          "url": "https://github.com/tokio-rs/tokio/commit/e41e6cddbba0cf13403924937ffe02aae6639e28"
        },
        "date": 1608401632891,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 161661,
            "range": "± 15750",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 621301,
            "range": "± 52216",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4860943,
            "range": "± 996033",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18642677,
            "range": "± 3122913",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "78f2340d259487d4470681f97cc4b5eac719e178",
          "message": "tokio: remove prelude (#3299)\n\nCloses: #3257",
          "timestamp": "2020-12-19T11:42:24-08:00",
          "tree_id": "2a093e994a41bae54e324fd8d20c836e46a5947c",
          "url": "https://github.com/tokio-rs/tokio/commit/78f2340d259487d4470681f97cc4b5eac719e178"
        },
        "date": 1608407043784,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 165174,
            "range": "± 55262",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 573017,
            "range": "± 141343",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4436346,
            "range": "± 1201818",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 17118339,
            "range": "± 3425792",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "arve.knudsen@gmail.com",
            "name": "Arve Knudsen",
            "username": "aknuds1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1b70507894035e066cb488c14ff328bd47ca696d",
          "message": "net: remove {Tcp,Unix}Stream::shutdown() (#3298)\n\n`shutdown()` on `AsyncWrite` performs a TCP shutdown. This avoids method\r\nconflicts.\r\n\r\nCloses #3294",
          "timestamp": "2020-12-19T12:17:52-08:00",
          "tree_id": "dd3c414761b8a3713b63515e7666f108a1391743",
          "url": "https://github.com/tokio-rs/tokio/commit/1b70507894035e066cb488c14ff328bd47ca696d"
        },
        "date": 1608409199359,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 191351,
            "range": "± 24291",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 719065,
            "range": "± 130878",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5412410,
            "range": "± 1050325",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20823708,
            "range": "± 2690528",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e5f513542c0f92f49f6acd31021cc056577be6b",
          "message": "chore: remove some left over `stream` feature code (#3300)\n\nRemoves the `stream` feature flag from `Cargo.toml` and removes the\r\n`futures-core` dependency. Once `Stream` lands in `std`, a feature flag\r\nis most likely not needed.",
          "timestamp": "2020-12-19T14:15:00-08:00",
          "tree_id": "bb5486367eec9a81b5d021202eaf8ab0fe34bdc8",
          "url": "https://github.com/tokio-rs/tokio/commit/5e5f513542c0f92f49f6acd31021cc056577be6b"
        },
        "date": 1608416211574,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 181684,
            "range": "± 15147",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 680427,
            "range": "± 92112",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4265103,
            "range": "± 547094",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19154589,
            "range": "± 2263951",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "28933599888a88e601acbb11fa824b0ee9f98c6e",
          "message": "chore: update to `bytes` 1.0 git branch (#3301)\n\nUpdates the code base to track the `bytes` git branch. This is in\r\npreparation for the 1.0 release.\r\n\r\nCloses #3058",
          "timestamp": "2020-12-19T15:57:16-08:00",
          "tree_id": "2021ef3acf9407fcfa39032e0a493a81f1eb74cc",
          "url": "https://github.com/tokio-rs/tokio/commit/28933599888a88e601acbb11fa824b0ee9f98c6e"
        },
        "date": 1608422321820,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 159621,
            "range": "± 5535",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 619702,
            "range": "± 16279",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4989644,
            "range": "± 1176815",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18648881,
            "range": "± 1982532",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bIgBV@users.noreply.github.com",
            "name": "Bhargav",
            "username": "bIgBV"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c7671a03840751f58b7a509386b8fe3b5e670a37",
          "message": "io: add _mut variants of methods on AsyncFd (#3304)\n\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-21T22:51:28+01:00",
          "tree_id": "9d3f26151c9ddd292c129f43c0fac83792073f62",
          "url": "https://github.com/tokio-rs/tokio/commit/c7671a03840751f58b7a509386b8fe3b5e670a37"
        },
        "date": 1608587595690,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 197920,
            "range": "± 36460",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 755605,
            "range": "± 130143",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5526396,
            "range": "± 1270826",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21887696,
            "range": "± 2805501",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "564c943309f0a94474e2e71a115ec0bb45f3e7fd",
          "message": "io: rename `AsyncFd::with_io()` and rm `with_poll()` (#3306)",
          "timestamp": "2020-12-21T15:42:38-08:00",
          "tree_id": "6f29310dda04438222fb9581b3cd9581f1abe13e",
          "url": "https://github.com/tokio-rs/tokio/commit/564c943309f0a94474e2e71a115ec0bb45f3e7fd"
        },
        "date": 1608594248997,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 195077,
            "range": "± 9945",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 733184,
            "range": "± 41575",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5850434,
            "range": "± 1013071",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21588590,
            "range": "± 2241011",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "eee3ca65d6e5b4537915571f663f94184e616e6c",
          "message": "deps: update rand to 0.8, loom to 0.4 (#3307)",
          "timestamp": "2020-12-22T10:28:35+01:00",
          "tree_id": "2f534ebe6cb319f30f72e7a9b2b389825500f051",
          "url": "https://github.com/tokio-rs/tokio/commit/eee3ca65d6e5b4537915571f663f94184e616e6c"
        },
        "date": 1608629417210,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 188655,
            "range": "± 23492",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 688605,
            "range": "± 73569",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5583676,
            "range": "± 1035611",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20280743,
            "range": "± 2190147",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f95ad1898041d2bcc09de3db69228e88f5f4abf8",
          "message": "net: clarify when wakeups are sent (#3310)",
          "timestamp": "2020-12-22T07:38:14-08:00",
          "tree_id": "16968a03d8ca55c53da7658c109e47a5f9a8c4e4",
          "url": "https://github.com/tokio-rs/tokio/commit/f95ad1898041d2bcc09de3db69228e88f5f4abf8"
        },
        "date": 1608651600318,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 181929,
            "range": "± 49366",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 695353,
            "range": "± 226320",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5331051,
            "range": "± 1226220",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19931429,
            "range": "± 2740151",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b83b3b8cc61e9d911abec00c33ec3762b2a6437",
          "message": "fs,sync: expose poll_ fns on misc types (#3308)\n\nIncludes methods on:\r\n\r\n* fs::DirEntry\r\n* io::Lines\r\n* io::Split\r\n* sync::mpsc::Receiver\r\n* sync::misc::UnboundedReceiver",
          "timestamp": "2020-12-22T09:28:14-08:00",
          "tree_id": "bd1ae38a53efb2fe78fc8aba0bdd0789cc7a1495",
          "url": "https://github.com/tokio-rs/tokio/commit/0b83b3b8cc61e9d911abec00c33ec3762b2a6437"
        },
        "date": 1608658195243,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 190751,
            "range": "± 25444",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 722817,
            "range": "± 165554",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5538952,
            "range": "± 573569",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20765315,
            "range": "± 2461635",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "carlos.baezruiz@gmail.com",
            "name": "Carlos B",
            "username": "carlosb1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d28e4cdbb63b0ad19e66d1b077690cbebf71353",
          "message": "examples: add futures executor threadpool (#3198)",
          "timestamp": "2020-12-22T20:08:43+01:00",
          "tree_id": "706d894cb4ea97ae1a91b9d2bd42d1800968559c",
          "url": "https://github.com/tokio-rs/tokio/commit/7d28e4cdbb63b0ad19e66d1b077690cbebf71353"
        },
        "date": 1608664239639,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 199562,
            "range": "± 93074",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 740136,
            "range": "± 310486",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5297487,
            "range": "± 1950839",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21586341,
            "range": "± 3863494",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608664541945,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 224482,
            "range": "± 93279",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 785738,
            "range": "± 248474",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5012529,
            "range": "± 947574",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 22020986,
            "range": "± 3996062",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608666638638,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 197742,
            "range": "± 45242",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 718741,
            "range": "± 139836",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5067658,
            "range": "± 1298948",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20354969,
            "range": "± 2337048",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b",
          "message": "time: make Interval::poll_tick() public (#3316)",
          "timestamp": "2020-12-22T12:31:14-08:00",
          "tree_id": "c06c2c6a1618d8dd177cd844f8f816f06e6033b8",
          "url": "https://github.com/tokio-rs/tokio/commit/be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b"
        },
        "date": 1608669184375,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 196660,
            "range": "± 20686",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 716374,
            "range": "± 54785",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5349583,
            "range": "± 643724",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21219917,
            "range": "± 2537162",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luke.steensen@gmail.com",
            "name": "Luke Steensen",
            "username": "lukesteensen"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8dda19da4e94acd45f34b5eb359b4cffafa833f",
          "message": "chore: update to released `bytes` 1.0 (#3317)",
          "timestamp": "2020-12-22T17:09:26-08:00",
          "tree_id": "c177db0f9bced11086bcb13be4ac2348e6c94469",
          "url": "https://github.com/tokio-rs/tokio/commit/a8dda19da4e94acd45f34b5eb359b4cffafa833f"
        },
        "date": 1608685874602,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 186861,
            "range": "± 36573",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 702141,
            "range": "± 110984",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 4696115,
            "range": "± 987851",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 18194390,
            "range": "± 2636139",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0deaeb84948f253b76b7fe64d7fe9d4527cd4275",
          "message": "chore: remove unused `slab` dependency (#3318)",
          "timestamp": "2020-12-22T21:56:22-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/0deaeb84948f253b76b7fe64d7fe9d4527cd4275"
        },
        "date": 1608703080818,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 186034,
            "range": "± 11793",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 703851,
            "range": "± 45313",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5387275,
            "range": "± 1022826",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 20839245,
            "range": "± 2445985",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "575938d4579e6fe6a89b700aadb0ae2bbab5483b",
          "message": "chore: use #[non_exhaustive] instead of private unit field (#3320)",
          "timestamp": "2020-12-23T22:48:33+09:00",
          "tree_id": "f2782c6135a26568d5dea4d45b3310baaf062e16",
          "url": "https://github.com/tokio-rs/tokio/commit/575938d4579e6fe6a89b700aadb0ae2bbab5483b"
        },
        "date": 1608731406249,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 197705,
            "range": "± 40639",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 756343,
            "range": "± 200857",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5353855,
            "range": "± 1181892",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21733507,
            "range": "± 3139343",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ce0e9c67cfe61c7a91a284331ecc53fa01c32879",
          "message": "chore: Revert \"use #[non_exhaustive] instead of private unit field\" (#3323)\n\nThis reverts commit 575938d4579e6fe6a89b700aadb0ae2bbab5483b.",
          "timestamp": "2020-12-23T08:27:58-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/ce0e9c67cfe61c7a91a284331ecc53fa01c32879"
        },
        "date": 1608740990085,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 199759,
            "range": "± 48862",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 752516,
            "range": "± 141899",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5415427,
            "range": "± 690792",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 21358385,
            "range": "± 2227081",
            "unit": "ns/iter"
          }
        ]
      }
    ],
    "sync_semaphore": [
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "distinct": true,
          "id": "af032dbf59195f5a637c14fd8805f45cce8c8563",
          "message": "try again",
          "timestamp": "2020-11-13T16:47:49-08:00",
          "tree_id": "2c351a9bf2bc6d1fb70754ee19640da3b69df204",
          "url": "https://github.com/tokio-rs/tokio/commit/af032dbf59195f5a637c14fd8805f45cce8c8563"
        },
        "date": 1605314979788,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13595,
            "range": "± 2805",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 916,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 548,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13431,
            "range": "± 2349",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 916,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "97c2c4203cd7c42960cac895987c43a17dff052e",
          "message": "chore: automate running benchmarks (#3140)\n\nUses Github actions to run benchmarks.",
          "timestamp": "2020-11-13T19:30:52-08:00",
          "tree_id": "f4a3cfebafb7afee68d6d4de1748daddcfc070c6",
          "url": "https://github.com/tokio-rs/tokio/commit/97c2c4203cd7c42960cac895987c43a17dff052e"
        },
        "date": 1605324754941,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13492,
            "range": "± 3133",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 950,
            "range": "± 224",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 611,
            "range": "± 109",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13849,
            "range": "± 3588",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 956,
            "range": "± 196",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "masnagam@gmail.com",
            "name": "masnagam",
            "username": "masnagam"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4e39c9b818eb8af064bb9f45f47e3cfc6593de95",
          "message": "net: restore TcpStream::{poll_read_ready, poll_write_ready} (#2743)",
          "timestamp": "2020-11-16T09:51:06-08:00",
          "tree_id": "2222dd2f8638fb64f228badef84814d2f4079a82",
          "url": "https://github.com/tokio-rs/tokio/commit/4e39c9b818eb8af064bb9f45f47e3cfc6593de95"
        },
        "date": 1605549191489,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15074,
            "range": "± 3954",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1104,
            "range": "± 143",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 639,
            "range": "± 131",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15715,
            "range": "± 5843",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1077,
            "range": "± 260",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f5cb4c20422a35b51bfba3391744f8bcb54f7581",
          "message": "net: Add send/recv buf size methods to `TcpSocket` (#3145)\n\nThis commit adds `set_{send, recv}_buffer_size` methods to `TcpSocket`\r\nfor setting the size of the TCP send and receive buffers, and `{send,\r\nrecv}_buffer_size` methods for returning the current value. These just\r\ncall into similar methods on `mio`'s `TcpSocket` type, which were added\r\nin tokio-rs/mio#1384.\r\n\r\nRefs: #3082\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-11-16T12:29:03-08:00",
          "tree_id": "fcf642984a21d04533efad0cdde613d294635c4d",
          "url": "https://github.com/tokio-rs/tokio/commit/f5cb4c20422a35b51bfba3391744f8bcb54f7581"
        },
        "date": 1605558672781,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16250,
            "range": "± 6549",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1148,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 642,
            "range": "± 105",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16877,
            "range": "± 5879",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1120,
            "range": "± 177",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "zaharidichev@gmail.com",
            "name": "Zahari Dichev",
            "username": "zaharidichev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d0ebb4154748166a4ba07baa4b424a1c45efd219",
          "message": "sync: add `Notify::notify_waiters` (#3098)\n\nThis PR makes `Notify::notify_waiters` public. The method\r\nalready exists, but it changes the way `notify_waiters`,\r\nis used. Previously in order for the consumer to\r\nregister interest, in a notification triggered by\r\n`notify_waiters`, the `Notified` future had to be\r\npolled. This introduced friction when using the api\r\nas the future had to be pinned before polled.\r\n\r\nThis change introduces a counter that tracks how many\r\ntimes `notified_waiters` has been called. Upon creation of\r\nthe future the number of times is loaded. When first\r\npolled the future compares this number with the count\r\nstate of the `Notify` type. This avoids the need for\r\nregistering the waiter upfront.\r\n\r\nFixes: #3066",
          "timestamp": "2020-11-16T12:49:35-08:00",
          "tree_id": "5ea4d611256290f62baea1a9ffa3333b254181df",
          "url": "https://github.com/tokio-rs/tokio/commit/d0ebb4154748166a4ba07baa4b424a1c45efd219"
        },
        "date": 1605559887160,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14761,
            "range": "± 6659",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1111,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 572,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14899,
            "range": "± 6462",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1040,
            "range": "± 180",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0ea23076503c5151d68a781a3d91823396c82949",
          "message": "net: add UdpSocket readiness and non-blocking ops (#3138)\n\nAdds `ready()`, `readable()`, and `writable()` async methods for waiting\r\nfor socket readiness. Adds `try_send`, `try_send_to`, `try_recv`, and\r\n`try_recv_from` for performing non-blocking operations on the socket.\r\n\r\nThis is the UDP equivalent of #3130.",
          "timestamp": "2020-11-16T15:44:01-08:00",
          "tree_id": "1e49d7dc0bb3cee6271133d942ba49c5971fde29",
          "url": "https://github.com/tokio-rs/tokio/commit/0ea23076503c5151d68a781a3d91823396c82949"
        },
        "date": 1605570353023,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 17403,
            "range": "± 5100",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1046,
            "range": "± 157",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 596,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 17072,
            "range": "± 5572",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1016,
            "range": "± 97",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "9832640+zekisherif@users.noreply.github.com",
            "name": "Zeki Sherif",
            "username": "zekisherif"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d11aa866837eea50a6f1e0ef7e24846a653cbf1",
          "message": "net: add SO_LINGER get/set to TcpStream (#3143)",
          "timestamp": "2020-11-17T09:58:00-08:00",
          "tree_id": "ca0d5edc04a29bbe6e2906c760a22908e032a4c9",
          "url": "https://github.com/tokio-rs/tokio/commit/7d11aa866837eea50a6f1e0ef7e24846a653cbf1"
        },
        "date": 1605635995329,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14052,
            "range": "± 2557",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1066,
            "range": "± 100",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 635,
            "range": "± 40",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14784,
            "range": "± 3824",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1064,
            "range": "± 97",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "34fcef258b84d17f8d418b39eb61fa07fa87c390",
          "message": "io: add vectored writes to `AsyncWrite` (#3149)\n\nThis adds `AsyncWrite::poll_write_vectored`, and implements it for\r\n`TcpStream` and `UnixStream`.\r\n\r\nRefs: #3135.",
          "timestamp": "2020-11-18T10:41:47-08:00",
          "tree_id": "98e37fc2d6fa541a9e499331df86ba3d1b7b6e3a",
          "url": "https://github.com/tokio-rs/tokio/commit/34fcef258b84d17f8d418b39eb61fa07fa87c390"
        },
        "date": 1605725013037,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16289,
            "range": "± 6362",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1138,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 641,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14883,
            "range": "± 4548",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1141,
            "range": "± 5",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "479c545c20b2cb44a8f09600733adc8c8dcb5aa0",
          "message": "chore: prepare v0.3.4 release (#3152)",
          "timestamp": "2020-11-18T12:38:13-08:00",
          "tree_id": "df6daba6b2f595de47ada2dd2f518475669ab919",
          "url": "https://github.com/tokio-rs/tokio/commit/479c545c20b2cb44a8f09600733adc8c8dcb5aa0"
        },
        "date": 1605732024626,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15022,
            "range": "± 5746",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1023,
            "range": "± 95",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 610,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14804,
            "range": "± 4029",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1050,
            "range": "± 109",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84",
          "message": "util: fix typo in udp/frame.rs (#3154)",
          "timestamp": "2020-11-20T15:06:14+09:00",
          "tree_id": "f09954c70e26336bdb1bc525f832916c2d7037bf",
          "url": "https://github.com/tokio-rs/tokio/commit/49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84"
        },
        "date": 1605852504046,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15976,
            "range": "± 6358",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1140,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 657,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16059,
            "range": "± 4105",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1126,
            "range": "± 127",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f927f01a34d7cedf0cdc820f729a7a6cd56e83dd",
          "message": "macros: fix rustfmt on 1.48.0 (#3160)\n\n## Motivation\r\n\r\nLooks like the Rust 1.48.0 version of `rustfmt` changed some formatting\r\nrules (fixed some bugs?), and some of the code in `tokio-macros` is no\r\nlonger correctly formatted. This is breaking CI.\r\n\r\n## Solution\r\n\r\nThis commit runs rustfmt on Rust 1.48.0. This fixes CI.\r\n\r\nCloses #3158",
          "timestamp": "2020-11-20T10:19:26-08:00",
          "tree_id": "bd0243a653ee49cfc50bf61b00a36cc0fce6a414",
          "url": "https://github.com/tokio-rs/tokio/commit/f927f01a34d7cedf0cdc820f729a7a6cd56e83dd"
        },
        "date": 1605896460411,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 19438,
            "range": "± 3548",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 774,
            "range": "± 232",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 504,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 18618,
            "range": "± 5481",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 886,
            "range": "± 362",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ae67851f11b7cc1f577de8ce21767ce3e2c7aff9",
          "message": "time: use intrusive lists for timer tracking (#3080)\n\nMore-or-less a half-rewrite of the current time driver, supporting the\r\nuse of intrusive futures for timer registration.\r\n\r\nFixes: #3028, #3069",
          "timestamp": "2020-11-23T10:42:50-08:00",
          "tree_id": "be43cb76333b0e9e42a101d659f9b2e41555d779",
          "url": "https://github.com/tokio-rs/tokio/commit/ae67851f11b7cc1f577de8ce21767ce3e2c7aff9"
        },
        "date": 1606157091611,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 17111,
            "range": "± 8658",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1156,
            "range": "± 114",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 665,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16969,
            "range": "± 6320",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1131,
            "range": "± 108",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "driftluo@foxmail.com",
            "name": "漂流",
            "username": "driftluo"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "874fc3320bc000fee20d63b3ad865a1145122640",
          "message": "codec: add read_buffer_mut to FramedRead (#3166)",
          "timestamp": "2020-11-24T09:39:16+01:00",
          "tree_id": "53540b744f6a915cedc1099afe1b0639443b2436",
          "url": "https://github.com/tokio-rs/tokio/commit/874fc3320bc000fee20d63b3ad865a1145122640"
        },
        "date": 1606207245207,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15902,
            "range": "± 5794",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1117,
            "range": "± 220",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 651,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15835,
            "range": "± 4875",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1096,
            "range": "± 196",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "de33ee85ce61377b316b630e4355d419cc4abcb7",
          "message": "time: replace 'ouClockTimeide' in internal docs with 'outside' (#3171)",
          "timestamp": "2020-11-24T10:23:20+01:00",
          "tree_id": "5ed85f95ea1846983471a11fe555328e6b0f5f6f",
          "url": "https://github.com/tokio-rs/tokio/commit/de33ee85ce61377b316b630e4355d419cc4abcb7"
        },
        "date": 1606209884612,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14402,
            "range": "± 2947",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 907,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 542,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14025,
            "range": "± 2005",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 906,
            "range": "± 3",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rajiv.chauhan@gmail.com",
            "name": "Rajiv Chauhan",
            "username": "chauhraj"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e406a7a47699d93fa2a77fb72553600cb7abd0f",
          "message": "macros: fix outdated documentation (#3180)\n\n1. Changed 0.2 to 0.3\r\n2. Changed ‘multi’ to ‘single’ to indicate that the behavior is single threaded",
          "timestamp": "2020-11-26T19:46:15+01:00",
          "tree_id": "ac6898684e4b84e4a5d0e781adf42d950bbc9e43",
          "url": "https://github.com/tokio-rs/tokio/commit/5e406a7a47699d93fa2a77fb72553600cb7abd0f"
        },
        "date": 1606416464760,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15834,
            "range": "± 4146",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1092,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 655,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16618,
            "range": "± 5810",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1091,
            "range": "± 7",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "niklas.fiekas@backscattering.de",
            "name": "Niklas Fiekas",
            "username": "niklasf"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49129434198a96444bc0e9582a14062d3a46e93a",
          "message": "signal: expose CtrlC stream on windows (#3186)\n\n* Make tokio::signal::windows::ctrl_c() public.\r\n* Stop referring to private tokio::signal::windows::Event in module\r\n  documentation.\r\n\r\nCloses #3178",
          "timestamp": "2020-11-27T19:53:17Z",
          "tree_id": "904fb6b1fb539bffe69168c7202ccc3db15321dc",
          "url": "https://github.com/tokio-rs/tokio/commit/49129434198a96444bc0e9582a14062d3a46e93a"
        },
        "date": 1606506881117,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13640,
            "range": "± 2386",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 902,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 548,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13641,
            "range": "± 2296",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 905,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0acd06b42a9d1461302388f2a533e86d391d6040",
          "message": "runtime: fix shutdown_timeout(0) blocking (#3174)",
          "timestamp": "2020-11-28T19:31:13+01:00",
          "tree_id": "c17e5d58e10ee419e492cb831843c3f08e1f66d8",
          "url": "https://github.com/tokio-rs/tokio/commit/0acd06b42a9d1461302388f2a533e86d391d6040"
        },
        "date": 1606588375385,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15036,
            "range": "± 3786",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 939,
            "range": "± 165",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 588,
            "range": "± 95",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14808,
            "range": "± 4324",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 933,
            "range": "± 160",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c55d846f4b248b4a72335d6c57829fa6396ab9a5",
          "message": "util: add rt to tokio-util full feature (#3194)",
          "timestamp": "2020-11-29T09:48:31+01:00",
          "tree_id": "5f27b29cd1018796f0713d6e87e4823920ba5084",
          "url": "https://github.com/tokio-rs/tokio/commit/c55d846f4b248b4a72335d6c57829fa6396ab9a5"
        },
        "date": 1606639818278,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14149,
            "range": "± 4588",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1030,
            "range": "± 174",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 585,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15698,
            "range": "± 5583",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 965,
            "range": "± 175",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kylekosic@gmail.com",
            "name": "Kyle Kosic",
            "username": "kykosic"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a85fdb884d961bb87a2f3d446c548802868e54cb",
          "message": "runtime: test for shutdown_timeout(0) (#3196)",
          "timestamp": "2020-11-29T21:30:19+01:00",
          "tree_id": "c554597c6596dc6eddc98bfafcc512361ddb5f31",
          "url": "https://github.com/tokio-rs/tokio/commit/a85fdb884d961bb87a2f3d446c548802868e54cb"
        },
        "date": 1606681917649,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14258,
            "range": "± 3153",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 953,
            "range": "± 109",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 573,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14104,
            "range": "± 2341",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 975,
            "range": "± 134",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pickfire@riseup.net",
            "name": "Ivan Tham",
            "username": "pickfire"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "72d6346c0d43d867002dc0cc5527fbd0b0e23c3f",
          "message": "macros: #[tokio::main] can be used on non-main (#3199)",
          "timestamp": "2020-11-30T17:34:11+01:00",
          "tree_id": "c558d1cb380cc67bfc56ea960a7d9e266259367a",
          "url": "https://github.com/tokio-rs/tokio/commit/72d6346c0d43d867002dc0cc5527fbd0b0e23c3f"
        },
        "date": 1606754153608,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14953,
            "range": "± 2824",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1152,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 673,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14743,
            "range": "± 2883",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1153,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "73653352+HK416-is-all-you-need@users.noreply.github.com",
            "name": "HK416-is-all-you-need",
            "username": "HK416-is-all-you-need"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5",
          "message": "io: add AsyncFd::with_interest (#3167)\n\nFixes #3072",
          "timestamp": "2020-11-30T11:11:18-08:00",
          "tree_id": "45e9d190af02ab0cdc92c317e3127a1b8227ac3a",
          "url": "https://github.com/tokio-rs/tokio/commit/7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5"
        },
        "date": 1606763577482,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15284,
            "range": "± 3441",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1080,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 646,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14948,
            "range": "± 4389",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1082,
            "range": "± 5",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "08548583b948a0be04338f1b1462917c001dbf4a",
          "message": "chore: prepare v0.3.5 release (#3201)",
          "timestamp": "2020-11-30T12:57:31-08:00",
          "tree_id": "bc964338ba8d03930d53192a1e2288132330ff97",
          "url": "https://github.com/tokio-rs/tokio/commit/08548583b948a0be04338f1b1462917c001dbf4a"
        },
        "date": 1606770001323,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 17953,
            "range": "± 8522",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1081,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 673,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 18201,
            "range": "± 11942",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1154,
            "range": "± 593",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "128495168d390092df2cb8ae8577cfec09f666ff",
          "message": "ci: switch FreeBSD CI environment to 12.2-RELEASE (#3202)\n\n12.1 will be EoL in two months.",
          "timestamp": "2020-12-01T10:19:54+09:00",
          "tree_id": "2a289d5667b3ffca2ebfb747785c380ee7eac034",
          "url": "https://github.com/tokio-rs/tokio/commit/128495168d390092df2cb8ae8577cfec09f666ff"
        },
        "date": 1606785692303,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15125,
            "range": "± 3614",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1129,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 675,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15523,
            "range": "± 5305",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1154,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "353b0544a04214e7d6e828641e2045df1d97cda8",
          "message": "ci: reenable CI on FreeBSD i686 (#3204)\n\nIt was temporarily disabled in 06c473e62842d257ed275497ce906710ea3f8e19\r\nand never reenabled.",
          "timestamp": "2020-12-01T10:20:18+09:00",
          "tree_id": "468f282ba9f5116f5ed9a81abacbb7385aaa9c1e",
          "url": "https://github.com/tokio-rs/tokio/commit/353b0544a04214e7d6e828641e2045df1d97cda8"
        },
        "date": 1606785737612,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15717,
            "range": "± 4442",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1225,
            "range": "± 234",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 577,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16282,
            "range": "± 4916",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1205,
            "range": "± 206",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ae8135b62057be6b1691f04b27eabe285b05efd",
          "message": "process: fix the process_kill_on_drop.rs test on non-Linux systems (#3203)\n\n\"disown\" is a bash builtin, not part of POSIX sh.",
          "timestamp": "2020-12-01T10:20:49+09:00",
          "tree_id": "8b211b0f9807692d77be8a64a4835718355afe7b",
          "url": "https://github.com/tokio-rs/tokio/commit/7ae8135b62057be6b1691f04b27eabe285b05efd"
        },
        "date": 1606785747978,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14082,
            "range": "± 4841",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 955,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 565,
            "range": "± 212",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14064,
            "range": "± 5000",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 943,
            "range": "± 106",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8e0f0a919663b210627c132d6af3e19a95d8037",
          "message": "example: add back udp-codec example (#3205)",
          "timestamp": "2020-12-01T12:20:20+09:00",
          "tree_id": "b18851ef95641ab2e2d1f632e2ce39cb1fcb1301",
          "url": "https://github.com/tokio-rs/tokio/commit/a8e0f0a919663b210627c132d6af3e19a95d8037"
        },
        "date": 1606792909506,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13991,
            "range": "± 2550",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 909,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 542,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13986,
            "range": "± 2289",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 910,
            "range": "± 7",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a6051a61ec5c96113f4b543de3ec55431695347a",
          "message": "sync: make add_permits panic with usize::MAX >> 3 permits (#3188)",
          "timestamp": "2020-12-02T22:58:28+01:00",
          "tree_id": "1a4d4bcc017f6a61a652505b1edd4a3bf36ea1ab",
          "url": "https://github.com/tokio-rs/tokio/commit/a6051a61ec5c96113f4b543de3ec55431695347a"
        },
        "date": 1606946417755,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 17118,
            "range": "± 6814",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1088,
            "range": "± 271",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 650,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15594,
            "range": "± 6244",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1097,
            "range": "± 162",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "647299866a2262c8a1183adad73673e5803293ed",
          "message": "util: add writev-aware `poll_write_buf` (#3156)\n\n## Motivation\r\n\r\nIn Tokio 0.2, `AsyncRead` and `AsyncWrite` had `poll_write_buf` and\r\n`poll_read_buf` methods for reading and writing to implementers of\r\n`bytes` `Buf` and `BufMut` traits. In 0.3, these were removed, but\r\n`poll_read_buf` was added as a free function in `tokio-util`. However,\r\nthere is currently no `poll_write_buf`.\r\n\r\nNow that `AsyncWrite` has regained support for vectored writes in #3149,\r\nthere's a lot of potential benefit in having a `poll_write_buf` that\r\nuses vectored writes when supported and non-vectored writes when not\r\nsupported, so that users don't have to reimplement this.\r\n\r\n## Solution\r\n\r\nThis PR adds a `poll_write_buf` function to `tokio_util::io`, analogous\r\nto the existing `poll_read_buf` function.\r\n\r\nThis function writes from a `Buf` to an `AsyncWrite`, advancing the\r\n`Buf`'s internal cursor. In addition, when the `AsyncWrite` supports\r\nvectored writes (i.e. its `is_write_vectored` method returns `true`),\r\nit will use vectored IO.\r\n\r\nI copied the documentation for this functions from the docs from Tokio\r\n0.2's `AsyncWrite::poll_write_buf` , with some minor modifications as\r\nappropriate.\r\n\r\nFinally, I fixed a minor issue in the existing docs for `poll_read_buf`\r\nand `read_buf`, and updated `tokio_util::codec` to use `poll_write_buf`.\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-12-03T11:19:16-08:00",
          "tree_id": "c92df9ae491f0a444e694879858d032c3f6a5373",
          "url": "https://github.com/tokio-rs/tokio/commit/647299866a2262c8a1183adad73673e5803293ed"
        },
        "date": 1607023263628,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14796,
            "range": "± 3135",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1024,
            "range": "± 193",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 607,
            "range": "± 103",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14316,
            "range": "± 4369",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1025,
            "range": "± 118",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "00500d1b35f00c68117d8f4e7320303e967e92e3",
          "message": "util: prepare v0.5.1 release (#3210)\n\n### Added\r\n\r\n- io: `poll_read_buf` util fn (#2972).\r\n- io: `poll_write_buf` util fn with vectored write support (#3156).\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-12-03T15:30:52-08:00",
          "tree_id": "fe18e0f55daa4f26cf53bfe42a713338ac5460d9",
          "url": "https://github.com/tokio-rs/tokio/commit/00500d1b35f00c68117d8f4e7320303e967e92e3"
        },
        "date": 1607038356780,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15173,
            "range": "± 4035",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1103,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 652,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15751,
            "range": "± 4170",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1080,
            "range": "± 51",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "razican@protonmail.ch",
            "name": "Iban Eguia",
            "username": "Razican"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0dbba139848de6a8ee88350cc7fc48d0b05016c5",
          "message": "deps: replace lazy_static with once_cell (#3187)",
          "timestamp": "2020-12-04T10:23:13+01:00",
          "tree_id": "73f3366b9c7a0c50d6dd146a2626368cf59b3178",
          "url": "https://github.com/tokio-rs/tokio/commit/0dbba139848de6a8ee88350cc7fc48d0b05016c5"
        },
        "date": 1607073907823,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14450,
            "range": "± 2918",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 964,
            "range": "± 90",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 580,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14253,
            "range": "± 2706",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 937,
            "range": "± 206",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "liufuyang@users.noreply.github.com",
            "name": "Fuyang Liu",
            "username": "liufuyang"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0707f4c19210d6dac620c663e94d34834714a7c9",
          "message": "net: add TcpStream::into_std (#3189)",
          "timestamp": "2020-12-06T14:33:04+01:00",
          "tree_id": "a3aff2f279b1e560602b4752435e092b4a22424e",
          "url": "https://github.com/tokio-rs/tokio/commit/0707f4c19210d6dac620c663e94d34834714a7c9"
        },
        "date": 1607261697161,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 17257,
            "range": "± 6779",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1118,
            "range": "± 150",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 664,
            "range": "± 169",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16924,
            "range": "± 7833",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1147,
            "range": "± 235",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "62023dffe5396ee1a0380f12c7530bf4ff59fe4a",
          "message": "sync: forward port 0.2 mpsc test (#3225)\n\nForward ports the test included in #3215. The mpsc sempahore has been\r\nreplaced in 0.3 and does not include the bug being fixed.",
          "timestamp": "2020-12-07T11:24:15-08:00",
          "tree_id": "c891a48ce299e6cfd01090a880d1baf16ebe0ad7",
          "url": "https://github.com/tokio-rs/tokio/commit/62023dffe5396ee1a0380f12c7530bf4ff59fe4a"
        },
        "date": 1607369157814,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14834,
            "range": "± 4014",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1089,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 648,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15229,
            "range": "± 5894",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1090,
            "range": "± 8",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "57dffb9dfe9e4c0f12429246540add3975f4a754",
          "message": "rt: fix deadlock in shutdown (#3228)\n\nPreviously, the runtime shutdown logic would first-hand control over all cores\r\nto a single thread, which would sequentially shut down all tasks on the core\r\nand then wait for them to complete.\r\n\r\nThis could deadlock when one task is waiting for a later core's task to\r\ncomplete. For example, in the newly added test, we have a `block_in_place` task\r\nthat is waiting for another task to be dropped. If the latter task adds its\r\ncore to the shutdown list later than the former, we end up waiting forever for\r\nthe `block_in_place` task to complete.\r\n\r\nAdditionally, there also was a bug wherein we'd attempt to park on the parker\r\nafter shutting it down which was fixed as part of the refactors above.\r\n\r\nThis change restructures the code to bring all tasks to a halt (and do any\r\nparking needed) before we collapse to a single thread to avoid this deadlock.\r\n\r\nThere was also an issue in which canceled tasks would not unpark the\r\noriginating thread, due to what appears to be some sort of optimization gone\r\nwrong. This has been fixed to be much more conservative in selecting when not\r\nto unpark the source thread (this may be too conservative; please take a look\r\nat the changes to `release()`).\r\n\r\nFixes: #2789",
          "timestamp": "2020-12-07T20:55:02-08:00",
          "tree_id": "1890e495daa058f06c8a738de4c88b0aeea52f77",
          "url": "https://github.com/tokio-rs/tokio/commit/57dffb9dfe9e4c0f12429246540add3975f4a754"
        },
        "date": 1607403404570,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14296,
            "range": "± 4307",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 963,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 603,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14548,
            "range": "± 3094",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 976,
            "range": "± 133",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e01391351bcb0715f737cefe94e1bc99f19af226",
          "message": "Add stress test (#3222)\n\nCreated a simple echo TCP server that on two different runtimes that is\r\ncalled from a GitHub action using Valgrind to ensure that there are\r\nno memory leaks.\r\n\r\nFixes: #3022",
          "timestamp": "2020-12-07T21:12:22-08:00",
          "tree_id": "5575f27e36e49b887062119225e1d61335a01b9a",
          "url": "https://github.com/tokio-rs/tokio/commit/e01391351bcb0715f737cefe94e1bc99f19af226"
        },
        "date": 1607404426712,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13509,
            "range": "± 2412",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 860,
            "range": "± 127",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 508,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13167,
            "range": "± 3195",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 836,
            "range": "± 122",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9",
          "message": "chore: fix stress test (#3233)",
          "timestamp": "2020-12-09T07:38:25+09:00",
          "tree_id": "0b92fb11f764a5e88d62a9f79aa2107ebcb75f42",
          "url": "https://github.com/tokio-rs/tokio/commit/fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9"
        },
        "date": 1607467196988,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13274,
            "range": "± 2605",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 908,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 541,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13673,
            "range": "± 2436",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 910,
            "range": "± 9",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9706ca92a8deb69d6e29265f21424042fea966c5",
          "message": "time: Fix race condition in timer drop (#3229)\n\nDropping a timer on the millisecond that it was scheduled for, when it was on\r\nthe pending list, could result in a panic previously, as we did not record the\r\npending-list state in cached_when.\r\n\r\nHopefully fixes: ZcashFoundation/zebra#1452",
          "timestamp": "2020-12-08T16:42:43-08:00",
          "tree_id": "cd77e2148b7cdf03d0fcb38e8e27cf3f7eed1ed9",
          "url": "https://github.com/tokio-rs/tokio/commit/9706ca92a8deb69d6e29265f21424042fea966c5"
        },
        "date": 1607474657057,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15169,
            "range": "± 3303",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1098,
            "range": "± 38",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 648,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14575,
            "range": "± 3304",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1096,
            "range": "± 33",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "473ddaa277f51917aeb2fe743b1582685828d6dd",
          "message": "chore: prepare for Tokio 1.0 work (#3238)",
          "timestamp": "2020-12-09T09:42:05-08:00",
          "tree_id": "7af80d4c2bfffff4b6f04db875779e2f49f31280",
          "url": "https://github.com/tokio-rs/tokio/commit/473ddaa277f51917aeb2fe743b1582685828d6dd"
        },
        "date": 1607535825863,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14478,
            "range": "± 4127",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1086,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 647,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14488,
            "range": "± 2905",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1088,
            "range": "± 150",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "52cd240053b2e1dd5835186539f563c3496dfd7d",
          "message": "task: add missing feature flags for task_local and spawn_blocking (#3237)",
          "timestamp": "2020-12-09T23:49:28+01:00",
          "tree_id": "bbc90b40091bd716d0269b84da2bafb32288b149",
          "url": "https://github.com/tokio-rs/tokio/commit/52cd240053b2e1dd5835186539f563c3496dfd7d"
        },
        "date": 1607554262463,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15620,
            "range": "± 4397",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1083,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 712,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15750,
            "range": "± 3242",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1085,
            "range": "± 77",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2a30e13f38b864807f9ad92023e91b060a6227a4",
          "message": "net: expose poll_* methods on UnixDatagram (#3223)",
          "timestamp": "2020-12-10T08:36:43+01:00",
          "tree_id": "2ff07d9ed9f82c562e03ae7f13dd05150ffe899f",
          "url": "https://github.com/tokio-rs/tokio/commit/2a30e13f38b864807f9ad92023e91b060a6227a4"
        },
        "date": 1607585874851,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13398,
            "range": "± 2372",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 910,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 538,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13919,
            "range": "± 2337",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 910,
            "range": "± 5",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f60860af7edefef5373d50d77ab605d648d60526",
          "message": "watch: fix spurious wakeup (#3234)\n\nCo-authored-by: @tijsvd",
          "timestamp": "2020-12-10T09:46:01+01:00",
          "tree_id": "44bc86bbaa5393a0dc3a94a2066569dcb1b79df1",
          "url": "https://github.com/tokio-rs/tokio/commit/f60860af7edefef5373d50d77ab605d648d60526"
        },
        "date": 1607590069272,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13993,
            "range": "± 3591",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 988,
            "range": "± 128",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 584,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14229,
            "range": "± 4359",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 990,
            "range": "± 163",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "clemens.koza@gmx.at",
            "name": "Clemens Koza",
            "username": "SillyFreak"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9646b4bce342342cc654c4c0834c0bf3627f7aa0",
          "message": "toml: enable test-util feature for the playground (#3224)",
          "timestamp": "2020-12-10T10:39:05+01:00",
          "tree_id": "0c5c06ea6a86a13b9485506cf2066945eaf53189",
          "url": "https://github.com/tokio-rs/tokio/commit/9646b4bce342342cc654c4c0834c0bf3627f7aa0"
        },
        "date": 1607593240760,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13708,
            "range": "± 2505",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 906,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 547,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13298,
            "range": "± 1918",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 907,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yusuktan@maguro.dev",
            "name": "Yusuke Tanaka",
            "username": "magurotuna"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4b1d76ec8f35052480eb14204d147df658bfdfdd",
          "message": "docs: fix typo in signal module documentation (#3249)",
          "timestamp": "2020-12-10T08:11:45-08:00",
          "tree_id": "46efd6f41cfaf702fb40c62b89800c511309d584",
          "url": "https://github.com/tokio-rs/tokio/commit/4b1d76ec8f35052480eb14204d147df658bfdfdd"
        },
        "date": 1607616804643,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16310,
            "range": "± 6819",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1054,
            "range": "± 180",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 606,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15704,
            "range": "± 5190",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1059,
            "range": "± 165",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e",
          "message": "net: Pass SocketAddr by value (#3125)",
          "timestamp": "2020-12-10T14:58:27-05:00",
          "tree_id": "d46d58a79f31dba872aa060ef378743fcedea70e",
          "url": "https://github.com/tokio-rs/tokio/commit/16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e"
        },
        "date": 1607630391511,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15871,
            "range": "± 5217",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1109,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 647,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16527,
            "range": "± 4794",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1092,
            "range": "± 26",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "69e62ef89e481e0fb29ce3ef4ddce1eea4114000",
          "message": "sync: make TryAcquireError public (#3250)\n\nThe [`Semaphore::try_acquire`][1] method currently returns a private error type.\r\n\r\n[1]: https://docs.rs/tokio/0.3/tokio/sync/struct.Semaphore.html#method.try_acquire",
          "timestamp": "2020-12-10T19:56:05-08:00",
          "tree_id": "0784747565f6583a726c85dfedcd0527d8373cc6",
          "url": "https://github.com/tokio-rs/tokio/commit/69e62ef89e481e0fb29ce3ef4ddce1eea4114000"
        },
        "date": 1607659046414,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15759,
            "range": "± 8241",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1090,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 646,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15141,
            "range": "± 4471",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1089,
            "range": "± 8",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cameron.evan@gmail.com",
            "name": "Evan Cameron",
            "username": "leshow"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "68717c7efaced76651915696495dcb04c890be25",
          "message": "net: remove empty udp module (#3260)",
          "timestamp": "2020-12-11T14:45:57-05:00",
          "tree_id": "1b7333194ac78d7ae87c5ca9f423ef830cb486b8",
          "url": "https://github.com/tokio-rs/tokio/commit/68717c7efaced76651915696495dcb04c890be25"
        },
        "date": 1607716034755,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14546,
            "range": "± 2984",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1088,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 646,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14852,
            "range": "± 3196",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1088,
            "range": "± 34",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b01b2dacf2e4136c0237977dac27a3688467d2ea",
          "message": "net: update `TcpStream::poll_peek` to use `ReadBuf` (#3259)\n\nCloses #2987",
          "timestamp": "2020-12-11T20:40:24-08:00",
          "tree_id": "1e0bbb86739731038cc9fd69fe112cad54662d16",
          "url": "https://github.com/tokio-rs/tokio/commit/b01b2dacf2e4136c0237977dac27a3688467d2ea"
        },
        "date": 1607748115672,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15759,
            "range": "± 7662",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1036,
            "range": "± 143",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 612,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15849,
            "range": "± 6515",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1026,
            "range": "± 226",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c1ec469ad2af883b001d54e81dad426c01f918cd",
          "message": "util: add constructors to TokioContext (#3221)",
          "timestamp": "2020-12-11T20:41:22-08:00",
          "tree_id": "cdb1273c1a4eea6c7175578bc8a13f417c3daf00",
          "url": "https://github.com/tokio-rs/tokio/commit/c1ec469ad2af883b001d54e81dad426c01f918cd"
        },
        "date": 1607748179882,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15996,
            "range": "± 4631",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1112,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 646,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16096,
            "range": "± 8348",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1113,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sunjay@users.noreply.github.com",
            "name": "Sunjay Varma",
            "username": "sunjay"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "df20c162ae1308c07073b6a67c8ba4202f52d208",
          "message": "sync: add blocking_recv method to UnboundedReceiver, similar to Receiver::blocking_recv (#3262)",
          "timestamp": "2020-12-12T08:47:35-08:00",
          "tree_id": "94fe5abd9735b0c4985d5b38a8d96c51953b0f0b",
          "url": "https://github.com/tokio-rs/tokio/commit/df20c162ae1308c07073b6a67c8ba4202f52d208"
        },
        "date": 1607791757008,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14433,
            "range": "± 4357",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 993,
            "range": "± 150",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 566,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13893,
            "range": "± 2218",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 919,
            "range": "± 129",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a",
          "message": "net: check for false-positives in TcpStream::ready doc test (#3255)",
          "timestamp": "2020-12-13T15:24:59+01:00",
          "tree_id": "89deb6d808e007e1728a43f0a198afe32a4aae1e",
          "url": "https://github.com/tokio-rs/tokio/commit/be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a"
        },
        "date": 1607869580842,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13715,
            "range": "± 2667",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 914,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 537,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13038,
            "range": "± 1796",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 914,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40773946+faith@users.noreply.github.com",
            "name": "Aldas",
            "username": "faith"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f26f444f42c9369bcf8afe14ea00ea53dd663cc2",
          "message": "doc: added tracing to the feature flags section (#3254)",
          "timestamp": "2020-12-13T15:26:56+01:00",
          "tree_id": "23e7ae2599b67a6ab6faf0586f568b0ff6e0c72d",
          "url": "https://github.com/tokio-rs/tokio/commit/f26f444f42c9369bcf8afe14ea00ea53dd663cc2"
        },
        "date": 1607869691195,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 12682,
            "range": "± 2378",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 910,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 536,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13100,
            "range": "± 1820",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 912,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4c5541945348c4614c29a13c06f2e996eb419e42",
          "message": "net: add UnixStream readiness and non-blocking ops (#3246)",
          "timestamp": "2020-12-13T16:21:11+01:00",
          "tree_id": "355917f9e8ba45094e1a3e1f465875f51f56e255",
          "url": "https://github.com/tokio-rs/tokio/commit/4c5541945348c4614c29a13c06f2e996eb419e42"
        },
        "date": 1607872969881,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14236,
            "range": "± 3900",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 973,
            "range": "± 186",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 564,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13850,
            "range": "± 5199",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 919,
            "range": "± 163",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "6172808+skerkour@users.noreply.github.com",
            "name": "Sylvain Kerkour",
            "username": "skerkour"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9149d7bfae251289cd21aa9ee109b4e2a190d0fa",
          "message": "docs: mention blocking thread timeout in src/lib.rs (#3253)",
          "timestamp": "2020-12-13T16:24:16+01:00",
          "tree_id": "38b69f17cc4644ac6ca081aa1d88d5cfe35825fa",
          "url": "https://github.com/tokio-rs/tokio/commit/9149d7bfae251289cd21aa9ee109b4e2a190d0fa"
        },
        "date": 1607873163027,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16586,
            "range": "± 6152",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1094,
            "range": "± 248",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 640,
            "range": "± 173",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16618,
            "range": "± 6466",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1071,
            "range": "± 225",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jacob@jotpot.co.uk",
            "name": "Jacob O'Toole",
            "username": "JOT85"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1f862d2e950bf5f4fb31f202a544e86880f51191",
          "message": "sync: add `watch::Sender::borrow()` (#3269)",
          "timestamp": "2020-12-13T20:46:11-08:00",
          "tree_id": "e203a63415d1b02e7b62f73f8ebeebedeaaef82d",
          "url": "https://github.com/tokio-rs/tokio/commit/1f862d2e950bf5f4fb31f202a544e86880f51191"
        },
        "date": 1607921264901,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16414,
            "range": "± 4082",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1149,
            "range": "± 169",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 680,
            "range": "± 64",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16923,
            "range": "± 7046",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1135,
            "range": "± 63",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3f29212cb7743462451293b65d4bb8b5b4973fc5",
          "message": "fs: use cfgs on fns instead of OS ext traits (#3264)\n\nInstead of using OS specific extension traits, OS specific methods are\r\nmoved onto the structs themselves and guarded with `cfg`. The API\r\ndocumentation should highlight the function is platform specific.\r\n\r\nCloses #2925",
          "timestamp": "2020-12-14T09:29:10-08:00",
          "tree_id": "a09b2ea3cce14487af2ace8030dd2ba6a3687cb5",
          "url": "https://github.com/tokio-rs/tokio/commit/3f29212cb7743462451293b65d4bb8b5b4973fc5"
        },
        "date": 1607967034009,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13907,
            "range": "± 2517",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 907,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 538,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 12911,
            "range": "± 2192",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 926,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8efa62013b551d5130791c3a79ce8ab5cb0b5abf",
          "message": "Move stream items into `tokio-stream` (#3277)\n\nThis change removes all references to `Stream` from\r\nwithin the `tokio` crate and moves them into a new\r\n`tokio-stream` crate. Most types have had their\r\n`impl Stream` removed as well in-favor of their\r\ninherent methods.\r\n\r\nCloses #2870",
          "timestamp": "2020-12-15T20:24:38-08:00",
          "tree_id": "6da8c41c8e1808bea98fd2d23ee1ec03a1cc7e80",
          "url": "https://github.com/tokio-rs/tokio/commit/8efa62013b551d5130791c3a79ce8ab5cb0b5abf"
        },
        "date": 1608092773843,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14816,
            "range": "± 4111",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1052,
            "range": "± 72",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 628,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14842,
            "range": "± 3013",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1039,
            "range": "± 66",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d74d17307dd53215061c4a8a1f20a0e30461e296",
          "message": "time: remove `Box` from `Sleep` (#3278)\n\nRemoves the box from `Sleep`, taking advantage of intrusive wakers. The\r\n`Sleep` future is now `!Unpin`.\r\n\r\nCloses #3267",
          "timestamp": "2020-12-16T21:51:34-08:00",
          "tree_id": "0cdbf57e4a9b38302ddae0078eb5a1b9a4977aa2",
          "url": "https://github.com/tokio-rs/tokio/commit/d74d17307dd53215061c4a8a1f20a0e30461e296"
        },
        "date": 1608184411565,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16719,
            "range": "± 6605",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1123,
            "range": "± 138",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 658,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 17226,
            "range": "± 5855",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1110,
            "range": "± 166",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "59e4b35f49e6a95a961fd9003db58191de97d3a0",
          "message": "stream: Fix a few doc issues (#3285)",
          "timestamp": "2020-12-17T10:35:49-05:00",
          "tree_id": "757b133c0eac5b1c26d2ba870d0f4b5c198d7505",
          "url": "https://github.com/tokio-rs/tokio/commit/59e4b35f49e6a95a961fd9003db58191de97d3a0"
        },
        "date": 1608219445912,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13173,
            "range": "± 2330",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 914,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 542,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13228,
            "range": "± 2717",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 917,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8",
          "message": "docs: Add more comprehensive stream docs (#3286)\n\n* docs: Add more comprehensive stream docs\r\n\r\n* Apply suggestions from code review\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>\r\n\r\n* Fix doc tests\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-17T11:46:09-05:00",
          "tree_id": "f7afa84006c8629a0d2c058b8e52042c54436203",
          "url": "https://github.com/tokio-rs/tokio/commit/c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8"
        },
        "date": 1608223667937,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15325,
            "range": "± 3736",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1140,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 678,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15449,
            "range": "± 3288",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1138,
            "range": "± 11",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "abd4c0025539f142ec48a09e01430f7ee3b83214",
          "message": "time: enforce `current_thread` rt for time::pause (#3289)\n\nPausing time is a capability added to assist with testing Tokio code\r\ndependent on time. Currently, the capability implicitly requires the\r\ncurrent_thread runtime.\r\n\r\nThis change enforces the requirement by panicking if called from a\r\nmulti-threaded runtime.",
          "timestamp": "2020-12-17T15:37:08-08:00",
          "tree_id": "6c565d6c74dff336ac847cb6463245283d8470d5",
          "url": "https://github.com/tokio-rs/tokio/commit/abd4c0025539f142ec48a09e01430f7ee3b83214"
        },
        "date": 1608248320305,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15649,
            "range": "± 4386",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1143,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 676,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15130,
            "range": "± 1930",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1145,
            "range": "± 9",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3ecaf9fd9a519da9b1decb4c7239b48277040522",
          "message": "codec: write documentation for codec (#3283)\n\nCo-authored-by: Lucio Franco <luciofranco14@gmail.com>",
          "timestamp": "2020-12-18T21:32:27+01:00",
          "tree_id": "f116db611cb8144b9ec3e2e97286aab59cdd9556",
          "url": "https://github.com/tokio-rs/tokio/commit/3ecaf9fd9a519da9b1decb4c7239b48277040522"
        },
        "date": 1608323633541,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15607,
            "range": "± 3451",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1099,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 647,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16848,
            "range": "± 5366",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1104,
            "range": "± 104",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d948ccedfce534953a18acf46c8c6572103567c7",
          "message": "chore: fix stress test (#3297)",
          "timestamp": "2020-12-19T12:11:10+01:00",
          "tree_id": "3c417da4134a45bfff1f2d85b9b8cf410dfd9bf9",
          "url": "https://github.com/tokio-rs/tokio/commit/d948ccedfce534953a18acf46c8c6572103567c7"
        },
        "date": 1608376381819,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15070,
            "range": "± 4144",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1046,
            "range": "± 294",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 624,
            "range": "± 231",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15295,
            "range": "± 5713",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1080,
            "range": "± 263",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b99b00eb302ae6ff19ca97d32b1e594143f43a60",
          "message": "rt: change `max_threads` to `max_blocking_threads` (#3287)\n\nFixes #2802",
          "timestamp": "2020-12-19T08:04:04-08:00",
          "tree_id": "458d7fb55f921184a1056e766b6d0101fb763579",
          "url": "https://github.com/tokio-rs/tokio/commit/b99b00eb302ae6ff19ca97d32b1e594143f43a60"
        },
        "date": 1608393947520,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 17205,
            "range": "± 5881",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1064,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 615,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16374,
            "range": "± 7521",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1046,
            "range": "± 131",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "flo.huebsch@pm.me",
            "name": "Florian Hübsch",
            "username": "fl9"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e41e6cddbba0cf13403924937ffe02aae6639e28",
          "message": "docs: tokio::main macro is also supported on rt (#3243)\n\nFixes: #3144\r\nRefs: #2225",
          "timestamp": "2020-12-19T19:12:08+01:00",
          "tree_id": "ee1af0c8a3b2ab9c9eaae05f2dca96ff966f42f9",
          "url": "https://github.com/tokio-rs/tokio/commit/e41e6cddbba0cf13403924937ffe02aae6639e28"
        },
        "date": 1608401646456,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16320,
            "range": "± 9602",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1107,
            "range": "± 127",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 642,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 16822,
            "range": "± 6462",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1103,
            "range": "± 163",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "78f2340d259487d4470681f97cc4b5eac719e178",
          "message": "tokio: remove prelude (#3299)\n\nCloses: #3257",
          "timestamp": "2020-12-19T11:42:24-08:00",
          "tree_id": "2a093e994a41bae54e324fd8d20c836e46a5947c",
          "url": "https://github.com/tokio-rs/tokio/commit/78f2340d259487d4470681f97cc4b5eac719e178"
        },
        "date": 1608407042976,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16773,
            "range": "± 7224",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1051,
            "range": "± 211",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 645,
            "range": "± 679",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 18189,
            "range": "± 9111",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1066,
            "range": "± 202",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "arve.knudsen@gmail.com",
            "name": "Arve Knudsen",
            "username": "aknuds1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1b70507894035e066cb488c14ff328bd47ca696d",
          "message": "net: remove {Tcp,Unix}Stream::shutdown() (#3298)\n\n`shutdown()` on `AsyncWrite` performs a TCP shutdown. This avoids method\r\nconflicts.\r\n\r\nCloses #3294",
          "timestamp": "2020-12-19T12:17:52-08:00",
          "tree_id": "dd3c414761b8a3713b63515e7666f108a1391743",
          "url": "https://github.com/tokio-rs/tokio/commit/1b70507894035e066cb488c14ff328bd47ca696d"
        },
        "date": 1608409176100,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16226,
            "range": "± 5394",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1053,
            "range": "± 127",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 604,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15829,
            "range": "± 5674",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1037,
            "range": "± 143",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e5f513542c0f92f49f6acd31021cc056577be6b",
          "message": "chore: remove some left over `stream` feature code (#3300)\n\nRemoves the `stream` feature flag from `Cargo.toml` and removes the\r\n`futures-core` dependency. Once `Stream` lands in `std`, a feature flag\r\nis most likely not needed.",
          "timestamp": "2020-12-19T14:15:00-08:00",
          "tree_id": "bb5486367eec9a81b5d021202eaf8ab0fe34bdc8",
          "url": "https://github.com/tokio-rs/tokio/commit/5e5f513542c0f92f49f6acd31021cc056577be6b"
        },
        "date": 1608416186341,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14517,
            "range": "± 5952",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 890,
            "range": "± 290",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 508,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14415,
            "range": "± 5101",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 830,
            "range": "± 229",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "28933599888a88e601acbb11fa824b0ee9f98c6e",
          "message": "chore: update to `bytes` 1.0 git branch (#3301)\n\nUpdates the code base to track the `bytes` git branch. This is in\r\npreparation for the 1.0 release.\r\n\r\nCloses #3058",
          "timestamp": "2020-12-19T15:57:16-08:00",
          "tree_id": "2021ef3acf9407fcfa39032e0a493a81f1eb74cc",
          "url": "https://github.com/tokio-rs/tokio/commit/28933599888a88e601acbb11fa824b0ee9f98c6e"
        },
        "date": 1608422314869,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13462,
            "range": "± 2840",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 918,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 540,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13553,
            "range": "± 2360",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 915,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bIgBV@users.noreply.github.com",
            "name": "Bhargav",
            "username": "bIgBV"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c7671a03840751f58b7a509386b8fe3b5e670a37",
          "message": "io: add _mut variants of methods on AsyncFd (#3304)\n\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-21T22:51:28+01:00",
          "tree_id": "9d3f26151c9ddd292c129f43c0fac83792073f62",
          "url": "https://github.com/tokio-rs/tokio/commit/c7671a03840751f58b7a509386b8fe3b5e670a37"
        },
        "date": 1608587567499,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13156,
            "range": "± 2267",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 927,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 540,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13174,
            "range": "± 2983",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 920,
            "range": "± 5",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "564c943309f0a94474e2e71a115ec0bb45f3e7fd",
          "message": "io: rename `AsyncFd::with_io()` and rm `with_poll()` (#3306)",
          "timestamp": "2020-12-21T15:42:38-08:00",
          "tree_id": "6f29310dda04438222fb9581b3cd9581f1abe13e",
          "url": "https://github.com/tokio-rs/tokio/commit/564c943309f0a94474e2e71a115ec0bb45f3e7fd"
        },
        "date": 1608594235809,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13521,
            "range": "± 3026",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 917,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 544,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13363,
            "range": "± 2624",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 918,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "eee3ca65d6e5b4537915571f663f94184e616e6c",
          "message": "deps: update rand to 0.8, loom to 0.4 (#3307)",
          "timestamp": "2020-12-22T10:28:35+01:00",
          "tree_id": "2f534ebe6cb319f30f72e7a9b2b389825500f051",
          "url": "https://github.com/tokio-rs/tokio/commit/eee3ca65d6e5b4537915571f663f94184e616e6c"
        },
        "date": 1608629399187,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13041,
            "range": "± 2778",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 913,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 539,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13562,
            "range": "± 3416",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 914,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f95ad1898041d2bcc09de3db69228e88f5f4abf8",
          "message": "net: clarify when wakeups are sent (#3310)",
          "timestamp": "2020-12-22T07:38:14-08:00",
          "tree_id": "16968a03d8ca55c53da7658c109e47a5f9a8c4e4",
          "url": "https://github.com/tokio-rs/tokio/commit/f95ad1898041d2bcc09de3db69228e88f5f4abf8"
        },
        "date": 1608651589747,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15817,
            "range": "± 4648",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1100,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 649,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15708,
            "range": "± 5502",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1099,
            "range": "± 7",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b83b3b8cc61e9d911abec00c33ec3762b2a6437",
          "message": "fs,sync: expose poll_ fns on misc types (#3308)\n\nIncludes methods on:\r\n\r\n* fs::DirEntry\r\n* io::Lines\r\n* io::Split\r\n* sync::mpsc::Receiver\r\n* sync::misc::UnboundedReceiver",
          "timestamp": "2020-12-22T09:28:14-08:00",
          "tree_id": "bd1ae38a53efb2fe78fc8aba0bdd0789cc7a1495",
          "url": "https://github.com/tokio-rs/tokio/commit/0b83b3b8cc61e9d911abec00c33ec3762b2a6437"
        },
        "date": 1608658176200,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13641,
            "range": "± 3290",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 938,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 541,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14005,
            "range": "± 2732",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 938,
            "range": "± 5",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "carlos.baezruiz@gmail.com",
            "name": "Carlos B",
            "username": "carlosb1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d28e4cdbb63b0ad19e66d1b077690cbebf71353",
          "message": "examples: add futures executor threadpool (#3198)",
          "timestamp": "2020-12-22T20:08:43+01:00",
          "tree_id": "706d894cb4ea97ae1a91b9d2bd42d1800968559c",
          "url": "https://github.com/tokio-rs/tokio/commit/7d28e4cdbb63b0ad19e66d1b077690cbebf71353"
        },
        "date": 1608664226074,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 16288,
            "range": "± 6221",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1100,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 652,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 17978,
            "range": "± 7711",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1092,
            "range": "± 114",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608664524976,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 17311,
            "range": "± 9385",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1067,
            "range": "± 214",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 635,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14998,
            "range": "± 3349",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1098,
            "range": "± 87",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608666585922,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 12992,
            "range": "± 1671",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 918,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 542,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 13388,
            "range": "± 3092",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 916,
            "range": "± 3",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b",
          "message": "time: make Interval::poll_tick() public (#3316)",
          "timestamp": "2020-12-22T12:31:14-08:00",
          "tree_id": "c06c2c6a1618d8dd177cd844f8f816f06e6033b8",
          "url": "https://github.com/tokio-rs/tokio/commit/be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b"
        },
        "date": 1608669166892,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15740,
            "range": "± 5339",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1092,
            "range": "± 165",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 625,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15728,
            "range": "± 5357",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1046,
            "range": "± 101",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luke.steensen@gmail.com",
            "name": "Luke Steensen",
            "username": "lukesteensen"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8dda19da4e94acd45f34b5eb359b4cffafa833f",
          "message": "chore: update to released `bytes` 1.0 (#3317)",
          "timestamp": "2020-12-22T17:09:26-08:00",
          "tree_id": "c177db0f9bced11086bcb13be4ac2348e6c94469",
          "url": "https://github.com/tokio-rs/tokio/commit/a8dda19da4e94acd45f34b5eb359b4cffafa833f"
        },
        "date": 1608685868544,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 18252,
            "range": "± 6739",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1103,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 666,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 17570,
            "range": "± 5542",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1101,
            "range": "± 6",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0deaeb84948f253b76b7fe64d7fe9d4527cd4275",
          "message": "chore: remove unused `slab` dependency (#3318)",
          "timestamp": "2020-12-22T21:56:22-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/0deaeb84948f253b76b7fe64d7fe9d4527cd4275"
        },
        "date": 1608703077221,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 14414,
            "range": "± 4212",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 937,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 547,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14457,
            "range": "± 5238",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 913,
            "range": "± 127",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "575938d4579e6fe6a89b700aadb0ae2bbab5483b",
          "message": "chore: use #[non_exhaustive] instead of private unit field (#3320)",
          "timestamp": "2020-12-23T22:48:33+09:00",
          "tree_id": "f2782c6135a26568d5dea4d45b3310baaf062e16",
          "url": "https://github.com/tokio-rs/tokio/commit/575938d4579e6fe6a89b700aadb0ae2bbab5483b"
        },
        "date": 1608731388000,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 13274,
            "range": "± 2902",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 941,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 543,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 14240,
            "range": "± 2603",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 943,
            "range": "± 5",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ce0e9c67cfe61c7a91a284331ecc53fa01c32879",
          "message": "chore: Revert \"use #[non_exhaustive] instead of private unit field\" (#3323)\n\nThis reverts commit 575938d4579e6fe6a89b700aadb0ae2bbab5483b.",
          "timestamp": "2020-12-23T08:27:58-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/ce0e9c67cfe61c7a91a284331ecc53fa01c32879"
        },
        "date": 1608740981503,
        "tool": "cargo",
        "benches": [
          {
            "name": "contended_concurrent_multi",
            "value": 15489,
            "range": "± 3698",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1049,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 623,
            "range": "± 53",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15171,
            "range": "± 3443",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1046,
            "range": "± 61",
            "unit": "ns/iter"
          }
        ]
      }
    ],
    "sync_mpsc": [
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "distinct": true,
          "id": "af032dbf59195f5a637c14fd8805f45cce8c8563",
          "message": "try again",
          "timestamp": "2020-11-13T16:47:49-08:00",
          "tree_id": "2c351a9bf2bc6d1fb70754ee19640da3b69df204",
          "url": "https://github.com/tokio-rs/tokio/commit/af032dbf59195f5a637c14fd8805f45cce8c8563"
        },
        "date": 1605314997643,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6701051,
            "range": "± 1798407",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6357946,
            "range": "± 1969273",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6190077,
            "range": "± 1989686",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 588,
            "range": "± 135",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 645,
            "range": "± 683",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 604,
            "range": "± 72",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 48623,
            "range": "± 4125",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 901,
            "range": "± 154",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1057858,
            "range": "± 197528",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 768305,
            "range": "± 105579",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "97c2c4203cd7c42960cac895987c43a17dff052e",
          "message": "chore: automate running benchmarks (#3140)\n\nUses Github actions to run benchmarks.",
          "timestamp": "2020-11-13T19:30:52-08:00",
          "tree_id": "f4a3cfebafb7afee68d6d4de1748daddcfc070c6",
          "url": "https://github.com/tokio-rs/tokio/commit/97c2c4203cd7c42960cac895987c43a17dff052e"
        },
        "date": 1605324759979,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6516564,
            "range": "± 2673764",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6603280,
            "range": "± 1010301",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5967313,
            "range": "± 2223714",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 604,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 606,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 604,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45361,
            "range": "± 1799",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 835,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1120543,
            "range": "± 16131",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 800918,
            "range": "± 22116",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "masnagam@gmail.com",
            "name": "masnagam",
            "username": "masnagam"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4e39c9b818eb8af064bb9f45f47e3cfc6593de95",
          "message": "net: restore TcpStream::{poll_read_ready, poll_write_ready} (#2743)",
          "timestamp": "2020-11-16T09:51:06-08:00",
          "tree_id": "2222dd2f8638fb64f228badef84814d2f4079a82",
          "url": "https://github.com/tokio-rs/tokio/commit/4e39c9b818eb8af064bb9f45f47e3cfc6593de95"
        },
        "date": 1605549207449,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6803300,
            "range": "± 2440677",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6651924,
            "range": "± 1480231",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6405364,
            "range": "± 2450565",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 668,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 667,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 667,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 50192,
            "range": "± 929",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 799,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1118325,
            "range": "± 21375",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 816811,
            "range": "± 316994",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f5cb4c20422a35b51bfba3391744f8bcb54f7581",
          "message": "net: Add send/recv buf size methods to `TcpSocket` (#3145)\n\nThis commit adds `set_{send, recv}_buffer_size` methods to `TcpSocket`\r\nfor setting the size of the TCP send and receive buffers, and `{send,\r\nrecv}_buffer_size` methods for returning the current value. These just\r\ncall into similar methods on `mio`'s `TcpSocket` type, which were added\r\nin tokio-rs/mio#1384.\r\n\r\nRefs: #3082\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-11-16T12:29:03-08:00",
          "tree_id": "fcf642984a21d04533efad0cdde613d294635c4d",
          "url": "https://github.com/tokio-rs/tokio/commit/f5cb4c20422a35b51bfba3391744f8bcb54f7581"
        },
        "date": 1605558683599,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6754053,
            "range": "± 2766922",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6486377,
            "range": "± 1920826",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6544483,
            "range": "± 3004414",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 834,
            "range": "± 375",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 842,
            "range": "± 301",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 845,
            "range": "± 160",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 57023,
            "range": "± 7455",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1048,
            "range": "± 182",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1072969,
            "range": "± 131055",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 764003,
            "range": "± 148570",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "zaharidichev@gmail.com",
            "name": "Zahari Dichev",
            "username": "zaharidichev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d0ebb4154748166a4ba07baa4b424a1c45efd219",
          "message": "sync: add `Notify::notify_waiters` (#3098)\n\nThis PR makes `Notify::notify_waiters` public. The method\r\nalready exists, but it changes the way `notify_waiters`,\r\nis used. Previously in order for the consumer to\r\nregister interest, in a notification triggered by\r\n`notify_waiters`, the `Notified` future had to be\r\npolled. This introduced friction when using the api\r\nas the future had to be pinned before polled.\r\n\r\nThis change introduces a counter that tracks how many\r\ntimes `notified_waiters` has been called. Upon creation of\r\nthe future the number of times is loaded. When first\r\npolled the future compares this number with the count\r\nstate of the `Notify` type. This avoids the need for\r\nregistering the waiter upfront.\r\n\r\nFixes: #3066",
          "timestamp": "2020-11-16T12:49:35-08:00",
          "tree_id": "5ea4d611256290f62baea1a9ffa3333b254181df",
          "url": "https://github.com/tokio-rs/tokio/commit/d0ebb4154748166a4ba07baa4b424a1c45efd219"
        },
        "date": 1605559901295,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5738427,
            "range": "± 2801992",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5782978,
            "range": "± 1550441",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 4938344,
            "range": "± 2609337",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 646,
            "range": "± 137",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 616,
            "range": "± 175",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 656,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 52459,
            "range": "± 9246",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 787,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 876850,
            "range": "± 128609",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 630049,
            "range": "± 83042",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0ea23076503c5151d68a781a3d91823396c82949",
          "message": "net: add UdpSocket readiness and non-blocking ops (#3138)\n\nAdds `ready()`, `readable()`, and `writable()` async methods for waiting\r\nfor socket readiness. Adds `try_send`, `try_send_to`, `try_recv`, and\r\n`try_recv_from` for performing non-blocking operations on the socket.\r\n\r\nThis is the UDP equivalent of #3130.",
          "timestamp": "2020-11-16T15:44:01-08:00",
          "tree_id": "1e49d7dc0bb3cee6271133d942ba49c5971fde29",
          "url": "https://github.com/tokio-rs/tokio/commit/0ea23076503c5151d68a781a3d91823396c82949"
        },
        "date": 1605570390864,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7194053,
            "range": "± 2331201",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6928458,
            "range": "± 1519084",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6464793,
            "range": "± 2430089",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 923,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 911,
            "range": "± 102",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 920,
            "range": "± 100",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 66036,
            "range": "± 6878",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1092,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1172665,
            "range": "± 151747",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 792689,
            "range": "± 98126",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "9832640+zekisherif@users.noreply.github.com",
            "name": "Zeki Sherif",
            "username": "zekisherif"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d11aa866837eea50a6f1e0ef7e24846a653cbf1",
          "message": "net: add SO_LINGER get/set to TcpStream (#3143)",
          "timestamp": "2020-11-17T09:58:00-08:00",
          "tree_id": "ca0d5edc04a29bbe6e2906c760a22908e032a4c9",
          "url": "https://github.com/tokio-rs/tokio/commit/7d11aa866837eea50a6f1e0ef7e24846a653cbf1"
        },
        "date": 1605636006243,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6386215,
            "range": "± 2561315",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6223133,
            "range": "± 1216930",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5641856,
            "range": "± 2812615",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 782,
            "range": "± 160",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 771,
            "range": "± 114",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 748,
            "range": "± 136",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 56412,
            "range": "± 10956",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 968,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1047860,
            "range": "± 157168",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 699818,
            "range": "± 128883",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "34fcef258b84d17f8d418b39eb61fa07fa87c390",
          "message": "io: add vectored writes to `AsyncWrite` (#3149)\n\nThis adds `AsyncWrite::poll_write_vectored`, and implements it for\r\n`TcpStream` and `UnixStream`.\r\n\r\nRefs: #3135.",
          "timestamp": "2020-11-18T10:41:47-08:00",
          "tree_id": "98e37fc2d6fa541a9e499331df86ba3d1b7b6e3a",
          "url": "https://github.com/tokio-rs/tokio/commit/34fcef258b84d17f8d418b39eb61fa07fa87c390"
        },
        "date": 1605725015672,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6265900,
            "range": "± 1903787",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5980619,
            "range": "± 1425562",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5608356,
            "range": "± 1837833",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 553,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 551,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 551,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 32381,
            "range": "± 3229",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 747,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 952719,
            "range": "± 1921",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 690909,
            "range": "± 15989",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "479c545c20b2cb44a8f09600733adc8c8dcb5aa0",
          "message": "chore: prepare v0.3.4 release (#3152)",
          "timestamp": "2020-11-18T12:38:13-08:00",
          "tree_id": "df6daba6b2f595de47ada2dd2f518475669ab919",
          "url": "https://github.com/tokio-rs/tokio/commit/479c545c20b2cb44a8f09600733adc8c8dcb5aa0"
        },
        "date": 1605732057947,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6429590,
            "range": "± 1921395",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6087351,
            "range": "± 1216119",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5570765,
            "range": "± 1754220",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 494,
            "range": "± 133",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 489,
            "range": "± 86",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 518,
            "range": "± 95",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 39959,
            "range": "± 4920",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 720,
            "range": "± 116",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 973766,
            "range": "± 210658",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 681859,
            "range": "± 151210",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84",
          "message": "util: fix typo in udp/frame.rs (#3154)",
          "timestamp": "2020-11-20T15:06:14+09:00",
          "tree_id": "f09954c70e26336bdb1bc525f832916c2d7037bf",
          "url": "https://github.com/tokio-rs/tokio/commit/49abfdb2ac7f564c638ef99b973b1ab7a2b7ec84"
        },
        "date": 1605852504276,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6976499,
            "range": "± 2320668",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 7088300,
            "range": "± 1690727",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6373230,
            "range": "± 2466055",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 805,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 803,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 803,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 49997,
            "range": "± 1458",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 871,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1147945,
            "range": "± 6018",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 836883,
            "range": "± 6205",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f927f01a34d7cedf0cdc820f729a7a6cd56e83dd",
          "message": "macros: fix rustfmt on 1.48.0 (#3160)\n\n## Motivation\r\n\r\nLooks like the Rust 1.48.0 version of `rustfmt` changed some formatting\r\nrules (fixed some bugs?), and some of the code in `tokio-macros` is no\r\nlonger correctly formatted. This is breaking CI.\r\n\r\n## Solution\r\n\r\nThis commit runs rustfmt on Rust 1.48.0. This fixes CI.\r\n\r\nCloses #3158",
          "timestamp": "2020-11-20T10:19:26-08:00",
          "tree_id": "bd0243a653ee49cfc50bf61b00a36cc0fce6a414",
          "url": "https://github.com/tokio-rs/tokio/commit/f927f01a34d7cedf0cdc820f729a7a6cd56e83dd"
        },
        "date": 1605896470436,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6396651,
            "range": "± 2089250",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6210787,
            "range": "± 1599888",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5777463,
            "range": "± 2201398",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 876,
            "range": "± 109",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 879,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 877,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 55799,
            "range": "± 2028",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1076,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1092314,
            "range": "± 31319",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 774222,
            "range": "± 43076",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ae67851f11b7cc1f577de8ce21767ce3e2c7aff9",
          "message": "time: use intrusive lists for timer tracking (#3080)\n\nMore-or-less a half-rewrite of the current time driver, supporting the\r\nuse of intrusive futures for timer registration.\r\n\r\nFixes: #3028, #3069",
          "timestamp": "2020-11-23T10:42:50-08:00",
          "tree_id": "be43cb76333b0e9e42a101d659f9b2e41555d779",
          "url": "https://github.com/tokio-rs/tokio/commit/ae67851f11b7cc1f577de8ce21767ce3e2c7aff9"
        },
        "date": 1606157077076,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6854697,
            "range": "± 2268317",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6837935,
            "range": "± 1397559",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6191297,
            "range": "± 2218839",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 626,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 628,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 627,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 53804,
            "range": "± 14664",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 862,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1190306,
            "range": "± 10627",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 839804,
            "range": "± 30196",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "driftluo@foxmail.com",
            "name": "漂流",
            "username": "driftluo"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "874fc3320bc000fee20d63b3ad865a1145122640",
          "message": "codec: add read_buffer_mut to FramedRead (#3166)",
          "timestamp": "2020-11-24T09:39:16+01:00",
          "tree_id": "53540b744f6a915cedc1099afe1b0639443b2436",
          "url": "https://github.com/tokio-rs/tokio/commit/874fc3320bc000fee20d63b3ad865a1145122640"
        },
        "date": 1606207272816,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6874876,
            "range": "± 2264622",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 7077282,
            "range": "± 1761310",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6583352,
            "range": "± 2602094",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 593,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 626,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 612,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 52823,
            "range": "± 5057",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 849,
            "range": "± 107",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1132312,
            "range": "± 46554",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 802557,
            "range": "± 80118",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "de33ee85ce61377b316b630e4355d419cc4abcb7",
          "message": "time: replace 'ouClockTimeide' in internal docs with 'outside' (#3171)",
          "timestamp": "2020-11-24T10:23:20+01:00",
          "tree_id": "5ed85f95ea1846983471a11fe555328e6b0f5f6f",
          "url": "https://github.com/tokio-rs/tokio/commit/de33ee85ce61377b316b630e4355d419cc4abcb7"
        },
        "date": 1606209916541,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6773998,
            "range": "± 2121267",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6803850,
            "range": "± 1404717",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5803786,
            "range": "± 2319369",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 595,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 605,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 601,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 49827,
            "range": "± 1373",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 824,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1145595,
            "range": "± 101979",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 822752,
            "range": "± 13086",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rajiv.chauhan@gmail.com",
            "name": "Rajiv Chauhan",
            "username": "chauhraj"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e406a7a47699d93fa2a77fb72553600cb7abd0f",
          "message": "macros: fix outdated documentation (#3180)\n\n1. Changed 0.2 to 0.3\r\n2. Changed ‘multi’ to ‘single’ to indicate that the behavior is single threaded",
          "timestamp": "2020-11-26T19:46:15+01:00",
          "tree_id": "ac6898684e4b84e4a5d0e781adf42d950bbc9e43",
          "url": "https://github.com/tokio-rs/tokio/commit/5e406a7a47699d93fa2a77fb72553600cb7abd0f"
        },
        "date": 1606416482810,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6545251,
            "range": "± 2437278",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6273179,
            "range": "± 1874503",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5864333,
            "range": "± 2511343",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 823,
            "range": "± 146",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 823,
            "range": "± 243",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 823,
            "range": "± 205",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 56599,
            "range": "± 12354",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1034,
            "range": "± 228",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1102654,
            "range": "± 761186",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 768163,
            "range": "± 118088",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "niklas.fiekas@backscattering.de",
            "name": "Niklas Fiekas",
            "username": "niklasf"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49129434198a96444bc0e9582a14062d3a46e93a",
          "message": "signal: expose CtrlC stream on windows (#3186)\n\n* Make tokio::signal::windows::ctrl_c() public.\r\n* Stop referring to private tokio::signal::windows::Event in module\r\n  documentation.\r\n\r\nCloses #3178",
          "timestamp": "2020-11-27T19:53:17Z",
          "tree_id": "904fb6b1fb539bffe69168c7202ccc3db15321dc",
          "url": "https://github.com/tokio-rs/tokio/commit/49129434198a96444bc0e9582a14062d3a46e93a"
        },
        "date": 1606506921199,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6703808,
            "range": "± 1859987",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6883299,
            "range": "± 1203924",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5857453,
            "range": "± 2137028",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 764,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 756,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 756,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 52786,
            "range": "± 845",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 864,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1106133,
            "range": "± 66908",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 768048,
            "range": "± 44858",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "github@max.sharnoff.org",
            "name": "Max Sharnoff",
            "username": "sharnoff"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0acd06b42a9d1461302388f2a533e86d391d6040",
          "message": "runtime: fix shutdown_timeout(0) blocking (#3174)",
          "timestamp": "2020-11-28T19:31:13+01:00",
          "tree_id": "c17e5d58e10ee419e492cb831843c3f08e1f66d8",
          "url": "https://github.com/tokio-rs/tokio/commit/0acd06b42a9d1461302388f2a533e86d391d6040"
        },
        "date": 1606588401306,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7024900,
            "range": "± 2147678",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6930013,
            "range": "± 1405724",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6474454,
            "range": "± 2351539",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 768,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 792,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 802,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45699,
            "range": "± 1918",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 913,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1162655,
            "range": "± 2369",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 824930,
            "range": "± 22589",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c55d846f4b248b4a72335d6c57829fa6396ab9a5",
          "message": "util: add rt to tokio-util full feature (#3194)",
          "timestamp": "2020-11-29T09:48:31+01:00",
          "tree_id": "5f27b29cd1018796f0713d6e87e4823920ba5084",
          "url": "https://github.com/tokio-rs/tokio/commit/c55d846f4b248b4a72335d6c57829fa6396ab9a5"
        },
        "date": 1606639841090,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6677120,
            "range": "± 2481685",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6671099,
            "range": "± 1720537",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5848486,
            "range": "± 1834868",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 852,
            "range": "± 197",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 815,
            "range": "± 261",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 810,
            "range": "± 155",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 60697,
            "range": "± 13430",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1011,
            "range": "± 221",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1035693,
            "range": "± 193376",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 742766,
            "range": "± 125277",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "kylekosic@gmail.com",
            "name": "Kyle Kosic",
            "username": "kykosic"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a85fdb884d961bb87a2f3d446c548802868e54cb",
          "message": "runtime: test for shutdown_timeout(0) (#3196)",
          "timestamp": "2020-11-29T21:30:19+01:00",
          "tree_id": "c554597c6596dc6eddc98bfafcc512361ddb5f31",
          "url": "https://github.com/tokio-rs/tokio/commit/a85fdb884d961bb87a2f3d446c548802868e54cb"
        },
        "date": 1606681923701,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6001104,
            "range": "± 2127391",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6077474,
            "range": "± 1352891",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5253000,
            "range": "± 2165577",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 517,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 557,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 568,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 36320,
            "range": "± 4272",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 664,
            "range": "± 205",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 884445,
            "range": "± 135354",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 648030,
            "range": "± 94596",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pickfire@riseup.net",
            "name": "Ivan Tham",
            "username": "pickfire"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "72d6346c0d43d867002dc0cc5527fbd0b0e23c3f",
          "message": "macros: #[tokio::main] can be used on non-main (#3199)",
          "timestamp": "2020-11-30T17:34:11+01:00",
          "tree_id": "c558d1cb380cc67bfc56ea960a7d9e266259367a",
          "url": "https://github.com/tokio-rs/tokio/commit/72d6346c0d43d867002dc0cc5527fbd0b0e23c3f"
        },
        "date": 1606754197863,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7210680,
            "range": "± 3350838",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 7034995,
            "range": "± 2440942",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6672770,
            "range": "± 2782948",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 770,
            "range": "± 76",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 788,
            "range": "± 64",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 802,
            "range": "± 200",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 66752,
            "range": "± 4020",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1065,
            "range": "± 113",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1146494,
            "range": "± 124772",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 791618,
            "range": "± 68730",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "73653352+HK416-is-all-you-need@users.noreply.github.com",
            "name": "HK416-is-all-you-need",
            "username": "HK416-is-all-you-need"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5",
          "message": "io: add AsyncFd::with_interest (#3167)\n\nFixes #3072",
          "timestamp": "2020-11-30T11:11:18-08:00",
          "tree_id": "45e9d190af02ab0cdc92c317e3127a1b8227ac3a",
          "url": "https://github.com/tokio-rs/tokio/commit/7707ba88efa9b9d78436f1fbdc873d81bfc3f7a5"
        },
        "date": 1606763584971,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6236293,
            "range": "± 1971274",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6059443,
            "range": "± 1323861",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5610724,
            "range": "± 2023793",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 544,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 542,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 541,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 32238,
            "range": "± 1599",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 745,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 957248,
            "range": "± 2245",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 690117,
            "range": "± 1909",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "08548583b948a0be04338f1b1462917c001dbf4a",
          "message": "chore: prepare v0.3.5 release (#3201)",
          "timestamp": "2020-11-30T12:57:31-08:00",
          "tree_id": "bc964338ba8d03930d53192a1e2288132330ff97",
          "url": "https://github.com/tokio-rs/tokio/commit/08548583b948a0be04338f1b1462917c001dbf4a"
        },
        "date": 1606769960282,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6773291,
            "range": "± 2054647",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6290147,
            "range": "± 1785787",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6157545,
            "range": "± 2613559",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 830,
            "range": "± 40",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 834,
            "range": "± 138",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 831,
            "range": "± 148",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 58159,
            "range": "± 7269",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1058,
            "range": "± 215",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1087146,
            "range": "± 182419",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 773018,
            "range": "± 137081",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "128495168d390092df2cb8ae8577cfec09f666ff",
          "message": "ci: switch FreeBSD CI environment to 12.2-RELEASE (#3202)\n\n12.1 will be EoL in two months.",
          "timestamp": "2020-12-01T10:19:54+09:00",
          "tree_id": "2a289d5667b3ffca2ebfb747785c380ee7eac034",
          "url": "https://github.com/tokio-rs/tokio/commit/128495168d390092df2cb8ae8577cfec09f666ff"
        },
        "date": 1606785710244,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6328276,
            "range": "± 2246802",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6288952,
            "range": "± 1380874",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5754739,
            "range": "± 2054266",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 655,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 698,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 649,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 49359,
            "range": "± 6960",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 675,
            "range": "± 93",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 942974,
            "range": "± 122454",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 693581,
            "range": "± 101894",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "353b0544a04214e7d6e828641e2045df1d97cda8",
          "message": "ci: reenable CI on FreeBSD i686 (#3204)\n\nIt was temporarily disabled in 06c473e62842d257ed275497ce906710ea3f8e19\r\nand never reenabled.",
          "timestamp": "2020-12-01T10:20:18+09:00",
          "tree_id": "468f282ba9f5116f5ed9a81abacbb7385aaa9c1e",
          "url": "https://github.com/tokio-rs/tokio/commit/353b0544a04214e7d6e828641e2045df1d97cda8"
        },
        "date": 1606785752138,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6423885,
            "range": "± 2221839",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6583139,
            "range": "± 1854430",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5850827,
            "range": "± 1885891",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 766,
            "range": "± 121",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 720,
            "range": "± 116",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 784,
            "range": "± 161",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 61864,
            "range": "± 8560",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 946,
            "range": "± 124",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1072786,
            "range": "± 132609",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 753761,
            "range": "± 129416",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asomers@gmail.com",
            "name": "Alan Somers",
            "username": "asomers"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ae8135b62057be6b1691f04b27eabe285b05efd",
          "message": "process: fix the process_kill_on_drop.rs test on non-Linux systems (#3203)\n\n\"disown\" is a bash builtin, not part of POSIX sh.",
          "timestamp": "2020-12-01T10:20:49+09:00",
          "tree_id": "8b211b0f9807692d77be8a64a4835718355afe7b",
          "url": "https://github.com/tokio-rs/tokio/commit/7ae8135b62057be6b1691f04b27eabe285b05efd"
        },
        "date": 1606785778873,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6032506,
            "range": "± 2359849",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6112976,
            "range": "± 1788315",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5243346,
            "range": "± 2373168",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 750,
            "range": "± 114",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 745,
            "range": "± 120",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 777,
            "range": "± 166",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 58936,
            "range": "± 8582",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 963,
            "range": "± 259",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 949434,
            "range": "± 178970",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 694922,
            "range": "± 215095",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8e0f0a919663b210627c132d6af3e19a95d8037",
          "message": "example: add back udp-codec example (#3205)",
          "timestamp": "2020-12-01T12:20:20+09:00",
          "tree_id": "b18851ef95641ab2e2d1f632e2ce39cb1fcb1301",
          "url": "https://github.com/tokio-rs/tokio/commit/a8e0f0a919663b210627c132d6af3e19a95d8037"
        },
        "date": 1606792948676,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6446143,
            "range": "± 1951807",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6227802,
            "range": "± 2228241",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5750007,
            "range": "± 2988299",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 679,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 672,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 686,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 56226,
            "range": "± 8050",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 998,
            "range": "± 166",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1027295,
            "range": "± 79491",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 743059,
            "range": "± 110554",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a6051a61ec5c96113f4b543de3ec55431695347a",
          "message": "sync: make add_permits panic with usize::MAX >> 3 permits (#3188)",
          "timestamp": "2020-12-02T22:58:28+01:00",
          "tree_id": "1a4d4bcc017f6a61a652505b1edd4a3bf36ea1ab",
          "url": "https://github.com/tokio-rs/tokio/commit/a6051a61ec5c96113f4b543de3ec55431695347a"
        },
        "date": 1606946421075,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6705661,
            "range": "± 2353737",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6760281,
            "range": "± 1367431",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6098688,
            "range": "± 2190842",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 699,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 696,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 676,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 42075,
            "range": "± 2432",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 761,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1105850,
            "range": "± 6828",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 806091,
            "range": "± 6530",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "eliza@buoyant.io",
            "name": "Eliza Weisman",
            "username": "hawkw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "647299866a2262c8a1183adad73673e5803293ed",
          "message": "util: add writev-aware `poll_write_buf` (#3156)\n\n## Motivation\r\n\r\nIn Tokio 0.2, `AsyncRead` and `AsyncWrite` had `poll_write_buf` and\r\n`poll_read_buf` methods for reading and writing to implementers of\r\n`bytes` `Buf` and `BufMut` traits. In 0.3, these were removed, but\r\n`poll_read_buf` was added as a free function in `tokio-util`. However,\r\nthere is currently no `poll_write_buf`.\r\n\r\nNow that `AsyncWrite` has regained support for vectored writes in #3149,\r\nthere's a lot of potential benefit in having a `poll_write_buf` that\r\nuses vectored writes when supported and non-vectored writes when not\r\nsupported, so that users don't have to reimplement this.\r\n\r\n## Solution\r\n\r\nThis PR adds a `poll_write_buf` function to `tokio_util::io`, analogous\r\nto the existing `poll_read_buf` function.\r\n\r\nThis function writes from a `Buf` to an `AsyncWrite`, advancing the\r\n`Buf`'s internal cursor. In addition, when the `AsyncWrite` supports\r\nvectored writes (i.e. its `is_write_vectored` method returns `true`),\r\nit will use vectored IO.\r\n\r\nI copied the documentation for this functions from the docs from Tokio\r\n0.2's `AsyncWrite::poll_write_buf` , with some minor modifications as\r\nappropriate.\r\n\r\nFinally, I fixed a minor issue in the existing docs for `poll_read_buf`\r\nand `read_buf`, and updated `tokio_util::codec` to use `poll_write_buf`.\r\n\r\nSigned-off-by: Eliza Weisman <eliza@buoyant.io>",
          "timestamp": "2020-12-03T11:19:16-08:00",
          "tree_id": "c92df9ae491f0a444e694879858d032c3f6a5373",
          "url": "https://github.com/tokio-rs/tokio/commit/647299866a2262c8a1183adad73673e5803293ed"
        },
        "date": 1607023249692,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5964889,
            "range": "± 1849700",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6154756,
            "range": "± 1386405",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5626561,
            "range": "± 1636445",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 537,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 527,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 534,
            "range": "± 23",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 41397,
            "range": "± 1648",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 717,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 975316,
            "range": "± 2782",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 680636,
            "range": "± 7107",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "razican@protonmail.ch",
            "name": "Iban Eguia",
            "username": "Razican"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0dbba139848de6a8ee88350cc7fc48d0b05016c5",
          "message": "deps: replace lazy_static with once_cell (#3187)",
          "timestamp": "2020-12-04T10:23:13+01:00",
          "tree_id": "73f3366b9c7a0c50d6dd146a2626368cf59b3178",
          "url": "https://github.com/tokio-rs/tokio/commit/0dbba139848de6a8ee88350cc7fc48d0b05016c5"
        },
        "date": 1607073928614,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6620887,
            "range": "± 2131305",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6521558,
            "range": "± 1548961",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5832038,
            "range": "± 2362869",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 604,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 602,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 600,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 41299,
            "range": "± 3153",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 779,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1098827,
            "range": "± 56482",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 803541,
            "range": "± 12781",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "liufuyang@users.noreply.github.com",
            "name": "Fuyang Liu",
            "username": "liufuyang"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0707f4c19210d6dac620c663e94d34834714a7c9",
          "message": "net: add TcpStream::into_std (#3189)",
          "timestamp": "2020-12-06T14:33:04+01:00",
          "tree_id": "a3aff2f279b1e560602b4752435e092b4a22424e",
          "url": "https://github.com/tokio-rs/tokio/commit/0707f4c19210d6dac620c663e94d34834714a7c9"
        },
        "date": 1607261700663,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6973863,
            "range": "± 2083110",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6693247,
            "range": "± 1321175",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6075930,
            "range": "± 1654202",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 502,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 509,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 507,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 35162,
            "range": "± 5368",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 862,
            "range": "± 104",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1108634,
            "range": "± 37945",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 808080,
            "range": "± 114545",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "62023dffe5396ee1a0380f12c7530bf4ff59fe4a",
          "message": "sync: forward port 0.2 mpsc test (#3225)\n\nForward ports the test included in #3215. The mpsc sempahore has been\r\nreplaced in 0.3 and does not include the bug being fixed.",
          "timestamp": "2020-12-07T11:24:15-08:00",
          "tree_id": "c891a48ce299e6cfd01090a880d1baf16ebe0ad7",
          "url": "https://github.com/tokio-rs/tokio/commit/62023dffe5396ee1a0380f12c7530bf4ff59fe4a"
        },
        "date": 1607369177345,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6052259,
            "range": "± 1542207",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5867396,
            "range": "± 1176250",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5539724,
            "range": "± 2075553",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 535,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 530,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 530,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 44703,
            "range": "± 445",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 688,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 966609,
            "range": "± 1765",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 690380,
            "range": "± 2453",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "57dffb9dfe9e4c0f12429246540add3975f4a754",
          "message": "rt: fix deadlock in shutdown (#3228)\n\nPreviously, the runtime shutdown logic would first-hand control over all cores\r\nto a single thread, which would sequentially shut down all tasks on the core\r\nand then wait for them to complete.\r\n\r\nThis could deadlock when one task is waiting for a later core's task to\r\ncomplete. For example, in the newly added test, we have a `block_in_place` task\r\nthat is waiting for another task to be dropped. If the latter task adds its\r\ncore to the shutdown list later than the former, we end up waiting forever for\r\nthe `block_in_place` task to complete.\r\n\r\nAdditionally, there also was a bug wherein we'd attempt to park on the parker\r\nafter shutting it down which was fixed as part of the refactors above.\r\n\r\nThis change restructures the code to bring all tasks to a halt (and do any\r\nparking needed) before we collapse to a single thread to avoid this deadlock.\r\n\r\nThere was also an issue in which canceled tasks would not unpark the\r\noriginating thread, due to what appears to be some sort of optimization gone\r\nwrong. This has been fixed to be much more conservative in selecting when not\r\nto unpark the source thread (this may be too conservative; please take a look\r\nat the changes to `release()`).\r\n\r\nFixes: #2789",
          "timestamp": "2020-12-07T20:55:02-08:00",
          "tree_id": "1890e495daa058f06c8a738de4c88b0aeea52f77",
          "url": "https://github.com/tokio-rs/tokio/commit/57dffb9dfe9e4c0f12429246540add3975f4a754"
        },
        "date": 1607403412209,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6793764,
            "range": "± 2223193",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6960770,
            "range": "± 1327814",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6176210,
            "range": "± 1951380",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 768,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 755,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 761,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 47199,
            "range": "± 1700",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 824,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1115066,
            "range": "± 37817",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 805312,
            "range": "± 47500",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e01391351bcb0715f737cefe94e1bc99f19af226",
          "message": "Add stress test (#3222)\n\nCreated a simple echo TCP server that on two different runtimes that is\r\ncalled from a GitHub action using Valgrind to ensure that there are\r\nno memory leaks.\r\n\r\nFixes: #3022",
          "timestamp": "2020-12-07T21:12:22-08:00",
          "tree_id": "5575f27e36e49b887062119225e1d61335a01b9a",
          "url": "https://github.com/tokio-rs/tokio/commit/e01391351bcb0715f737cefe94e1bc99f19af226"
        },
        "date": 1607404445512,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5954439,
            "range": "± 2078083",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5700434,
            "range": "± 1453585",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5337747,
            "range": "± 1671028",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 463,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 461,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 461,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45469,
            "range": "± 416",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 798,
            "range": "± 120",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 949077,
            "range": "± 4029",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 680610,
            "range": "± 1149",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rodrigblas@gmail.com",
            "name": "Blas Rodriguez Irizar",
            "username": "blasrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9",
          "message": "chore: fix stress test (#3233)",
          "timestamp": "2020-12-09T07:38:25+09:00",
          "tree_id": "0b92fb11f764a5e88d62a9f79aa2107ebcb75f42",
          "url": "https://github.com/tokio-rs/tokio/commit/fc7a4b3c6e765d6d2b4ea97266cefbf466d52dc9"
        },
        "date": 1607467213186,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6769632,
            "range": "± 1883963",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6741123,
            "range": "± 1266824",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6170212,
            "range": "± 2111987",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 612,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 610,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 619,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 39394,
            "range": "± 2171",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 843,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1087764,
            "range": "± 62371",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 806009,
            "range": "± 10747",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bdonlan@gmail.com",
            "name": "bdonlan",
            "username": "bdonlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9706ca92a8deb69d6e29265f21424042fea966c5",
          "message": "time: Fix race condition in timer drop (#3229)\n\nDropping a timer on the millisecond that it was scheduled for, when it was on\r\nthe pending list, could result in a panic previously, as we did not record the\r\npending-list state in cached_when.\r\n\r\nHopefully fixes: ZcashFoundation/zebra#1452",
          "timestamp": "2020-12-08T16:42:43-08:00",
          "tree_id": "cd77e2148b7cdf03d0fcb38e8e27cf3f7eed1ed9",
          "url": "https://github.com/tokio-rs/tokio/commit/9706ca92a8deb69d6e29265f21424042fea966c5"
        },
        "date": 1607474764713,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7317557,
            "range": "± 3066683",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 7431812,
            "range": "± 2100179",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6505435,
            "range": "± 3180816",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 888,
            "range": "± 130",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 875,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 877,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 67029,
            "range": "± 6153",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1161,
            "range": "± 156",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1121079,
            "range": "± 187875",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 799965,
            "range": "± 96054",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "473ddaa277f51917aeb2fe743b1582685828d6dd",
          "message": "chore: prepare for Tokio 1.0 work (#3238)",
          "timestamp": "2020-12-09T09:42:05-08:00",
          "tree_id": "7af80d4c2bfffff4b6f04db875779e2f49f31280",
          "url": "https://github.com/tokio-rs/tokio/commit/473ddaa277f51917aeb2fe743b1582685828d6dd"
        },
        "date": 1607535854595,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6738675,
            "range": "± 2337391",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6599997,
            "range": "± 1320483",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6121600,
            "range": "± 2053191",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 657,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 651,
            "range": "± 53",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 631,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 38973,
            "range": "± 3872",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 817,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1080147,
            "range": "± 60006",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 766902,
            "range": "± 33649",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "52cd240053b2e1dd5835186539f563c3496dfd7d",
          "message": "task: add missing feature flags for task_local and spawn_blocking (#3237)",
          "timestamp": "2020-12-09T23:49:28+01:00",
          "tree_id": "bbc90b40091bd716d0269b84da2bafb32288b149",
          "url": "https://github.com/tokio-rs/tokio/commit/52cd240053b2e1dd5835186539f563c3496dfd7d"
        },
        "date": 1607554285261,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5701025,
            "range": "± 2101863",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5902471,
            "range": "± 1299005",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5312893,
            "range": "± 2066924",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 535,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 528,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 523,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 41296,
            "range": "± 1207",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 741,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 956677,
            "range": "± 1396",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 686286,
            "range": "± 4631",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2a30e13f38b864807f9ad92023e91b060a6227a4",
          "message": "net: expose poll_* methods on UnixDatagram (#3223)",
          "timestamp": "2020-12-10T08:36:43+01:00",
          "tree_id": "2ff07d9ed9f82c562e03ae7f13dd05150ffe899f",
          "url": "https://github.com/tokio-rs/tokio/commit/2a30e13f38b864807f9ad92023e91b060a6227a4"
        },
        "date": 1607585939997,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7259522,
            "range": "± 3012048",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6705222,
            "range": "± 1440966",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6426301,
            "range": "± 1925620",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 825,
            "range": "± 124",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 824,
            "range": "± 103",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 814,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 63351,
            "range": "± 5209",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1121,
            "range": "± 144",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1172286,
            "range": "± 110044",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 806782,
            "range": "± 90531",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f60860af7edefef5373d50d77ab605d648d60526",
          "message": "watch: fix spurious wakeup (#3234)\n\nCo-authored-by: @tijsvd",
          "timestamp": "2020-12-10T09:46:01+01:00",
          "tree_id": "44bc86bbaa5393a0dc3a94a2066569dcb1b79df1",
          "url": "https://github.com/tokio-rs/tokio/commit/f60860af7edefef5373d50d77ab605d648d60526"
        },
        "date": 1607590071451,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5678952,
            "range": "± 2085355",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5807312,
            "range": "± 1120951",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5605697,
            "range": "± 1581653",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 568,
            "range": "± 69",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 558,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 558,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45453,
            "range": "± 270",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 790,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 960254,
            "range": "± 2667",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 689678,
            "range": "± 3998",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "clemens.koza@gmx.at",
            "name": "Clemens Koza",
            "username": "SillyFreak"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9646b4bce342342cc654c4c0834c0bf3627f7aa0",
          "message": "toml: enable test-util feature for the playground (#3224)",
          "timestamp": "2020-12-10T10:39:05+01:00",
          "tree_id": "0c5c06ea6a86a13b9485506cf2066945eaf53189",
          "url": "https://github.com/tokio-rs/tokio/commit/9646b4bce342342cc654c4c0834c0bf3627f7aa0"
        },
        "date": 1607593240731,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5894707,
            "range": "± 2351465",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5795698,
            "range": "± 1429950",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5695541,
            "range": "± 1619266",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 535,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 533,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 534,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45042,
            "range": "± 356",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 688,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 954954,
            "range": "± 2690",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 687728,
            "range": "± 1206",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yusuktan@maguro.dev",
            "name": "Yusuke Tanaka",
            "username": "magurotuna"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4b1d76ec8f35052480eb14204d147df658bfdfdd",
          "message": "docs: fix typo in signal module documentation (#3249)",
          "timestamp": "2020-12-10T08:11:45-08:00",
          "tree_id": "46efd6f41cfaf702fb40c62b89800c511309d584",
          "url": "https://github.com/tokio-rs/tokio/commit/4b1d76ec8f35052480eb14204d147df658bfdfdd"
        },
        "date": 1607616829775,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6772718,
            "range": "± 2146756",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6790388,
            "range": "± 1584405",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6024027,
            "range": "± 1921333",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 728,
            "range": "± 160",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 723,
            "range": "± 176",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 737,
            "range": "± 194",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 52968,
            "range": "± 6646",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 931,
            "range": "± 200",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1096821,
            "range": "± 56719",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 791839,
            "range": "± 77235",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50183564+nylonicious@users.noreply.github.com",
            "name": "Nylonicious",
            "username": "nylonicious"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e",
          "message": "net: Pass SocketAddr by value (#3125)",
          "timestamp": "2020-12-10T14:58:27-05:00",
          "tree_id": "d46d58a79f31dba872aa060ef378743fcedea70e",
          "url": "https://github.com/tokio-rs/tokio/commit/16c2e0983cc0ab22f9a0b7a1ac37ea32a42b9a6e"
        },
        "date": 1607630412356,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6901006,
            "range": "± 3001410",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6919049,
            "range": "± 1403263",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6070651,
            "range": "± 2234634",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 716,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 720,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 717,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 52349,
            "range": "± 906",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 996,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1126610,
            "range": "± 9493",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 805696,
            "range": "± 2027",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "69e62ef89e481e0fb29ce3ef4ddce1eea4114000",
          "message": "sync: make TryAcquireError public (#3250)\n\nThe [`Semaphore::try_acquire`][1] method currently returns a private error type.\r\n\r\n[1]: https://docs.rs/tokio/0.3/tokio/sync/struct.Semaphore.html#method.try_acquire",
          "timestamp": "2020-12-10T19:56:05-08:00",
          "tree_id": "0784747565f6583a726c85dfedcd0527d8373cc6",
          "url": "https://github.com/tokio-rs/tokio/commit/69e62ef89e481e0fb29ce3ef4ddce1eea4114000"
        },
        "date": 1607659056958,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5883918,
            "range": "± 1966163",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5962076,
            "range": "± 1073729",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5617932,
            "range": "± 2406112",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 562,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 562,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 562,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 44870,
            "range": "± 376",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 713,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 954080,
            "range": "± 2791",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 688455,
            "range": "± 1691",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cameron.evan@gmail.com",
            "name": "Evan Cameron",
            "username": "leshow"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "68717c7efaced76651915696495dcb04c890be25",
          "message": "net: remove empty udp module (#3260)",
          "timestamp": "2020-12-11T14:45:57-05:00",
          "tree_id": "1b7333194ac78d7ae87c5ca9f423ef830cb486b8",
          "url": "https://github.com/tokio-rs/tokio/commit/68717c7efaced76651915696495dcb04c890be25"
        },
        "date": 1607716049611,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6174872,
            "range": "± 1685197",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5944085,
            "range": "± 1168658",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5366837,
            "range": "± 1835803",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 582,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 558,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 558,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 37582,
            "range": "± 3080",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 719,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 955598,
            "range": "± 2167",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 686343,
            "range": "± 1738",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b01b2dacf2e4136c0237977dac27a3688467d2ea",
          "message": "net: update `TcpStream::poll_peek` to use `ReadBuf` (#3259)\n\nCloses #2987",
          "timestamp": "2020-12-11T20:40:24-08:00",
          "tree_id": "1e0bbb86739731038cc9fd69fe112cad54662d16",
          "url": "https://github.com/tokio-rs/tokio/commit/b01b2dacf2e4136c0237977dac27a3688467d2ea"
        },
        "date": 1607748118186,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5796710,
            "range": "± 2020607",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5917670,
            "range": "± 1187167",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5576780,
            "range": "± 2107658",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 588,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 588,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 588,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45558,
            "range": "± 231",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 682,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 950997,
            "range": "± 1113",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 687342,
            "range": "± 1106",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c1ec469ad2af883b001d54e81dad426c01f918cd",
          "message": "util: add constructors to TokioContext (#3221)",
          "timestamp": "2020-12-11T20:41:22-08:00",
          "tree_id": "cdb1273c1a4eea6c7175578bc8a13f417c3daf00",
          "url": "https://github.com/tokio-rs/tokio/commit/c1ec469ad2af883b001d54e81dad426c01f918cd"
        },
        "date": 1607748202361,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5953347,
            "range": "± 1975519",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5833656,
            "range": "± 1603010",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5455117,
            "range": "± 1671063",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 623,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 632,
            "range": "± 210",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 632,
            "range": "± 100",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 43091,
            "range": "± 2867",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 687,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 936896,
            "range": "± 152207",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 671285,
            "range": "± 110640",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sunjay@users.noreply.github.com",
            "name": "Sunjay Varma",
            "username": "sunjay"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "df20c162ae1308c07073b6a67c8ba4202f52d208",
          "message": "sync: add blocking_recv method to UnboundedReceiver, similar to Receiver::blocking_recv (#3262)",
          "timestamp": "2020-12-12T08:47:35-08:00",
          "tree_id": "94fe5abd9735b0c4985d5b38a8d96c51953b0f0b",
          "url": "https://github.com/tokio-rs/tokio/commit/df20c162ae1308c07073b6a67c8ba4202f52d208"
        },
        "date": 1607791749733,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6247717,
            "range": "± 1989646",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6068051,
            "range": "± 970569",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5617133,
            "range": "± 1754676",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 530,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 516,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 513,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45721,
            "range": "± 320",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 736,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 958457,
            "range": "± 2915",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 689873,
            "range": "± 1221",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a",
          "message": "net: check for false-positives in TcpStream::ready doc test (#3255)",
          "timestamp": "2020-12-13T15:24:59+01:00",
          "tree_id": "89deb6d808e007e1728a43f0a198afe32a4aae1e",
          "url": "https://github.com/tokio-rs/tokio/commit/be2cb7a5ceb1d2abca4a69645d70ce7b6f97ee4a"
        },
        "date": 1607869596989,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5980183,
            "range": "± 1897111",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5811267,
            "range": "± 1330125",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5538000,
            "range": "± 1709234",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 554,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 552,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 549,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 45641,
            "range": "± 365",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 621,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 955071,
            "range": "± 2258",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 687187,
            "range": "± 1969",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40773946+faith@users.noreply.github.com",
            "name": "Aldas",
            "username": "faith"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f26f444f42c9369bcf8afe14ea00ea53dd663cc2",
          "message": "doc: added tracing to the feature flags section (#3254)",
          "timestamp": "2020-12-13T15:26:56+01:00",
          "tree_id": "23e7ae2599b67a6ab6faf0586f568b0ff6e0c72d",
          "url": "https://github.com/tokio-rs/tokio/commit/f26f444f42c9369bcf8afe14ea00ea53dd663cc2"
        },
        "date": 1607869726419,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6175381,
            "range": "± 2235410",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6133687,
            "range": "± 1048167",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5593387,
            "range": "± 1390615",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 504,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 499,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 498,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 39332,
            "range": "± 1717",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 698,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 954229,
            "range": "± 3006",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 688299,
            "range": "± 1728",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cssivision@gmail.com",
            "name": "cssivision",
            "username": "cssivision"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4c5541945348c4614c29a13c06f2e996eb419e42",
          "message": "net: add UnixStream readiness and non-blocking ops (#3246)",
          "timestamp": "2020-12-13T16:21:11+01:00",
          "tree_id": "355917f9e8ba45094e1a3e1f465875f51f56e255",
          "url": "https://github.com/tokio-rs/tokio/commit/4c5541945348c4614c29a13c06f2e996eb419e42"
        },
        "date": 1607873013984,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7110752,
            "range": "± 2664338",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 7266688,
            "range": "± 1314897",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6612772,
            "range": "± 2064951",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 834,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 840,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 837,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 65295,
            "range": "± 5391",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1073,
            "range": "± 102",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1189275,
            "range": "± 89924",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 828774,
            "range": "± 55967",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "6172808+skerkour@users.noreply.github.com",
            "name": "Sylvain Kerkour",
            "username": "skerkour"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9149d7bfae251289cd21aa9ee109b4e2a190d0fa",
          "message": "docs: mention blocking thread timeout in src/lib.rs (#3253)",
          "timestamp": "2020-12-13T16:24:16+01:00",
          "tree_id": "38b69f17cc4644ac6ca081aa1d88d5cfe35825fa",
          "url": "https://github.com/tokio-rs/tokio/commit/9149d7bfae251289cd21aa9ee109b4e2a190d0fa"
        },
        "date": 1607873152638,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5961643,
            "range": "± 2043431",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6050719,
            "range": "± 1362743",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5733492,
            "range": "± 1768262",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 533,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 532,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 531,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 37641,
            "range": "± 2088",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 722,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 939242,
            "range": "± 2090",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 675195,
            "range": "± 5636",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jacob@jotpot.co.uk",
            "name": "Jacob O'Toole",
            "username": "JOT85"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1f862d2e950bf5f4fb31f202a544e86880f51191",
          "message": "sync: add `watch::Sender::borrow()` (#3269)",
          "timestamp": "2020-12-13T20:46:11-08:00",
          "tree_id": "e203a63415d1b02e7b62f73f8ebeebedeaaef82d",
          "url": "https://github.com/tokio-rs/tokio/commit/1f862d2e950bf5f4fb31f202a544e86880f51191"
        },
        "date": 1607921273658,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5627975,
            "range": "± 2155424",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5307130,
            "range": "± 1378745",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 4837573,
            "range": "± 2734525",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 652,
            "range": "± 90",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 647,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 647,
            "range": "± 69",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 53993,
            "range": "± 11163",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 718,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 826535,
            "range": "± 192225",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 590086,
            "range": "± 126766",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8efa62013b551d5130791c3a79ce8ab5cb0b5abf",
          "message": "Move stream items into `tokio-stream` (#3277)\n\nThis change removes all references to `Stream` from\r\nwithin the `tokio` crate and moves them into a new\r\n`tokio-stream` crate. Most types have had their\r\n`impl Stream` removed as well in-favor of their\r\ninherent methods.\r\n\r\nCloses #2870",
          "timestamp": "2020-12-15T20:24:38-08:00",
          "tree_id": "6da8c41c8e1808bea98fd2d23ee1ec03a1cc7e80",
          "url": "https://github.com/tokio-rs/tokio/commit/8efa62013b551d5130791c3a79ce8ab5cb0b5abf"
        },
        "date": 1608092787685,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5873634,
            "range": "± 1771401",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6079296,
            "range": "± 1074230",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5486375,
            "range": "± 2058283",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 490,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 494,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 489,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 71780,
            "range": "± 3664",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1202,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 955663,
            "range": "± 3534",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 687562,
            "range": "± 4665",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d74d17307dd53215061c4a8a1f20a0e30461e296",
          "message": "time: remove `Box` from `Sleep` (#3278)\n\nRemoves the box from `Sleep`, taking advantage of intrusive wakers. The\r\n`Sleep` future is now `!Unpin`.\r\n\r\nCloses #3267",
          "timestamp": "2020-12-16T21:51:34-08:00",
          "tree_id": "0cdbf57e4a9b38302ddae0078eb5a1b9a4977aa2",
          "url": "https://github.com/tokio-rs/tokio/commit/d74d17307dd53215061c4a8a1f20a0e30461e296"
        },
        "date": 1608184437858,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6908110,
            "range": "± 2935094",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6928476,
            "range": "± 2003228",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6774653,
            "range": "± 2411046",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 926,
            "range": "± 157",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 936,
            "range": "± 134",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 923,
            "range": "± 248",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 138586,
            "range": "± 8620",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1739,
            "range": "± 292",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1097788,
            "range": "± 136232",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 772982,
            "range": "± 90549",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "59e4b35f49e6a95a961fd9003db58191de97d3a0",
          "message": "stream: Fix a few doc issues (#3285)",
          "timestamp": "2020-12-17T10:35:49-05:00",
          "tree_id": "757b133c0eac5b1c26d2ba870d0f4b5c198d7505",
          "url": "https://github.com/tokio-rs/tokio/commit/59e4b35f49e6a95a961fd9003db58191de97d3a0"
        },
        "date": 1608219477083,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6901253,
            "range": "± 2923837",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6692569,
            "range": "± 1177919",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6468488,
            "range": "± 2452441",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 583,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 581,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 581,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 83938,
            "range": "± 4567",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1418,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1142167,
            "range": "± 8906",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 826597,
            "range": "± 7472",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8",
          "message": "docs: Add more comprehensive stream docs (#3286)\n\n* docs: Add more comprehensive stream docs\r\n\r\n* Apply suggestions from code review\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>\r\n\r\n* Fix doc tests\r\n\r\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-17T11:46:09-05:00",
          "tree_id": "f7afa84006c8629a0d2c058b8e52042c54436203",
          "url": "https://github.com/tokio-rs/tokio/commit/c5861ef62fb0c1ed6bb8fe07a8e02534264eb7b8"
        },
        "date": 1608223681896,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5989541,
            "range": "± 1807389",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5808684,
            "range": "± 1213654",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5547191,
            "range": "± 1753269",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 511,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 510,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 510,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 70711,
            "range": "± 3370",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1280,
            "range": "± 177",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 940357,
            "range": "± 3997",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 685099,
            "range": "± 7647",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "abd4c0025539f142ec48a09e01430f7ee3b83214",
          "message": "time: enforce `current_thread` rt for time::pause (#3289)\n\nPausing time is a capability added to assist with testing Tokio code\r\ndependent on time. Currently, the capability implicitly requires the\r\ncurrent_thread runtime.\r\n\r\nThis change enforces the requirement by panicking if called from a\r\nmulti-threaded runtime.",
          "timestamp": "2020-12-17T15:37:08-08:00",
          "tree_id": "6c565d6c74dff336ac847cb6463245283d8470d5",
          "url": "https://github.com/tokio-rs/tokio/commit/abd4c0025539f142ec48a09e01430f7ee3b83214"
        },
        "date": 1608248350793,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7195114,
            "range": "± 2043265",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6904592,
            "range": "± 1337050",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6559100,
            "range": "± 1858124",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 716,
            "range": "± 100",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 671,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 672,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 84063,
            "range": "± 6061",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1448,
            "range": "± 346",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1141003,
            "range": "± 9863",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 818448,
            "range": "± 2677",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3ecaf9fd9a519da9b1decb4c7239b48277040522",
          "message": "codec: write documentation for codec (#3283)\n\nCo-authored-by: Lucio Franco <luciofranco14@gmail.com>",
          "timestamp": "2020-12-18T21:32:27+01:00",
          "tree_id": "f116db611cb8144b9ec3e2e97286aab59cdd9556",
          "url": "https://github.com/tokio-rs/tokio/commit/3ecaf9fd9a519da9b1decb4c7239b48277040522"
        },
        "date": 1608323668149,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6520234,
            "range": "± 1790207",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6002660,
            "range": "± 1251782",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5623098,
            "range": "± 2013594",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 579,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 592,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 595,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 72039,
            "range": "± 4820",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1264,
            "range": "± 165",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 951998,
            "range": "± 5983",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 689296,
            "range": "± 29919",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d948ccedfce534953a18acf46c8c6572103567c7",
          "message": "chore: fix stress test (#3297)",
          "timestamp": "2020-12-19T12:11:10+01:00",
          "tree_id": "3c417da4134a45bfff1f2d85b9b8cf410dfd9bf9",
          "url": "https://github.com/tokio-rs/tokio/commit/d948ccedfce534953a18acf46c8c6572103567c7"
        },
        "date": 1608376400045,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6511462,
            "range": "± 2819552",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6414887,
            "range": "± 1856186",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6170157,
            "range": "± 2088106",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 835,
            "range": "± 185",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 836,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 837,
            "range": "± 163",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 129968,
            "range": "± 12372",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1797,
            "range": "± 201",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1077746,
            "range": "± 138254",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 748308,
            "range": "± 138418",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luciofranco14@gmail.com",
            "name": "Lucio Franco",
            "username": "LucioFranco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b99b00eb302ae6ff19ca97d32b1e594143f43a60",
          "message": "rt: change `max_threads` to `max_blocking_threads` (#3287)\n\nFixes #2802",
          "timestamp": "2020-12-19T08:04:04-08:00",
          "tree_id": "458d7fb55f921184a1056e766b6d0101fb763579",
          "url": "https://github.com/tokio-rs/tokio/commit/b99b00eb302ae6ff19ca97d32b1e594143f43a60"
        },
        "date": 1608393941200,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5928391,
            "range": "± 2620032",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5704310,
            "range": "± 1449151",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5745525,
            "range": "± 2173272",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 589,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 588,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 586,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 72371,
            "range": "± 3417",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1160,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 941341,
            "range": "± 2874",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 687411,
            "range": "± 17098",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "flo.huebsch@pm.me",
            "name": "Florian Hübsch",
            "username": "fl9"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e41e6cddbba0cf13403924937ffe02aae6639e28",
          "message": "docs: tokio::main macro is also supported on rt (#3243)\n\nFixes: #3144\r\nRefs: #2225",
          "timestamp": "2020-12-19T19:12:08+01:00",
          "tree_id": "ee1af0c8a3b2ab9c9eaae05f2dca96ff966f42f9",
          "url": "https://github.com/tokio-rs/tokio/commit/e41e6cddbba0cf13403924937ffe02aae6639e28"
        },
        "date": 1608401671508,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6145929,
            "range": "± 2562212",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5659201,
            "range": "± 1504037",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5995757,
            "range": "± 2154165",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 655,
            "range": "± 91",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 663,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 678,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 122643,
            "range": "± 9493",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1572,
            "range": "± 116",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 964386,
            "range": "± 40760",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 692754,
            "range": "± 54886",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "78f2340d259487d4470681f97cc4b5eac719e178",
          "message": "tokio: remove prelude (#3299)\n\nCloses: #3257",
          "timestamp": "2020-12-19T11:42:24-08:00",
          "tree_id": "2a093e994a41bae54e324fd8d20c836e46a5947c",
          "url": "https://github.com/tokio-rs/tokio/commit/78f2340d259487d4470681f97cc4b5eac719e178"
        },
        "date": 1608407058018,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6938342,
            "range": "± 2501112",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6364996,
            "range": "± 1805568",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5775769,
            "range": "± 2592573",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 629,
            "range": "± 121",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 669,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 652,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 82663,
            "range": "± 14203",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1545,
            "range": "± 463",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1122519,
            "range": "± 118714",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 818353,
            "range": "± 100587",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "arve.knudsen@gmail.com",
            "name": "Arve Knudsen",
            "username": "aknuds1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1b70507894035e066cb488c14ff328bd47ca696d",
          "message": "net: remove {Tcp,Unix}Stream::shutdown() (#3298)\n\n`shutdown()` on `AsyncWrite` performs a TCP shutdown. This avoids method\r\nconflicts.\r\n\r\nCloses #3294",
          "timestamp": "2020-12-19T12:17:52-08:00",
          "tree_id": "dd3c414761b8a3713b63515e7666f108a1391743",
          "url": "https://github.com/tokio-rs/tokio/commit/1b70507894035e066cb488c14ff328bd47ca696d"
        },
        "date": 1608409186215,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7233913,
            "range": "± 2727610",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6703943,
            "range": "± 1675365",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6679825,
            "range": "± 2792964",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 639,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 644,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 652,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 84547,
            "range": "± 14221",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1494,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1117340,
            "range": "± 100791",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 816460,
            "range": "± 104380",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5e5f513542c0f92f49f6acd31021cc056577be6b",
          "message": "chore: remove some left over `stream` feature code (#3300)\n\nRemoves the `stream` feature flag from `Cargo.toml` and removes the\r\n`futures-core` dependency. Once `Stream` lands in `std`, a feature flag\r\nis most likely not needed.",
          "timestamp": "2020-12-19T14:15:00-08:00",
          "tree_id": "bb5486367eec9a81b5d021202eaf8ab0fe34bdc8",
          "url": "https://github.com/tokio-rs/tokio/commit/5e5f513542c0f92f49f6acd31021cc056577be6b"
        },
        "date": 1608416218718,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6492835,
            "range": "± 2983454",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6758909,
            "range": "± 1904281",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6237752,
            "range": "± 2579266",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 603,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 606,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 648,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 82004,
            "range": "± 4926",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1380,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1120065,
            "range": "± 37939",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 816203,
            "range": "± 11541",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "28933599888a88e601acbb11fa824b0ee9f98c6e",
          "message": "chore: update to `bytes` 1.0 git branch (#3301)\n\nUpdates the code base to track the `bytes` git branch. This is in\r\npreparation for the 1.0 release.\r\n\r\nCloses #3058",
          "timestamp": "2020-12-19T15:57:16-08:00",
          "tree_id": "2021ef3acf9407fcfa39032e0a493a81f1eb74cc",
          "url": "https://github.com/tokio-rs/tokio/commit/28933599888a88e601acbb11fa824b0ee9f98c6e"
        },
        "date": 1608422325042,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6033145,
            "range": "± 2756062",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5878919,
            "range": "± 1257941",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5758138,
            "range": "± 1922068",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 528,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 537,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 538,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 73605,
            "range": "± 5411",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1196,
            "range": "± 125",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 949580,
            "range": "± 3080",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 674192,
            "range": "± 2575",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bIgBV@users.noreply.github.com",
            "name": "Bhargav",
            "username": "bIgBV"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c7671a03840751f58b7a509386b8fe3b5e670a37",
          "message": "io: add _mut variants of methods on AsyncFd (#3304)\n\nCo-authored-by: Alice Ryhl <alice@ryhl.io>",
          "timestamp": "2020-12-21T22:51:28+01:00",
          "tree_id": "9d3f26151c9ddd292c129f43c0fac83792073f62",
          "url": "https://github.com/tokio-rs/tokio/commit/c7671a03840751f58b7a509386b8fe3b5e670a37"
        },
        "date": 1608587603668,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6783986,
            "range": "± 2854254",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6600011,
            "range": "± 1187726",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6483181,
            "range": "± 2154498",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 685,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 652,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 653,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 86586,
            "range": "± 15193",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1446,
            "range": "± 188",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1140068,
            "range": "± 23864",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 822555,
            "range": "± 11110",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "564c943309f0a94474e2e71a115ec0bb45f3e7fd",
          "message": "io: rename `AsyncFd::with_io()` and rm `with_poll()` (#3306)",
          "timestamp": "2020-12-21T15:42:38-08:00",
          "tree_id": "6f29310dda04438222fb9581b3cd9581f1abe13e",
          "url": "https://github.com/tokio-rs/tokio/commit/564c943309f0a94474e2e71a115ec0bb45f3e7fd"
        },
        "date": 1608594270957,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6876973,
            "range": "± 2444264",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 7110984,
            "range": "± 1492469",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6960961,
            "range": "± 2332350",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 644,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 643,
            "range": "± 53",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 644,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 83666,
            "range": "± 3769",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1582,
            "range": "± 270",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1126536,
            "range": "± 44750",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 829814,
            "range": "± 37634",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "eee3ca65d6e5b4537915571f663f94184e616e6c",
          "message": "deps: update rand to 0.8, loom to 0.4 (#3307)",
          "timestamp": "2020-12-22T10:28:35+01:00",
          "tree_id": "2f534ebe6cb319f30f72e7a9b2b389825500f051",
          "url": "https://github.com/tokio-rs/tokio/commit/eee3ca65d6e5b4537915571f663f94184e616e6c"
        },
        "date": 1608629432918,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6256650,
            "range": "± 2778553",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6396567,
            "range": "± 1742610",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6304147,
            "range": "± 2457348",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 537,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 558,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 581,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 81504,
            "range": "± 28386",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1498,
            "range": "± 240",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1052539,
            "range": "± 237250",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 745688,
            "range": "± 103350",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f95ad1898041d2bcc09de3db69228e88f5f4abf8",
          "message": "net: clarify when wakeups are sent (#3310)",
          "timestamp": "2020-12-22T07:38:14-08:00",
          "tree_id": "16968a03d8ca55c53da7658c109e47a5f9a8c4e4",
          "url": "https://github.com/tokio-rs/tokio/commit/f95ad1898041d2bcc09de3db69228e88f5f4abf8"
        },
        "date": 1608651631712,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7178522,
            "range": "± 3051595",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6564881,
            "range": "± 1850921",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6339675,
            "range": "± 2971219",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 832,
            "range": "± 115",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 820,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 828,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 142093,
            "range": "± 16782",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1720,
            "range": "± 320",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1039930,
            "range": "± 157923",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 750934,
            "range": "± 153641",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alice@ryhl.io",
            "name": "Alice Ryhl",
            "username": "Darksonn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b83b3b8cc61e9d911abec00c33ec3762b2a6437",
          "message": "fs,sync: expose poll_ fns on misc types (#3308)\n\nIncludes methods on:\r\n\r\n* fs::DirEntry\r\n* io::Lines\r\n* io::Split\r\n* sync::mpsc::Receiver\r\n* sync::misc::UnboundedReceiver",
          "timestamp": "2020-12-22T09:28:14-08:00",
          "tree_id": "bd1ae38a53efb2fe78fc8aba0bdd0789cc7a1495",
          "url": "https://github.com/tokio-rs/tokio/commit/0b83b3b8cc61e9d911abec00c33ec3762b2a6437"
        },
        "date": 1608658205371,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6122156,
            "range": "± 2023177",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5787558,
            "range": "± 1397204",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5741329,
            "range": "± 1981629",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 543,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 540,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 518,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 69785,
            "range": "± 1995",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1231,
            "range": "± 130",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 956657,
            "range": "± 5880",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 688794,
            "range": "± 13448",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "carlos.baezruiz@gmail.com",
            "name": "Carlos B",
            "username": "carlosb1"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d28e4cdbb63b0ad19e66d1b077690cbebf71353",
          "message": "examples: add futures executor threadpool (#3198)",
          "timestamp": "2020-12-22T20:08:43+01:00",
          "tree_id": "706d894cb4ea97ae1a91b9d2bd42d1800968559c",
          "url": "https://github.com/tokio-rs/tokio/commit/7d28e4cdbb63b0ad19e66d1b077690cbebf71353"
        },
        "date": 1608664227568,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6048311,
            "range": "± 2103116",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5965596,
            "range": "± 1621768",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5764013,
            "range": "± 2030694",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 562,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 560,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 550,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 70073,
            "range": "± 4074",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1200,
            "range": "± 116",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 950928,
            "range": "± 2496",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 683962,
            "range": "± 6414",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608664572463,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 7409603,
            "range": "± 3511645",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 7037871,
            "range": "± 1976099",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6290464,
            "range": "± 3878532",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 945,
            "range": "± 111",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 953,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 940,
            "range": "± 93",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 146786,
            "range": "± 21091",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1835,
            "range": "± 161",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1183274,
            "range": "± 140496",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 826800,
            "range": "± 111222",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2ee9520d10c1089230fc66eaa81f0574038faa82",
          "message": "fs: misc small API tweaks (#3315)",
          "timestamp": "2020-12-22T11:13:41-08:00",
          "tree_id": "0cfded0285a4943e6aac2fd843965f48e9ccc4ff",
          "url": "https://github.com/tokio-rs/tokio/commit/2ee9520d10c1089230fc66eaa81f0574038faa82"
        },
        "date": 1608666611682,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6060103,
            "range": "± 2214087",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5816635,
            "range": "± 1278812",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5708997,
            "range": "± 1957959",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 542,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 542,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 540,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 69391,
            "range": "± 5936",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1180,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 953451,
            "range": "± 3080",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 699843,
            "range": "± 5480",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "sean@seanmonstar.com",
            "name": "Sean McArthur",
            "username": "seanmonstar"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b",
          "message": "time: make Interval::poll_tick() public (#3316)",
          "timestamp": "2020-12-22T12:31:14-08:00",
          "tree_id": "c06c2c6a1618d8dd177cd844f8f816f06e6033b8",
          "url": "https://github.com/tokio-rs/tokio/commit/be9fdb697dd62e00ad3a9e492778c1b5af7cbf0b"
        },
        "date": 1608669192046,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6188928,
            "range": "± 2646469",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6269508,
            "range": "± 1934157",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6076907,
            "range": "± 2118327",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 612,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 615,
            "range": "± 86",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 618,
            "range": "± 85",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 78480,
            "range": "± 13996",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1303,
            "range": "± 295",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1047645,
            "range": "± 161742",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 742158,
            "range": "± 111115",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "luke.steensen@gmail.com",
            "name": "Luke Steensen",
            "username": "lukesteensen"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a8dda19da4e94acd45f34b5eb359b4cffafa833f",
          "message": "chore: update to released `bytes` 1.0 (#3317)",
          "timestamp": "2020-12-22T17:09:26-08:00",
          "tree_id": "c177db0f9bced11086bcb13be4ac2348e6c94469",
          "url": "https://github.com/tokio-rs/tokio/commit/a8dda19da4e94acd45f34b5eb359b4cffafa833f"
        },
        "date": 1608685873446,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 5788934,
            "range": "± 2069605",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6121584,
            "range": "± 1338636",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5631968,
            "range": "± 1541457",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 488,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 486,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 487,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 73273,
            "range": "± 3731",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1171,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 950292,
            "range": "± 3366",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 684526,
            "range": "± 2340",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0deaeb84948f253b76b7fe64d7fe9d4527cd4275",
          "message": "chore: remove unused `slab` dependency (#3318)",
          "timestamp": "2020-12-22T21:56:22-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/0deaeb84948f253b76b7fe64d7fe9d4527cd4275"
        },
        "date": 1608703087151,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6105530,
            "range": "± 2403996",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5637052,
            "range": "± 1175055",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5805114,
            "range": "± 2167239",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 494,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 470,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 465,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 71026,
            "range": "± 3456",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1229,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 948941,
            "range": "± 2154",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 682966,
            "range": "± 4537",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "575938d4579e6fe6a89b700aadb0ae2bbab5483b",
          "message": "chore: use #[non_exhaustive] instead of private unit field (#3320)",
          "timestamp": "2020-12-23T22:48:33+09:00",
          "tree_id": "f2782c6135a26568d5dea4d45b3310baaf062e16",
          "url": "https://github.com/tokio-rs/tokio/commit/575938d4579e6fe6a89b700aadb0ae2bbab5483b"
        },
        "date": 1608731415827,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6620481,
            "range": "± 2703673",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6715561,
            "range": "± 1698493",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 6714635,
            "range": "± 2299935",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 634,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 633,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 622,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 85027,
            "range": "± 4427",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1379,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1131651,
            "range": "± 31841",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 831430,
            "range": "± 19346",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "te316e89@gmail.com",
            "name": "Taiki Endo",
            "username": "taiki-e"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ce0e9c67cfe61c7a91a284331ecc53fa01c32879",
          "message": "chore: Revert \"use #[non_exhaustive] instead of private unit field\" (#3323)\n\nThis reverts commit 575938d4579e6fe6a89b700aadb0ae2bbab5483b.",
          "timestamp": "2020-12-23T08:27:58-08:00",
          "tree_id": "3b9ad84403d71b2ad5d2c749c23516c3dfaec3ce",
          "url": "https://github.com/tokio-rs/tokio/commit/ce0e9c67cfe61c7a91a284331ecc53fa01c32879"
        },
        "date": 1608740986911,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6092475,
            "range": "± 2067448",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 5729332,
            "range": "± 1260403",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5508239,
            "range": "± 2387294",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 593,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 573,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 571,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 70151,
            "range": "± 5583",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 1148,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 939298,
            "range": "± 2104",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 683635,
            "range": "± 22479",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}