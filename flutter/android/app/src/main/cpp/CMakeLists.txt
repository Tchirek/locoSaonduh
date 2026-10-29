cmake_minimum_required(VERSION 3.22.1)
project(locoopus C CXX)

set(OPUS_BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(OPUS_BUILD_PROGRAMS OFF CACHE BOOL "" FORCE)
set(OPUS_BUILD_SHARED_LIBRARY OFF CACHE BOOL "" FORCE)
set(OPUS_INSTALL_PKG_CONFIG_MODULE OFF CACHE BOOL "" FORCE)
set(OPUS_INSTALL_CMAKE_CONFIG_MODULE OFF CACHE BOOL "" FORCE)
set(OPUS_HARDENING OFF CACHE BOOL "" FORCE)

add_subdirectory(opus EXCLUDE_FROM_ALL)

add_library(locoopus SHARED opus_jni.cpp)
target_include_directories(locoopus PRIVATE opus/include)
target_link_libraries(locoopus PRIVATE opus log)
set_target_properties(locoopus PROPERTIES CXX_STANDARD 17)
