#![allow(unknown_lints, unexpected_cfgs)]

#[cfg(tokio_unstable)]
mod unstable {
    use std::{
        future::Future,
        sync::{atomic::AtomicUsize, Arc, Mutex},
    };

    use tokio::task::yield_now;

    pin_project_lite::pin_project! {
        struct PollCounter<F> {
            #[pin]
            inner: F,
            counter: Arc<AtomicUsize>,
        }
    }

    impl<F: Future> Future for PollCounter<F> {
        type Output = F::Output;

        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            let this = self.project();
            this.counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            this.inner.poll(cx)
        }
    }

    impl PollCounter<()> {
        fn new<F: Future>(future: F) -> (PollCounter<F>, Arc<AtomicUsize>) {
            let counter = Arc::new(AtomicUsize::new(0));
            (
                PollCounter {
                    inner: future,
                    counter: counter.clone(),
                },
                counter,
            )
        }
    }

    #[cfg(not(target_os = "wasi"))]
    #[test]
    fn callbacks_fire_multi_thread() {
        let poll_start_counter = Arc::new(AtomicUsize::new(0));
        let poll_stop_counter = Arc::new(AtomicUsize::new(0));
        let poll_start = poll_start_counter.clone();
        let poll_stop = poll_stop_counter.clone();

        let before_task_poll_callback_task_id: Arc<Mutex<Option<tokio::task::Id>>> =
            Arc::new(Mutex::new(None));
        let after_task_poll_callback_task_id: Arc<Mutex<Option<tokio::task::Id>>> =
            Arc::new(Mutex::new(None));

        let before_task_poll_callback_task_id_ref = Arc::clone(&before_task_poll_callback_task_id);
        let after_task_poll_callback_task_id_ref = Arc::clone(&after_task_poll_callback_task_id);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .on_before_task_poll(move |task_meta| {
                before_task_poll_callback_task_id_ref
                    .lock()
                    .unwrap()
                    .replace(task_meta.id());
                poll_start_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .on_after_task_poll(move |task_meta| {
                after_task_poll_callback_task_id_ref
                    .lock()
                    .unwrap()
                    .replace(task_meta.id());
                poll_stop_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .build()
            .unwrap();
        let (task, count) = PollCounter::new(async {
            yield_now().await;
            yield_now().await;
            yield_now().await;
            5
        });
        let task = rt.spawn(task);

        let spawned_task_id = task.id();

        assert_eq!(rt.block_on(task).expect("task should succeed"), 5);
        // We need to drop the runtime to force the workers to cleanly exit
        drop(rt);

        assert_eq!(
            before_task_poll_callback_task_id.lock().unwrap().unwrap(),
            spawned_task_id
        );
        assert_eq!(
            after_task_poll_callback_task_id.lock().unwrap().unwrap(),
            spawned_task_id
        );
        let actual_count = count.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            poll_start.load(std::sync::atomic::Ordering::SeqCst),
            actual_count,
            "unexpected number of poll starts"
        );
        assert_eq!(
            poll_stop.load(std::sync::atomic::Ordering::SeqCst),
            actual_count,
            "unexpected number of poll stops"
        );
    }

    #[test]
    fn callbacks_fire_current_thread() {
        let poll_start_counter = Arc::new(AtomicUsize::new(0));
        let poll_stop_counter = Arc::new(AtomicUsize::new(0));
        let poll_start = poll_start_counter.clone();
        let poll_stop = poll_stop_counter.clone();

        let before_task_poll_callback_task_id: Arc<Mutex<Option<tokio::task::Id>>> =
            Arc::new(Mutex::new(None));
        let after_task_poll_callback_task_id: Arc<Mutex<Option<tokio::task::Id>>> =
            Arc::new(Mutex::new(None));

        let before_task_poll_callback_task_id_ref = Arc::clone(&before_task_poll_callback_task_id);
        let after_task_poll_callback_task_id_ref = Arc::clone(&after_task_poll_callback_task_id);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .on_before_task_poll(move |task_meta| {
                before_task_poll_callback_task_id_ref
                    .lock()
                    .unwrap()
                    .replace(task_meta.id());
                poll_start_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            })
            .on_after_task_poll(move |task_meta| {
                after_task_poll_callback_task_id_ref
                    .lock()
                    .unwrap()
                    .replace(task_meta.id());
                poll_stop_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            })
            .build()
            .unwrap();

        let task = rt.spawn(async {
            yield_now().await;
            yield_now().await;
            yield_now().await;
        });

        let spawned_task_id = task.id();

        let _ = rt.block_on(task);

        assert_eq!(
            before_task_poll_callback_task_id.lock().unwrap().unwrap(),
            spawned_task_id
        );
        assert_eq!(
            after_task_poll_callback_task_id.lock().unwrap().unwrap(),
            spawned_task_id
        );
        assert_eq!(poll_start.load(std::sync::atomic::Ordering::Relaxed), 4);
        assert_eq!(poll_stop.load(std::sync::atomic::Ordering::Relaxed), 4);
    }
}
