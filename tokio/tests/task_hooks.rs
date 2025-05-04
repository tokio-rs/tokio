#![cfg(all(
    feature = "full",
    tokio_unstable,
    target_has_atomic = "64",
    not(target_arch = "wasm32")
))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::runtime;
use tokio::runtime::{
    AfterTaskPollAction, AfterTaskPollContext, BeforeTaskPollAction, BeforeTaskPollContext,
    OnChildSpawnAction, OnChildTaskSpawnContext, OnTaskTerminateAction, OnTaskTerminateContext,
    OnTopLevelSpawnAction, OnTopLevelTaskSpawnContext, TaskHookHarness, TaskHookHarnessFactory,
};

#[test]
fn runtime_default_factory() {
    let ct = runtime::Builder::new_current_thread();
    let mt = runtime::Builder::new_multi_thread();

    run_runtime_default_factory(ct);
    run_runtime_default_factory(mt);
}

#[test]
fn parent_child_chaining() {
    let ct = runtime::Builder::new_current_thread();
    let mt = runtime::Builder::new_multi_thread();

    run_parent_child_chaining(ct);
    run_parent_child_chaining(mt);
}

#[test]
fn before_poll() {
    let ct = runtime::Builder::new_current_thread();
    let mt = runtime::Builder::new_multi_thread();

    run_before_poll(ct);
    run_before_poll(mt);
}

#[test]
fn after_poll() {
    let ct = runtime::Builder::new_current_thread();
    let mt = runtime::Builder::new_multi_thread();

    run_after_poll(ct);
    run_after_poll(mt);
}

#[test]
fn terminate() {
    let ct = runtime::Builder::new_current_thread();

    run_terminate(ct);
}

#[test]
fn hook_switching() {
    let ct = runtime::Builder::new_current_thread();
    let mt = runtime::Builder::new_multi_thread();

    run_hook_switching(ct);
    run_hook_switching(mt);
}

#[test]
fn override_hooks() {
    let ct = runtime::Builder::new_current_thread();
    let mt = runtime::Builder::new_multi_thread();

    run_override(ct);
    run_override(mt);
}

fn run_runtime_default_factory(mut builder: runtime::Builder) {
    struct TestFactory {
        counter: Arc<AtomicUsize>,
    }

    impl TaskHookHarnessFactory for TestFactory {
        fn on_top_level_spawn(
            &self,
            _ctx: &mut OnTopLevelTaskSpawnContext<'_>,
        ) -> OnTopLevelSpawnAction {
            self.counter.fetch_add(1, Ordering::SeqCst);

            Default::default()
        }
    }

    let counter = Arc::new(AtomicUsize::new(0));

    let rt = builder
        .hook_harness_factory(TestFactory {
            counter: counter.clone(),
        })
        .build()
        .unwrap();

    rt.spawn(async {});

    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let handle = rt.handle();

    handle.spawn(async {});

    assert_eq!(counter.load(Ordering::SeqCst), 2);

    rt.block_on(async {});

    assert_eq!(counter.load(Ordering::SeqCst), 2);

    rt.block_on(async { tokio::spawn(async {}) });

    assert_eq!(counter.load(Ordering::SeqCst), 3);

    // block on a future which spawns a future and waits for it, which in turn spawns another future
    //
    // this checks that stuff works from on-worker within a multithreaded runtime
    let _ = rt.block_on(async { tokio::spawn(async { tokio::spawn(async {}) }).await });

    assert_eq!(counter.load(Ordering::SeqCst), 5);
}

fn run_parent_child_chaining(mut builder: runtime::Builder) {
    struct TestFactory {
        parent_spawns: Arc<AtomicUsize>,
        child_spawns: Arc<AtomicUsize>,
    }

    struct TestHooks {
        spawns: Arc<AtomicUsize>,
    }

    impl TaskHookHarnessFactory for TestFactory {
        fn on_top_level_spawn(
            &self,
            _ctx: &mut OnTopLevelTaskSpawnContext<'_>,
        ) -> OnTopLevelSpawnAction {
            self.parent_spawns.fetch_add(1, Ordering::SeqCst);

            let mut a = OnTopLevelSpawnAction::default();

            a.set_hooks(TestHooks {
                spawns: self.child_spawns.clone(),
            });

            a
        }
    }

    impl TaskHookHarness for TestHooks {
        fn on_child_spawn(&mut self, _ctx: &mut OnChildTaskSpawnContext<'_>) -> OnChildSpawnAction {
            self.spawns.fetch_add(1, Ordering::SeqCst);

            let mut a = OnChildSpawnAction::default();

            a.set_hooks(Self {
                spawns: self.spawns.clone(),
            });

            a
        }
    }

    let parent_spawns = Arc::new(AtomicUsize::new(0));
    let child_spawns = Arc::new(AtomicUsize::new(0));

    let rt = builder
        .hook_harness_factory(TestFactory {
            parent_spawns: parent_spawns.clone(),
            child_spawns: child_spawns.clone(),
        })
        .build()
        .unwrap();

    rt.spawn(async {});

    assert_eq!(parent_spawns.load(Ordering::SeqCst), 1);
    assert_eq!(child_spawns.load(Ordering::SeqCst), 0);

    let _ = rt.block_on(async { tokio::spawn(async { tokio::spawn(async {}) }).await });

    assert_eq!(parent_spawns.load(Ordering::SeqCst), 2);
    assert_eq!(child_spawns.load(Ordering::SeqCst), 1);
}

fn run_before_poll(mut builder: runtime::Builder) {
    struct TestFactory {
        polls: Arc<AtomicUsize>,
    }

    struct TestHooks {
        polls: Arc<AtomicUsize>,
    }

    impl TaskHookHarnessFactory for TestFactory {
        fn on_top_level_spawn(
            &self,
            _ctx: &mut OnTopLevelTaskSpawnContext<'_>,
        ) -> OnTopLevelSpawnAction {
            let mut a = OnTopLevelSpawnAction::default();

            a.set_hooks(TestHooks {
                polls: self.polls.clone(),
            });

            a
        }
    }

    impl TaskHookHarness for TestHooks {
        fn before_poll(&mut self, _ctx: &mut BeforeTaskPollContext<'_>) -> BeforeTaskPollAction {
            self.polls.fetch_add(1, Ordering::SeqCst);

            Default::default()
        }
    }

    let polls = Arc::new(AtomicUsize::new(0));

    let rt = builder
        .hook_harness_factory(TestFactory {
            polls: polls.clone(),
        })
        .build()
        .unwrap();

    rt.block_on(async {});
    assert_eq!(polls.load(Ordering::SeqCst), 0);

    let _ = rt.block_on(async { tokio::spawn(async {}).await });
    assert_eq!(polls.load(Ordering::SeqCst), 1);

    let _ = rt.block_on(async { tokio::spawn(async { tokio::spawn(async {}).await }).await });
    assert_eq!(polls.load(Ordering::SeqCst), 4);
}

fn run_after_poll(mut builder: runtime::Builder) {
    struct TestFactory {
        polls: Arc<AtomicUsize>,
    }

    struct TestHooks {
        polls: Arc<AtomicUsize>,
    }

    impl TaskHookHarnessFactory for TestFactory {
        fn on_top_level_spawn(
            &self,
            _ctx: &mut OnTopLevelTaskSpawnContext<'_>,
        ) -> OnTopLevelSpawnAction {
            let mut a = OnTopLevelSpawnAction::default();

            a.set_hooks(TestHooks {
                polls: self.polls.clone(),
            });

            a
        }
    }

    impl TaskHookHarness for TestHooks {
        fn after_poll(&mut self, _ctx: &mut AfterTaskPollContext<'_>) -> AfterTaskPollAction {
            self.polls.fetch_add(1, Ordering::SeqCst);

            Default::default()
        }
    }

    let polls = Arc::new(AtomicUsize::new(0));

    let rt = builder
        .hook_harness_factory(TestFactory {
            polls: polls.clone(),
        })
        .build()
        .unwrap();

    rt.block_on(async {});
    assert_eq!(polls.load(Ordering::SeqCst), 0);

    let _ = rt.block_on(async { tokio::spawn(async {}).await });
    assert_eq!(polls.load(Ordering::SeqCst), 1);

    let _ = rt.block_on(async { tokio::spawn(async { tokio::spawn(async {}).await }).await });
    assert_eq!(polls.load(Ordering::SeqCst), 4);
}

fn run_terminate(mut builder: runtime::Builder) {
    struct TestFactory {
        terminations: Arc<AtomicUsize>,
    }

    struct TestHooks {
        terminations: Arc<AtomicUsize>,
    }

    impl TaskHookHarnessFactory for TestFactory {
        fn on_top_level_spawn(
            &self,
            _ctx: &mut OnTopLevelTaskSpawnContext<'_>,
        ) -> OnTopLevelSpawnAction {
            let mut a = OnTopLevelSpawnAction::default();

            a.set_hooks(TestHooks {
                terminations: self.terminations.clone(),
            });

            a
        }
    }

    impl TaskHookHarness for TestHooks {
        fn on_task_terminate(
            &mut self,
            _ctx: &mut OnTaskTerminateContext<'_>,
        ) -> OnTaskTerminateAction {
            self.terminations.fetch_add(1, Ordering::SeqCst);

            Default::default()
        }
    }

    let terminations = Arc::new(AtomicUsize::new(0));

    let rt = builder
        .hook_harness_factory(TestFactory {
            terminations: terminations.clone(),
        })
        .build()
        .unwrap();

    let _ = rt.block_on(async { tokio::spawn(async { tokio::spawn(async {}).await }).await });

    assert_eq!(terminations.load(Ordering::SeqCst), 2);
}

fn run_hook_switching(mut builder: runtime::Builder) {
    struct TestFactory {
        next_id: Arc<AtomicUsize>,
        flag: Arc<AtomicUsize>,
    }

    struct TestHooks {
        id: usize,
        flag: Arc<AtomicUsize>,
    }

    impl TaskHookHarnessFactory for TestFactory {
        fn on_top_level_spawn(
            &self,
            _ctx: &mut OnTopLevelTaskSpawnContext<'_>,
        ) -> OnTopLevelSpawnAction {
            let mut a = OnTopLevelSpawnAction::default();

            a.set_hooks(TestHooks {
                id: self.next_id.fetch_add(1, Ordering::SeqCst),
                flag: self.flag.clone(),
            });

            a
        }
    }

    impl TaskHookHarness for TestHooks {
        fn before_poll(&mut self, _ctx: &mut BeforeTaskPollContext<'_>) -> BeforeTaskPollAction {
            self.flag.store(self.id, Ordering::SeqCst);

            Default::default()
        }
    }

    let polls = Arc::new(AtomicUsize::new(0));

    let rt = builder
        .hook_harness_factory(TestFactory {
            next_id: Arc::new(Default::default()),
            flag: polls.clone(),
        })
        .build()
        .unwrap();

    let _ = rt.block_on(async { tokio::spawn(async {}).await });
    assert_eq!(polls.load(Ordering::SeqCst), 0);

    let _ = rt.block_on(async { tokio::spawn(async { tokio::spawn(async {}).await }).await });
    assert_eq!(polls.load(Ordering::SeqCst), 1);

    let _ = rt.block_on(async { tokio::spawn(async {}).await });
    assert_eq!(polls.load(Ordering::SeqCst), 3);
}

fn run_override(mut builder: runtime::Builder) {
    struct TestFactory {
        counter: Arc<AtomicUsize>,
    }

    struct TestHooks {
        counter: Arc<AtomicUsize>,
    }

    impl TaskHookHarness for TestHooks {
        fn before_poll(&mut self, _ctx: &mut BeforeTaskPollContext<'_>) -> BeforeTaskPollAction {
            self.counter.fetch_add(1, Ordering::SeqCst);

            Default::default()
        }

        fn on_child_spawn(&mut self, _ctx: &mut OnChildTaskSpawnContext<'_>) -> OnChildSpawnAction {
            let mut a = OnChildSpawnAction::default();

            a.set_hooks(Self {
                counter: self.counter.clone(),
            });

            a
        }
    }

    impl TaskHookHarnessFactory for TestFactory {
        fn on_top_level_spawn(
            &self,
            _ctx: &mut OnTopLevelTaskSpawnContext<'_>,
        ) -> OnTopLevelSpawnAction {
            self.counter.fetch_add(1, Ordering::SeqCst);

            Default::default()
        }
    }

    let factory_counter = Arc::new(AtomicUsize::new(0));
    let builder_counter = Arc::new(AtomicUsize::new(0));

    let rt = builder
        .hook_harness_factory(TestFactory {
            counter: factory_counter.clone(),
        })
        .build()
        .unwrap();

    rt.spawn(async {});

    assert_eq!(factory_counter.load(Ordering::SeqCst), 1);

    let _ = rt.block_on(async {
        tokio::task::spawn_with_hooks(
            async {},
            TestHooks {
                counter: builder_counter.clone(),
            },
        )
        .await
    });

    assert_eq!(factory_counter.load(Ordering::SeqCst), 1);
    assert_eq!(builder_counter.load(Ordering::SeqCst), 1);

    let _ = rt.block_on(async {
        let counter = builder_counter.clone();
        tokio::spawn(async { tokio::task::spawn_with_hooks(async {}, TestHooks { counter }).await })
            .await
    });

    assert_eq!(factory_counter.load(Ordering::SeqCst), 2);
    assert_eq!(builder_counter.load(Ordering::SeqCst), 2);
}
