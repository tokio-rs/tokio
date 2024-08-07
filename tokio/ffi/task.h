struct Context {
    void (*wake) (void*);

};

struct Task {
    void* data;
    void* (*poll) (void*);
};

