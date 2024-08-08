#include <stdint.h>

struct RuntimeHooks {
    void (*on_thread_park)(void);

    void (*on_thread_unpark)(void);

    void (*on_thread_start)(void);

    void (*on_thread_stop)(void);
}__attribute__((__packed__));

struct RuntimeOptions {
    bool enable_time;
    bool enable_io;

    uint32_t event_interval;
    uint32_t global_queue_interval;

    size_t max_blocking_threads;

    size_t max_io_events_per_tick;

    uint8_t runtime_kind;

    bool start_paused;

    struct timespec *thread_keep_alive;

    struct RuntimeHooks *runtime_hooks;

    char *thread_name;

    char *(*thread_name_fn)(void);

    size_t thread_stack_size;

    size_t worker_threads;
}__attribute__((__packed__));

struct Runtime;