#include <stdint.h>

/* Independent bitwise implementation of the POSIX cksum CRC recurrence.
   Polynomial: x^32 + 0x04c11db7. No file-length folding or final complement. */
uint32_t crc_feedback(uint32_t state) {
    return (state & UINT32_C(0x80000000)) ? UINT32_C(0x04c11db7) : 0;
}

/* The low eight bits of byte are consumed; the u32 interface avoids casts
   between widths in Gremlin's current language. */
uint32_t crc_byte(uint32_t state, uint32_t byte) {
    state ^= byte << 24;
    for (unsigned i = 0; i < 8; ++i) {
        uint32_t feedback = (state & UINT32_C(0x80000000))
            ? UINT32_C(0x04c11db7) : 0;
        state = (state << 1) ^ feedback;
    }
    return state;
}
