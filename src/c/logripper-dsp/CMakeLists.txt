cmake_minimum_required(VERSION 3.20)
project(logripper-dsp C)

set(CMAKE_C_STANDARD 17)

add_library(logripper-dsp STATIC
    src/dsp.c
)

target_include_directories(logripper-dsp PUBLIC include)
