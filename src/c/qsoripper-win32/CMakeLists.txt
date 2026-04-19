cmake_minimum_required(VERSION 3.20)
project(qsoripper-win32 C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

add_executable(qsoripper-win32 WIN32
    src/main.c
    src/backend_ffi_gate.c
)

# FFI library from the Rust qsoripper-ffi crate
# The .dll.lib import library and header are produced by `cargo build -p qsoripper-ffi`
set(FFI_ROOT "${CMAKE_SOURCE_DIR}/../../rust/qsoripper-ffi")
set(FFI_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/include")

# Allow override via -DFFI_LIB_DIR for CI or custom layouts
if(NOT DEFINED FFI_LIB_DIR)
    set(FFI_LIB_DIR "${CMAKE_SOURCE_DIR}/../../rust/target/release")
endif()

target_include_directories(qsoripper-win32 PRIVATE ${FFI_INCLUDE})
target_link_libraries(qsoripper-win32 PRIVATE
    user32 gdi32 shell32 comctl32
)

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

enable_testing()

add_executable(qsoripper-win32-backend-lifetime-tests
    tests/backend_lifetime_tests.c
    src/backend_ffi_gate.c
)
target_include_directories(qsoripper-win32-backend-lifetime-tests PRIVATE
    ${FFI_INCLUDE}
)
target_link_libraries(qsoripper-win32-backend-lifetime-tests PRIVATE
    user32
)
if(MSVC)
    target_compile_options(qsoripper-win32-backend-lifetime-tests PRIVATE /W4 /WX)
else()
    target_compile_options(qsoripper-win32-backend-lifetime-tests PRIVATE -Wall -Wextra)
endif()
add_test(
    NAME qsoripper-win32-backend-lifetime-tests
    COMMAND qsoripper-win32-backend-lifetime-tests
)

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

include(CTest)
if(BUILD_TESTING)
    add_executable(qsoripper-win32-cli-exitcode-regression
        tests/cli_exit_code_regression.c
        src/main.c
    )

    target_include_directories(qsoripper-win32-cli-exitcode-regression PRIVATE ${FFI_INCLUDE})
    target_link_libraries(qsoripper-win32-cli-exitcode-regression PRIVATE
        user32 gdi32 shell32 comctl32
    )
    target_compile_definitions(qsoripper-win32-cli-exitcode-regression PRIVATE
        UNICODE _UNICODE QSORIPPER_WIN32_TESTING
    )

    if(MSVC)
        target_compile_options(qsoripper-win32-cli-exitcode-regression PRIVATE
            /W4
            /analyze
        )
    else()
        target_compile_options(qsoripper-win32-cli-exitcode-regression PRIVATE -Wall -Wextra)
    endif()

    add_test(
        NAME win32-cli-exitcode-regression
        COMMAND qsoripper-win32-cli-exitcode-regression
    )
endif()
