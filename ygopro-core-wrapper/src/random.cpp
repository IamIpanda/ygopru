#include "mtrandom.h"
#include <cstdint>
#include <utility>

extern "C" void* mtrandom_create(uint32_t seeds[], size_t len) {
    return new mtrandom(seeds, len);
}

extern "C" void* mtrandom_create_value(uint32_t value) {
    return new mtrandom((uint_fast32_t)value);
}

extern "C" void mtrandom_destroy(void* handle) {
    delete static_cast<mtrandom*>(handle);
}

extern "C" uint32_t mtrandom_rand(void* handle) {
    return (uint32_t)static_cast<mtrandom*>(handle)->rand();
}

extern "C" void mtrandom_discard(void* handle, uint64_t z) {
    static_cast<mtrandom*>(handle)->discard(z);
}

extern "C" int32_t mtrandom_get_random_integer(void* handle, int32_t l, int32_t h) {
    return static_cast<mtrandom*>(handle)->get_random_integer_v2(l, h);
}

extern "C" void mtrandom_shuffle_vector(void* handle, uint32_t* data, size_t count) {
    auto rnd = static_cast<mtrandom*>(handle);
    for (int i = 0; i < (int)count - 1; ++i) {
        int r = rnd->get_random_integer_v2(i, (int)count - 1);
        std::swap(data[i], data[r]);
    }
}
