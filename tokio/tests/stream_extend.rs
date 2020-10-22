use std::collections::HashMap;

use tokio::stream;

#[tokio::test]
async fn vec_extend_stream() {
    let s = stream::iter(vec![0, 2, 4, 6]);
    let mut buff = vec![-2];
    stream::extend(&mut buff, s).await;
    assert_eq!(buff, vec![-2, 0, 2, 4, 6]);
}

#[tokio::test]
async fn hash_map_extend_stream() {
    let hm: HashMap<String, i32> = vec![("one".to_string(), 1)].into_iter().collect();
    let s = stream::iter(hm);

    let mut buff: HashMap<String, i32> = vec![("zero".to_string(), 0)].into_iter().collect();
    buff.insert("zero".to_string(), 0);
    stream::extend(&mut buff, s).await;

    let hm_expected: HashMap<_, _> = vec![("zero".to_string(), 0), ("one".to_string(), 1)]
        .into_iter()
        .collect();
    assert_eq!(hm_expected, buff);
}
