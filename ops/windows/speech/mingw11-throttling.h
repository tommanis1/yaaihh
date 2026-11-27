/* N-554: mingw-w64 11 (Ubuntu 24.04's, also what `apt install g++-mingw-w64-x86-64-posix` gives) has ThreadPowerThrottling
   but not its struct, which ggml-cpu.c uses (ggml_thread_apply_priority); mingw-w64 12 defines it as below, after
   the Windows SDK's processthreadsapi.h. Given through CFLAGS -include, which cc-rs also passes when gcc assembles a
   .S file (blake3's): there it gives nothing, or the assembler reads C (W3's build, yaaihh N-776.p1). */
#if defined(_WIN32) && !defined(__ASSEMBLER__)
#include <windows.h>
#ifndef THREAD_POWER_THROTTLING_CURRENT_VERSION
#define THREAD_POWER_THROTTLING_CURRENT_VERSION 1
#define THREAD_POWER_THROTTLING_EXECUTION_SPEED 0x1
#define THREAD_POWER_THROTTLING_VALID_FLAGS (THREAD_POWER_THROTTLING_EXECUTION_SPEED)
typedef struct _THREAD_POWER_THROTTLING_STATE {
  ULONG Version;
  ULONG ControlMask;
  ULONG StateMask;
} THREAD_POWER_THROTTLING_STATE;
#endif
#endif
