# 0.1.2 (February 20, 2019)

### Fixes
- `mpsc` and `Semaphore` when releasing permits (#904).
- `oneshot` task handle leak (#911).

### Changes
- Performance improvements in `AtomicTask` (#892).
- Improved assert message when creating a channel with bound of zero (#906).

### Adds
- `AtomicTask::take_task` (#895).

# 0.1.1 (February 1, 2019)

### Fixes
- Panic when creating a channel with bound 0 (#879).

# 0.1.0 (January 24, 2019)

- Initial Release
