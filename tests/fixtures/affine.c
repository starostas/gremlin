#include <stdint.h>
uint64_t affine_u64(uint64_t x) { return ((x ^ UINT64_C(0x12345678)) * UINT64_C(7)) + UINT64_C(3); }
