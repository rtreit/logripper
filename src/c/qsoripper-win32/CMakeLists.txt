cmake_minimum_required(VERSION 3.20)
project(qsoripper-win32 C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

add_executable(qsoripper-win32 WIN32 src/main.c)
target_link_libraries(qsoripper-win32 PRIVATE user32 gdi32 shell32 comctl32)

# Enable Unicode
target_compile_definitions(qsoripper-win32 PRIVATE UNICODE _UNICODE)

if(MSVC)
    target_compile_options(qsoripper-win32 PRIVATE
        /W4
        /analyze            # MSVC static analysis; /WX already promotes analysis warnings to errors
    )
else()
    target_compile_options(qsoripper-win32 PRIVATE -Wall -Wextra)
endif()

# cppcheck static analysis
find_program(CPPCHECK_EXE NAMES cppcheck)
if(CPPCHECK_EXE)
    message(STATUS "cppcheck found: ${CPPCHECK_EXE}")
    set(CPPCHECK_ARGS
        ${CPPCHECK_EXE}
        --enable=warning,performance,portability
        --error-exitcode=1
        --std=c11
        --suppress=missingIncludeSystem
        --suppress=missingInclude
        --inline-suppr
    )
    set_target_properties(qsoripper-win32 PROPERTIES C_CPPCHECK "${CPPCHECK_ARGS}")
else()
    message(STATUS "cppcheck not found — install it to enable static analysis")
endif()
