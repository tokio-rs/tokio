#include <stdint.h>

struct RuntimeUnstableOptions {
    // unstable
    bool enable_metrics_poll_count_histogram;
    // unstable
    size_t metrics_poll_count_histogram_buckets;
    // unstable
    struct timespec metrics_poll_count_histogram_resolution;

    // unstable
    uint8_t unhandled_panic;
};

struct RuntimeHooks {
    void (*on_thread_park) (void);
    void (*on_thread_unpark) (void);
    void (*on_thread_start) (void);
    void (*on_thread_stop) (void);
};

struct RuntimeOptions {
    // unstable
    bool disable_lifo_slot;

    bool enable_time;
    bool enable_io;

    uint32_t event_interval;
    uint32_t global_queue_interval;

    size_t max_blocking_threads;

    size_t max_io_events_per_tick;

    uint8_t runtime_kind;

    // todo rng_seed

    bool start_paused;

    struct timespec thread_keep_alive;

    char* thread_name;
    char* (*thread_name_fn) (void);
    size_t thread_stack_size;

    size_t worker_threads;
};

struct Runtime;