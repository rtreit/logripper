cmake_minimum_required(VERSION 3.20)
project(qsoripper-win32 C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

add_executable(qsoripper-win32 WIN32 src/main.c)
target_link_libraries(qsoripper-win32 PRIVATE user32 gdi32 shell32 comctl32)

# Enable Unicode
target_compile_definitions(qsoripper-win32 PRIVATE UNICODE _UNICODE)

if(MSVC)
    target_compile_options(qsoripper-win32 PRIVATE /W4)
else()
    target_compile_options(qsoripper-win32 PRIVATE -Wall -Wextra)
endif()
