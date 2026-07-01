cmake_minimum_required(VERSION 3.16)

project(vinput_fcitx_bridge VERSION 0.1.0 LANGUAGES CXX)

include(CTest)
include(GNUInstallDirs)
find_package(PkgConfig REQUIRED)
pkg_check_modules(SYSTEMD REQUIRED libsystemd)
find_package(Fcitx5Core QUIET)

option(VINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS
       "Probe Fcitx5 dependencies for the retained addon module target" ON)
option(VINPUT_FCITX_BRIDGE_ENABLE_TESTS
       "Build pure C++ bridge smoke tests" ON)
option(VINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE
       "Fail configure if Fcitx5Core is unavailable" OFF)
option(VINPUT_FCITX_BRIDGE_INSTALL_DBUS_SERVICE
       "Install the DBus activation service for the Rust daemon" ON)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)


function(vinput_bridge_install_dir output value fallback)
  if(NOT "${value}" STREQUAL "")
    if(IS_ABSOLUTE "${value}")
      file(RELATIVE_PATH relative_value "/" "${value}")
      set(${output} "${relative_value}" PARENT_SCOPE)
    else()
      set(${output} "${value}" PARENT_SCOPE)
    endif()
  else()
    set(${output} "${fallback}" PARENT_SCOPE)
  endif()
endfunction()

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
set_target_properties(vinput_fcitx_bridge_core PROPERTIES POSITION_INDEPENDENT_CODE ON)
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
set_target_properties(vinput_fcitx_bridge_sdbus PROPERTIES POSITION_INDEPENDENT_CODE ON)
vinput_bridge_apply_warnings(vinput_fcitx_bridge_sdbus)

set(VINPUT_DAEMON_EXECUTABLE
    "${CMAKE_INSTALL_FULL_BINDIR}/vinput-daemon"
    CACHE FILEPATH "Installed vinput-daemon executable used by DBus activation")
configure_file(vinput-addon.conf.in vinput-addon.conf @ONLY)
configure_file(
  "${CMAKE_CURRENT_SOURCE_DIR}/../../data/org.fcitx.Vinput.service.in"
  org.fcitx.Vinput.service
  @ONLY
)


if(Fcitx5Core_FOUND)
  add_library(vinput_fcitx_bridge_fcitx STATIC
    src/fcitx_candidates.cpp
    src/fcitx_key_trigger.cpp
    src/fcitx_outcome.cpp
    src/fcitx_selection.cpp
  )
  add_library(vinput_fcitx_bridge::fcitx ALIAS vinput_fcitx_bridge_fcitx)
  target_link_libraries(vinput_fcitx_bridge_fcitx
    PUBLIC
      vinput_fcitx_bridge::core
      Fcitx5::Core
  )
  target_compile_features(vinput_fcitx_bridge_fcitx PUBLIC cxx_std_20)
  if(Fcitx5Core_VERSION VERSION_GREATER_EQUAL "5.1.9")
    target_compile_definitions(
      vinput_fcitx_bridge_fcitx
      PUBLIC VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
    )
  endif()
  set_target_properties(vinput_fcitx_bridge_fcitx PROPERTIES POSITION_INDEPENDENT_CODE ON)
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_fcitx)

  add_library(fcitx5_vinput_addon MODULE
    src/fcitx_addon.cpp
    src/fcitx_addon_factory.cpp
  )
  target_link_libraries(fcitx5_vinput_addon
    PRIVATE
      vinput_fcitx_bridge::sdbus
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  target_compile_features(fcitx5_vinput_addon PRIVATE cxx_std_20)
  if(Fcitx5Core_VERSION VERSION_GREATER_EQUAL "5.1.12")
    target_compile_definitions(
      fcitx5_vinput_addon
      PRIVATE VINPUT_FCITX5_CORE_HAVE_ADDON_FACTORY_V2
    )
  endif()
  set_target_properties(fcitx5_vinput_addon PROPERTIES
    PREFIX ""
    OUTPUT_NAME "fcitx5-vinput"
  )
  vinput_bridge_apply_warnings(fcitx5_vinput_addon)


  add_executable(
    vinput_fcitx_bridge_candidates_smoke
    tests/fcitx_candidates_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_candidates_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_candidates_smoke)
  add_test(
    NAME vinput_fcitx_bridge_candidates_smoke
    COMMAND vinput_fcitx_bridge_candidates_smoke
  )

  add_executable(
    vinput_fcitx_bridge_key_trigger_smoke
    tests/fcitx_key_trigger_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_key_trigger_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_key_trigger_smoke)
  add_test(
    NAME vinput_fcitx_bridge_key_trigger_smoke
    COMMAND vinput_fcitx_bridge_key_trigger_smoke
  )

  add_executable(
    vinput_fcitx_bridge_selection_smoke
    tests/fcitx_selection_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_selection_smoke
    PRIVATE
      vinput_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_selection_smoke)
  add_test(
    NAME vinput_fcitx_bridge_selection_smoke
    COMMAND vinput_fcitx_bridge_selection_smoke
  )

  vinput_bridge_install_dir(
    VINPUT_FCITX_MODULE_INSTALL_DIR
    "${FCITX_INSTALL_LIBDIR}/fcitx5"
    "${CMAKE_INSTALL_LIBDIR}/fcitx5"
  )
  vinput_bridge_install_dir(
    VINPUT_FCITX_ADDON_INSTALL_DIR
    "${FCITX_INSTALL_PKGDATADIR}/addon"
    "${CMAKE_INSTALL_DATADIR}/fcitx5/addon"
  )
  install(TARGETS fcitx5_vinput_addon DESTINATION "${VINPUT_FCITX_MODULE_INSTALL_DIR}")
  install(
    FILES "${CMAKE_CURRENT_BINARY_DIR}/vinput-addon.conf"
    DESTINATION "${VINPUT_FCITX_ADDON_INSTALL_DIR}"
    RENAME vinput.conf
  )
  if(VINPUT_FCITX_BRIDGE_INSTALL_DBUS_SERVICE)
    install(
      FILES "${CMAKE_CURRENT_BINARY_DIR}/org.fcitx.Vinput.service"
      DESTINATION "${CMAKE_INSTALL_DATADIR}/dbus-1/services"
    )
  endif()
else()
  if(VINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE)
    message(FATAL_ERROR "Fcitx5Core is required for the addon module target")
  endif()
  message(STATUS "Fcitx5Core not found; skipping fcitx5-vinput module target")
endif()

if(VINPUT_FCITX_BRIDGE_ENABLE_FCITX_DEPS)
  find_package(Fcitx5ModuleDBus QUIET)
  find_package(Fcitx5ModuleClipboard QUIET)
  find_package(Fcitx5ModuleNotifications QUIET)

  if(Fcitx5Core_FOUND AND Fcitx5ModuleDBus_FOUND AND
     Fcitx5ModuleClipboard_FOUND AND Fcitx5ModuleNotifications_FOUND)
    message(STATUS "Fcitx5 addon dependencies found; retained addon sources can link module helpers")
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

  add_executable(
    vinput_fcitx_bridge_dbus_smoke
    tests/sd_bus_daemon_client_smoke.cpp
  )
  target_link_libraries(
    vinput_fcitx_bridge_dbus_smoke
    PRIVATE
      vinput_fcitx_bridge::sdbus
  )
  vinput_bridge_apply_warnings(vinput_fcitx_bridge_dbus_smoke)
endif()
