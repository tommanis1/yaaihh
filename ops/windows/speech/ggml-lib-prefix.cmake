# N-554: ggml/CMakeLists.txt sets CMAKE_STATIC_LIBRARY_PREFIX "" on WIN32, so mingw installs ggml.a, ggml-base.a and
# ggml-cpu.a, which rustc's `-l static=ggml` (transcribe-cpp-sys's manifest) does not find. Run as ggml's project
# include; at the end of ggml's directory, its static libraries get back the `lib` that mingw's linker looks for.
function(yaaihh_ggml_lib_prefix)
  foreach(t ggml ggml-base ggml-cpu)
    if(TARGET ${t})
      set_target_properties(${t} PROPERTIES PREFIX "lib")
    endif()
  endforeach()
endfunction()
cmake_language(DEFER CALL yaaihh_ggml_lib_prefix)
