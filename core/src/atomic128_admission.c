#include <stdint.h>
#if defined(_WIN32) && defined(__x86_64__)
_Static_assert(sizeof(unsigned int) == 4, "Win64 CPUID output uses 32-bit unsigned int");
// The explicit RBX output makes its clobber visible to Clang's Win64 frame
// lowering. cpuid.h and this compiler's __cpuidex intrinsic instead hide a
// transient RBX change behind xchg with a volatile register, which cannot be
// described by the leaf unwind fallback. Qualify the emitted save/unwind pair.
// This baseline TU never receives -mcx16 and touches no caller memory.
int hpr3_admit(void) {
    unsigned int a, b, c, d;
    __asm__ __volatile__("cpuid"
        : "=a"(a), "=b"(b), "=c"(c), "=d"(d)
        : "0"(0U), "2"(0U));
    if (a < 1U) return 0;
    __asm__ __volatile__("cpuid"
        : "=a"(a), "=b"(b), "=c"(c), "=d"(d)
        : "0"(1U), "2"(0U));
    return (c & (1U << 13)) != 0;
}
#elif defined(__linux__) && defined(__x86_64__)
#include <cpuid.h>
int hpr3_admit(void) {
    unsigned int a, b, c, d;
    return __get_cpuid(1, &a, &b, &c, &d) && (c & bit_CMPXCHG16B) != 0;
}
#elif defined(__linux__) && defined(__aarch64__)
#include <sys/auxv.h>
#include <asm/hwcap.h>
int hpr3_admit(void) { return (getauxval(AT_HWCAP) & HWCAP_ATOMICS) != 0; }
#elif defined(__APPLE__) && defined(__aarch64__)
int hpr3_admit(void) { return 1; }
#else
#error "No CPU admission contract for this target"
#endif
