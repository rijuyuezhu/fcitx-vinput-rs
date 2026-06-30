cmake_minimum_required(VERSION 3.16)

project(vinput_fcitx_bridge VERSION 0.1.0 LANGUAGES CXX)

include(CTest)
include(GNUInstallDirs)
find_package(PkgConfig REQUIRED)
pkg_check_modules(SYSTEMD REQUIRED libsystemd)

option(VINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS
       "Probe Fcitx5 dependencies for the future addon module target" ON)
option(VINPUT_FCITX_BRIDGE_ENABLE_TESTS
       "Build pure C++ bridge smoke tests" ON)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

function(vinput_bridge_apply_warnings target)
  if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(${target} PRIVATE -Wall -Wextra -Werror)
  endif()
endfunction()

add_library(vinput_fcitx_bridge_core STATIC
  src/frontend_bridge.cpp
  src/recognition_payload.cpp
)
add_library(vinput_fcitx_bridge::core ALIAS vinput_fcitx_bridge_core)
target_include_directories(vinput_fcitx_bridge_core
  PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
)
target_compile_features(vinput_fcitx_bridge_core PUBLIC cxx_std_20)
vinput_bridge_apply_warnings(vinput_fcitx_bridge_core)

add_library(vinput_fcitx_bridge_sdbus STATIC src/sd_bus_daemon_client.cpp)
add_library(vinput_fcitx_bridge::sdbus ALIAS vinput_fcitx_bridge_sdbus)
target_link_libraries(vinput_fcitx_bridge_sdbus
  PUBLIC
    vinput_fcitx_bridge::core
  PRIVATE
    ${SYSTEMD_LIBRARIES}
)
target_include_directories(vinput_fcitx_bridge_sdbus PRIVATE ${SYSTEMD_INCLUDE_DIRS})
target_compile_options(vinput_fcitx_bridge_sdbus PRIVATE ${SYSTEMD_CFLAGS_OTHER})
target_compile_features(vinput_fcitx_bridge_sdbus PUBLIC cxx_std_20)
vinput_bridge_apply_warnings(vinput_fcitx_bridge_sdbus)

configure_file(vinput-addon.conf.in vinput-addon.conf @ONLY)

if(VINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS)
  find_package(Fcitx5Core QUIET)
  find_package(Fcitx5ModuleDBus QUIET)
  find_package(Fcitx5ModuleClipboard QUIET)
  find_package(Fcitx5ModuleNotifications QUIET)

  if(Fcitx5Core_FOUND AND Fcitx5ModuleDBus_FOUND AND
     Fcitx5ModuleClipboard_FOUND AND Fcitx5ModuleNotifications_FOUND)
    message(STATUS "Fcitx5 addon dependencies found; the module target will be enabled when adapter sources land")
  else()
    message(STATUS "Fcitx5 addon dependencies incomplete; building bridge core and sd-bus client only")
  endif()
endif()

if(BUILD_TESTING AND VINPUT_FCITX_BRIDGE_ENABLE_TESTS)
  function(vinput_bridge_smoke_test target source)
    add_executable(${target} ${source})
    target_link_libraries(${target} PRIVATE vinput_fcitx_bridge::core)
    vinput_bridge_apply_warnings(${target})
    add_test(NAME ${target} COMMAND ${target})
  endfunction()

  vinput_bridge_smoke_test(
    vinput_fcitx_bridge_recognition_payload_smoke
    tests/recognition_payload_smoke.cpp
  )
  vinput_bridge_smoke_test(
    vinput_fcitx_bridge_frontend_smoke
    tests/frontend_bridge_smoke.cpp
  )
endif()
