#include <stdint.h>
#if defined(__linux__) && defined(__x86_64__)
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
