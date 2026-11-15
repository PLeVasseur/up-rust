#ifndef __clang__
#error "PAL wide atomic backend requires Clang generic atomic builtins"
#endif
#include <stddef.h>
#include <stdint.h>

/* Deliberately NOT a C _Atomic object: the Rust-owned object is the identical
 * two-integer representation. All shared accesses use Clang's documented
 * generic __atomic builtins. There is no foreign initialization/lifetime or
 * hidden address-dependent lock state to preserve when Rust moves an unshared
 * value. The supported target/compiler is a separate dependency lemma. */
typedef struct __attribute__((aligned(16))) {
    uint64_t low;
    uint64_t high;
} hpr3_word;

_Static_assert(sizeof(hpr3_word) == 16, "exact Rust/C size required");
_Static_assert(_Alignof(hpr3_word) == 16, "exact Rust/C alignment required");
_Static_assert(offsetof(hpr3_word, low) == 0, "low offset");
_Static_assert(offsetof(hpr3_word, high) == 8, "high offset");
_Static_assert(sizeof(uint64_t) == 8, "64-bit limbs required");
_Static_assert(__atomic_always_lock_free(16, 0), "no lock-table fallback admitted");

void hpr3_load(const void *storage, uint64_t *low, uint64_t *high) {
    hpr3_word value;
    __atomic_load((const hpr3_word *)storage, &value, __ATOMIC_ACQUIRE);
    *low = value.low;
    *high = value.high;
}

int hpr3_cas(void *storage, uint64_t old_low, uint64_t old_high,
             uint64_t new_low, uint64_t new_high) {
    hpr3_word expected = {old_low, old_high};
    hpr3_word desired = {new_low, new_high};
    return __atomic_compare_exchange((hpr3_word *)storage, &expected, &desired,
                                     0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
}

/* Executable affected layout receipt, not a substitute for assertions. */
uint64_t hpr3_layout(void) {
    return ((uint64_t)sizeof(hpr3_word) << 32) | _Alignof(hpr3_word);
}
