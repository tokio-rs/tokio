#[cfg(all(tokio_unstable, feature = "tracing"))]
mod tests {
    use std::rc::Rc;
    use tokio::task::{Builder, LocalSet};

    #[tokio::test]
    async fn spawn_with_name() {
        let result = Builder::new()
            .name("name")
            .spawn(async { "task executed" })
            .unwrap()
            .await;

        assert_eq!(result.unwrap(), "task executed");
    }

    #[tokio::test]
    async fn spawn_blocking_with_name() {
        let result = Builder::new()
            .name("name")
            .spawn_blocking(|| "task executed")
            .unwrap()
            .await;

        assert_eq!(result.unwrap(), "task executed");
    }

    #[tokio::test]
    async fn spawn_local_with_name() {
        let unsend_data = Rc::new("task executed");
        let result = LocalSet::new()
            .run_until(async move {
                Builder::new()
                    .name("name")
                    .spawn_local(async move { unsend_data })
                    .unwrap()
                    .await
            })
            .await;

        assert_eq!(*result.unwrap(), "task executed");
    }

    #[tokio::test]
    async fn spawn_without_name() {
        let result = Builder::new()
            .spawn(async { "task executed" })
            .unwrap()
            .await;

        assert_eq!(result.unwrap(), "task executed");
    }

    #[tokio::test]
    async fn spawn_blocking_without_name() {
        let result = Builder::new()
            .spawn_blocking(|| "task executed")
            .unwrap()
            .await;

        assert_eq!(result.unwrap(), "task executed");
    }

    #[tokio::test]
    async fn spawn_local_without_name() {
        let unsend_data = Rc::new("task executed");
        let result = LocalSet::new()
            .run_until(async move {
                Builder::new()
                    .spawn_local(async move { unsend_data })
                    .unwrap()
                    .await
            })
            .await;

        assert_eq!(*result.unwrap(), "task executed");
    }

    #[test]
    fn spawn_yields_error_if_shutdown() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let handle = rt.handle().to_owned();

        // Shut down the runtime
        drop(rt);

        fn blocking_task() {
            panic!("should not run");
        }

        async fn task() {
            panic!("should not run");
        }

        {
            let b = Builder::new();
            let _guard = handle.enter();
            assert!(b.spawn(task()).unwrap_err().is_shutdown());
        }
        {
            let b = Builder::new();
            let _guard = handle.enter();
            assert!(b.spawn_blocking(blocking_task).unwrap_err().is_shutdown());
        }

        {
            let b = Builder::new();
            assert!(b.spawn_on(task(), &handle).unwrap_err().is_shutdown());
        }
        {
            let b = Builder::new();
            assert!(b
                .spawn_blocking_on(blocking_task, &handle)
                .unwrap_err()
                .is_shutdown());
        }
    }
}
