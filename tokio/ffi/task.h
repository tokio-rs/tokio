struct RawWakerVTable {
    void (*clone)(void *);

    void (*wake)(void *);

    void (*wake_by_ref)(void *);

    void (*drop)(void *);
} __attribute__((__packed__));

struct Waker {
    void *data;
};

struct Context {

};

struct Task {
    void *data;

};

