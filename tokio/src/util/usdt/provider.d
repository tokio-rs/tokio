provider tokio {
    probe task__details(uint64_t, char*, char*, uint32_t, uint32_t);
    probe task__start(uint64_t, uint8_t, uintptr_t, uintptr_t);
    probe task__terminate(uint64_t, uint8_t);

    probe task__poll__start(uint64_t);
    probe task__poll__end(uint64_t);

    probe task__waker__clone(uint64_t);
    probe task__waker__drop(uint64_t);
    probe task__waker__wake(uint64_t);
};
