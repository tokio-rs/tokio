use crate::runtime::Builder;
use crate::task::TaskSet;

#[test]
fn test_task_set() {
    loom::model(|| {
        let rt = Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let mut set = TaskSet::new();

        rt.block_on(async {
            assert_eq!(set.len(), 0);
            set.spawn(async { () });
            assert_eq!(set.len(), 1);
            set.spawn(async { () });
            assert_eq!(set.len(), 2);
            let () = set.join_one().await.unwrap().unwrap();
            assert_eq!(set.len(), 1);
            set.spawn(async { () });
            assert_eq!(set.len(), 2);
            let () = set.join_one().await.unwrap().unwrap();
            assert_eq!(set.len(), 1);
            let () = set.join_one().await.unwrap().unwrap();
            assert_eq!(set.len(), 0);
            set.spawn(async { () });
            assert_eq!(set.len(), 1);
        });

        drop(set);
        drop(rt);
    });
}
