window.BENCHMARK_DATA = {
  "lastUpdate": 1605314961323,
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
      }
    ]
  }
}