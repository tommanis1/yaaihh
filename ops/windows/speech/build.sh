#!/bin/bash
# N-554 step 1: this try crate built for Windows through the runner. TOOLS: a folder with mingw-w64's C and C++ (posix)
# drivers (the unpacked g++-mingw-w64-x86-64-posix for the try; /usr/bin once it is installed). $1: log file; the rest:
# cargo's arguments after the target (e.g. --features engine).
here=$(cd "$(dirname "$0")" && pwd)
tools=${TOOLS:-/usr/bin}
cd "$here/.." || exit 1
export CC_x86_64_pc_windows_gnu=$tools/x86_64-w64-mingw32-gcc-posix CXX_x86_64_pc_windows_gnu=$tools/x86_64-w64-mingw32-g++-posix
# rustc links with -nodefaultlibs, so g++ adds no C++ library: libstdc++ and the mingw runtime it needs, all static
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=$tools/x86_64-w64-mingw32-g++-posix
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=-static -C link-arg=-Wl,-Bstatic -C link-arg=-Wl,--start-group -C link-arg=-lstdc++ -C link-arg=-lpthread -C link-arg=-lmingwex -C link-arg=-lmsvcrt -C link-arg=-lgcc -C link-arg=-lgcc_eh -C link-arg=-lkernel32 -C link-arg=-Wl,--end-group"
# mingw-w64 11's headers lack THREAD_POWER_THROTTLING_STATE (ggml-cpu.c)
export CFLAGS_x86_64_pc_windows_gnu="-include $here/mingw11-throttling.h"
# no tuning to this machine's processor: AVX2, FMA and F16C are the floor; ggml's libraries get back their `lib`
export TRANSCRIBE_CMAKE_ARGS="-DCMAKE_SYSTEM_NAME=Windows -DGGML_NATIVE=OFF -DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON -DCMAKE_PROJECT_ggml_INCLUDE=$here/ggml-lib-prefix.cmake"
log=$1; shift
S=$(date +%s)
build-runner cargo --folder "${FOLDER:-$(whoami)}" --env CC_x86_64_pc_windows_gnu --env CXX_x86_64_pc_windows_gnu --env CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER --env CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS --env TRANSCRIBE_CMAKE_ARGS --env CFLAGS_x86_64_pc_windows_gnu build --release --target x86_64-pc-windows-gnu "$@" > "$log" 2>&1
echo "exit $? after $(( $(date +%s)-S )) s"
grep -E "build-runner|Finished|^error" "$log" | head
