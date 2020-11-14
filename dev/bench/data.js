window.BENCHMARK_DATA = {
  "lastUpdate": 1605312920320,
  "repoUrl": "https://github.com/tokio-rs/tokio",
  "entries": {
    "Tokio Benchmarks": [
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
          "id": "39f7d89f11d4b89fabff4a84646f2a46a4eb1ea0",
          "message": "try again",
          "timestamp": "2020-11-13T16:01:17-08:00",
          "tree_id": "bff434a6d38dd1e828df2856a4f80d6838d94900",
          "url": "https://github.com/tokio-rs/tokio/commit/39f7d89f11d4b89fabff4a84646f2a46a4eb1ea0"
        },
        "date": 1605312298875,
        "tool": "cargo",
        "benches": [
          {
            "name": "contention_bounded",
            "value": 6332322,
            "range": "± 1926674",
            "unit": "ns/iter"
          },
          {
            "name": "contention_bounded_full",
            "value": 6517289,
            "range": "± 1230065",
            "unit": "ns/iter"
          },
          {
            "name": "contention_unbounded",
            "value": 5949566,
            "range": "± 1925757",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_000_medium",
            "value": 521,
            "range": "± 72",
            "unit": "ns/iter"
          },
          {
            "name": "create_100_medium",
            "value": 530,
            "range": "± 68",
            "unit": "ns/iter"
          },
          {
            "name": "create_1_medium",
            "value": 530,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "send_large",
            "value": 40723,
            "range": "± 3343",
            "unit": "ns/iter"
          },
          {
            "name": "send_medium",
            "value": 786,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_bounded",
            "value": 1040319,
            "range": "± 114234",
            "unit": "ns/iter"
          },
          {
            "name": "uncontented_unbounded",
            "value": 752520,
            "range": "± 89542",
            "unit": "ns/iter"
          },
          {
            "name": "chained_spawn",
            "value": 178198,
            "range": "± 14750",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 706035,
            "range": "± 78186",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5365977,
            "range": "± 742507",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19909759,
            "range": "± 2392145",
            "unit": "ns/iter"
          },
          {
            "name": "many_signals",
            "value": 26900,
            "range": "± 2579",
            "unit": "ns/iter"
          },
          {
            "name": "basic_scheduler_local_spawn",
            "value": 121,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "basic_scheduler_remote_spawn",
            "value": 137,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "threaded_scheduler_local_spawn",
            "value": 127,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "threaded_scheduler_remote_spawn",
            "value": 383,
            "range": "± 223",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended",
            "value": 978,
            "range": "± 182",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_contended_multi",
            "value": 14209,
            "range": "± 4929",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended",
            "value": 982,
            "range": "± 137",
            "unit": "ns/iter"
          },
          {
            "name": "read_concurrent_uncontended_multi",
            "value": 14600,
            "range": "± 3949",
            "unit": "ns/iter"
          },
          {
            "name": "read_uncontended",
            "value": 554,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_multi",
            "value": 14696,
            "range": "± 3300",
            "unit": "ns/iter"
          },
          {
            "name": "contended_concurrent_single",
            "value": 1027,
            "range": "± 126",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended",
            "value": 594,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_multi",
            "value": 15909,
            "range": "± 5272",
            "unit": "ns/iter"
          },
          {
            "name": "uncontended_concurrent_single",
            "value": 1014,
            "range": "± 112",
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
            "email": "me@carllerche.com",
            "name": "Carl Lerche",
            "username": "carllerche"
          },
          "distinct": true,
          "id": "3a758470548e2dffffa3ce0f8e8904ac0a5ddc59",
          "message": "try matrix build",
          "timestamp": "2020-11-13T16:13:41-08:00",
          "tree_id": "992b8465cb5d1b7783a03ca750c97a809adee1b8",
          "url": "https://github.com/tokio-rs/tokio/commit/3a758470548e2dffffa3ce0f8e8904ac0a5ddc59"
        },
        "date": 1605312919492,
        "tool": "cargo",
        "benches": [
          {
            "name": "chained_spawn",
            "value": 186232,
            "range": "± 15379",
            "unit": "ns/iter"
          },
          {
            "name": "ping_pong",
            "value": 680951,
            "range": "± 60516",
            "unit": "ns/iter"
          },
          {
            "name": "spawn_many",
            "value": 5362416,
            "range": "± 1163763",
            "unit": "ns/iter"
          },
          {
            "name": "yield_many",
            "value": 19844934,
            "range": "± 2274059",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}