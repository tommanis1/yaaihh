# Sourced by the Windows build of yaaihh.exe with speech's engine (N-554): `. ops/windows/speech/env.sh` with
# YAAIHH_MINGW set to the bin folder of mingw-w64's posix C and C++ drivers (yaaihh's unpacked folder,
# /home/t/.local/opt/mingw-w64-posix/root/usr/bin). It exports what that build needs and sets two names for the caller:
#   SPEECH_ENV      the variables to pass through the runner, as `--env NAME` each
#   SPEECH_FEATURES cargo's argument that turns the engine on
# Without YAAIHH_MINGW it sets nothing and both names are empty: the build stays as it was, with no engine.
SPEECH_ENV= SPEECH_FEATURES=
if [ -n "${YAAIHH_MINGW:-}" ]; then
  _speech_here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
  # 1. the compilers and the linker: the posix drivers (std::thread, which transcribe.cpp uses)
  export CC_x86_64_pc_windows_gnu=$YAAIHH_MINGW/x86_64-w64-mingw32-gcc-posix
  export CXX_x86_64_pc_windows_gnu=$YAAIHH_MINGW/x86_64-w64-mingw32-g++-posix
  export AR_x86_64_pc_windows_gnu=$YAAIHH_MINGW/x86_64-w64-mingw32-ar
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=$YAAIHH_MINGW/x86_64-w64-mingw32-g++-posix
  # 2. static C++ runtime: rustc links with -nodefaultlibs and turns -Bdynamic on before the link args, so libstdc++
  #    and the mingw runtime it needs are named here, static; the exe imports Windows' own libraries only
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=-static -C link-arg=-Wl,-Bstatic -C link-arg=-Wl,--start-group -C link-arg=-lstdc++ -C link-arg=-lpthread -C link-arg=-lmingwex -C link-arg=-lmsvcrt -C link-arg=-lgcc -C link-arg=-lgcc_eh -C link-arg=-lkernel32 -C link-arg=-Wl,--end-group"
  # 3. mingw-w64 11's headers lack THREAD_POWER_THROTTLING_STATE (ggml-cpu.c); C files only
  export CFLAGS_x86_64_pc_windows_gnu="-include $_speech_here/mingw11-throttling.h"
  # ggml: no tuning to the build machine; AVX2, FMA and F16C are the floor (speech::FLOOR says so at a press); its
  # static libraries get back their `lib` prefix
  export TRANSCRIBE_CMAKE_ARGS="-DCMAKE_SYSTEM_NAME=Windows -DGGML_NATIVE=OFF -DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON -DCMAKE_PROJECT_ggml_INCLUDE=$_speech_here/ggml-lib-prefix.cmake"
  SPEECH_ENV="--env CC_x86_64_pc_windows_gnu --env CXX_x86_64_pc_windows_gnu --env AR_x86_64_pc_windows_gnu --env CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER --env CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS --env CFLAGS_x86_64_pc_windows_gnu --env TRANSCRIBE_CMAKE_ARGS"
  SPEECH_FEATURES="--features yaaihh/speech-engine"
  unset _speech_here
fi
