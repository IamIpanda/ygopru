#include "mtrandom.h"
#include <cstdint>
#include <vector>

extern "C" void shuffle_deck(uint32_t seeds[8], uint32_t* deck, size_t count) {
    mtrandom rnd(seeds, 8);

    std::vector<uint32_t> v(deck, deck + count);
    rnd.shuffle_vector(v);

    for (size_t i = 0; i < count; i++)
        deck[i] = v[i];
}
