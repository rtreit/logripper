cmake_minimum_required(VERSION 3.20)
project(qsoripper-dsp C)

set(CMAKE_C_STANDARD 17)

add_library(qsoripper-dsp STATIC
    src/dsp.c
)

target_include_directories(qsoripper-dsp PUBLIC include)
