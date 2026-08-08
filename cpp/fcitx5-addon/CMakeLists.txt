cmake_minimum_required(VERSION 3.16)

project(vinpst_fcitx_bridge VERSION 0.1.0 LANGUAGES CXX)

include(CTest)
include(GNUInstallDirs)
find_package(PkgConfig REQUIRED)
pkg_check_modules(SYSTEMD REQUIRED libsystemd)
find_package(Fcitx5Core QUIET)
find_package(Fcitx5ModuleClipboard QUIET)
find_package(Fcitx5ModuleDBus QUIET)
option(VINPST_FCITX_BRIDGE_ENABLE_FCITX_DEPS
       "Probe Fcitx5 dependencies for the retained addon module target" ON)
option(VINPST_FCITX_BRIDGE_ENABLE_TESTS
       "Build pure C++ bridge smoke tests" ON)
option(VINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE
       "Fail configure if Fcitx5Core is unavailable" OFF)
option(VINPST_FCITX_BRIDGE_INSTALL_DBUS_SERVICE
       "Install the DBus activation service for the Rust daemon" ON)
option(VINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE
       "Install the systemd user service for the Rust daemon" ON)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_program(CARGO_EXECUTABLE cargo REQUIRED)
get_filename_component(
  VINPST_REPOSITORY_ROOT
  "${CMAKE_CURRENT_SOURCE_DIR}/../.."
  ABSOLUTE
)
set(
  VINPST_FCITX_RUST_TARGET_DIR
  "${VINPST_REPOSITORY_ROOT}/target"
  CACHE PATH "Cargo target directory for the Rust frontend bridge"
)
set(
  VINPST_FCITX_RUST_BUILD_PROFILE
  auto
  CACHE STRING "Cargo profile for the Rust frontend bridge: auto, debug, or release"
)
set_property(
  CACHE VINPST_FCITX_RUST_BUILD_PROFILE
  PROPERTY STRINGS auto debug release
)
if(VINPST_FCITX_RUST_BUILD_PROFILE STREQUAL "auto")
  if(CMAKE_BUILD_TYPE MATCHES "^(Release|RelWithDebInfo|MinSizeRel)$")
    set(VINPST_FCITX_RUST_RESOLVED_PROFILE release)
  else()
    set(VINPST_FCITX_RUST_RESOLVED_PROFILE debug)
  endif()
elseif(VINPST_FCITX_RUST_BUILD_PROFILE MATCHES "^(debug|release)$")
  set(VINPST_FCITX_RUST_RESOLVED_PROFILE "${VINPST_FCITX_RUST_BUILD_PROFILE}")
else()
  message(FATAL_ERROR
    "VINPST_FCITX_RUST_BUILD_PROFILE must be auto, debug, or release"
  )
endif()
if(VINPST_FCITX_RUST_RESOLVED_PROFILE STREQUAL "release")
  set(VINPST_FCITX_RUST_CARGO_ARGS --release)
else()
  set(VINPST_FCITX_RUST_CARGO_ARGS)
endif()
set(
  VINPST_FCITX_RUST_LIBRARY
  "${VINPST_FCITX_RUST_TARGET_DIR}/${VINPST_FCITX_RUST_RESOLVED_PROFILE}/libvinpst_fcitx_ffi.a"
)
file(
  GLOB_RECURSE VINPST_FCITX_RUST_SOURCES CONFIGURE_DEPENDS
  "${VINPST_REPOSITORY_ROOT}/crates/vinpst-fcitx-core/src/*.rs"
  "${VINPST_REPOSITORY_ROOT}/crates/vinpst-fcitx-dbus/src/*.rs"
  "${VINPST_REPOSITORY_ROOT}/crates/vinpst-fcitx-ffi/src/*.rs"
  "${VINPST_REPOSITORY_ROOT}/crates/vinpst-protocol/src/*.rs"
)
add_custom_command(
  OUTPUT "${VINPST_FCITX_RUST_LIBRARY}"
  COMMAND
    "${CMAKE_COMMAND}" -E env
    "CARGO_TARGET_DIR=${VINPST_FCITX_RUST_TARGET_DIR}"
    "${CARGO_EXECUTABLE}" build --locked -p vinpst-fcitx-ffi
    ${VINPST_FCITX_RUST_CARGO_ARGS}
  WORKING_DIRECTORY "${VINPST_REPOSITORY_ROOT}"
  DEPENDS
    "${VINPST_REPOSITORY_ROOT}/Cargo.toml"
    "${VINPST_REPOSITORY_ROOT}/Cargo.lock"
    "${VINPST_REPOSITORY_ROOT}/crates/vinpst-fcitx-core/Cargo.toml"
    "${VINPST_REPOSITORY_ROOT}/crates/vinpst-fcitx-dbus/Cargo.toml"
    "${VINPST_REPOSITORY_ROOT}/crates/vinpst-fcitx-ffi/Cargo.toml"
    "${VINPST_REPOSITORY_ROOT}/crates/vinpst-protocol/Cargo.toml"
    ${VINPST_FCITX_RUST_SOURCES}
  COMMENT "Building Rust Fcitx frontend bridge"
  VERBATIM
)
add_custom_target(
  vinpst_fcitx_rust_ffi_build
  DEPENDS "${VINPST_FCITX_RUST_LIBRARY}"
)
add_library(vinpst_fcitx_rust_ffi STATIC IMPORTED GLOBAL)
set_target_properties(
  vinpst_fcitx_rust_ffi
  PROPERTIES
    IMPORTED_LOCATION "${VINPST_FCITX_RUST_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES
      "${VINPST_REPOSITORY_ROOT}/crates/vinpst-fcitx-ffi/include"
)
add_dependencies(vinpst_fcitx_rust_ffi vinpst_fcitx_rust_ffi_build)
function(vinpst_bridge_install_dir output value fallback)
  if(NOT "${${output}}" STREQUAL "")
    set(candidate "${${output}}")
  elseif(NOT "${value}" STREQUAL "")
    set(candidate "${value}")
  else()
    set(candidate "${fallback}")
  endif()
  if(IS_ABSOLUTE "${candidate}")
      file(RELATIVE_PATH prefix_relative_value "${CMAKE_INSTALL_PREFIX}" "${candidate}")
      if(NOT prefix_relative_value MATCHES "^\\.\\.($|/)")
        set(${output} "${prefix_relative_value}" PARENT_SCOPE)
      else()
        file(RELATIVE_PATH root_relative_value "/" "${candidate}")
        set(${output} "${root_relative_value}" PARENT_SCOPE)
      endif()
  else()
    set(${output} "${candidate}" PARENT_SCOPE)
  endif()
endfunction()

function(vinpst_bridge_apply_warnings target)
  if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(${target} PRIVATE -Wall -Wextra -Werror)
    if("${target}" MATCHES "_smoke$")
      # C++ smoke tests use assert for behavior checks; keep them active in
      # Release builds instead of compiling the checks and their inputs away.
      target_compile_options(${target} PRIVATE -UNDEBUG)
    endif()
  endif()
endfunction()

add_library(vinpst_fcitx_bridge_core STATIC
  src/frontend_bridge.cpp
)
add_library(vinpst_fcitx_bridge::core ALIAS vinpst_fcitx_bridge_core)
target_include_directories(vinpst_fcitx_bridge_core
  PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
)
# The public bridge headers use opaque types and deleters from the canonical
# Rust FFI header, so consumers need its include and link interface.
target_link_libraries(vinpst_fcitx_bridge_core PUBLIC vinpst_fcitx_rust_ffi)
target_compile_features(vinpst_fcitx_bridge_core PUBLIC cxx_std_20)
set_target_properties(vinpst_fcitx_bridge_core PROPERTIES POSITION_INDEPENDENT_CODE ON)
vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_core)

add_library(vinpst_fcitx_bridge_sdbus STATIC src/sd_bus_daemon_client.cpp)
add_library(vinpst_fcitx_bridge::sdbus ALIAS vinpst_fcitx_bridge_sdbus)
target_link_libraries(vinpst_fcitx_bridge_sdbus
  PUBLIC
    vinpst_fcitx_bridge::core
  PRIVATE
    vinpst_fcitx_rust_ffi
)
target_compile_features(vinpst_fcitx_bridge_sdbus PUBLIC cxx_std_20)
set_target_properties(vinpst_fcitx_bridge_sdbus PROPERTIES POSITION_INDEPENDENT_CODE ON)
vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_sdbus)

set(VINPST_DAEMON_EXECUTABLE
    "${CMAKE_INSTALL_FULL_BINDIR}/vinpst-daemon"
    CACHE FILEPATH "Installed vinpst-daemon executable used by DBus activation")
set(VINPST_DAEMON_ARGS
    ""
    CACHE STRING "Arguments used by the DBus activation service for vinpst-daemon")
set(VINPST_DAEMON_ARGS_SUFFIX "")
if(NOT "${VINPST_DAEMON_ARGS}" STREQUAL "")
  set(VINPST_DAEMON_ARGS_SUFFIX " ${VINPST_DAEMON_ARGS}")
endif()
set(VINPST_DBUS_SYSTEMD_SERVICE_LINE "")
if(VINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE)
  if("${VINPST_SYSTEMD_USER_UNIT_DIR}" STREQUAL "")
    pkg_get_variable(VINPST_SYSTEMD_USER_UNIT_DIR systemd systemduserunitdir)
  endif()
  if("${VINPST_SYSTEMD_USER_UNIT_DIR}" STREQUAL "")
    set(VINPST_SYSTEMD_USER_UNIT_DIR "${CMAKE_INSTALL_LIBDIR}/systemd/user")
  endif()
  set(VINPST_DBUS_SYSTEMD_SERVICE_LINE "SystemdService=vinpst-daemon.service")
  configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/../../data/vinpst-daemon.service.in"
    vinpst-daemon.service
    @ONLY
  )
endif()
configure_file(vinpst-addon.conf.in vinpst-addon.conf @ONLY)
configure_file(
  "${CMAKE_CURRENT_SOURCE_DIR}/../../data/org.fcitx.Vinpst.service.in"
  org.fcitx.Vinpst.service
  @ONLY
)


if(Fcitx5Core_FOUND)
  find_package(Gettext REQUIRED)
  find_package(Fcitx5ModuleDBus REQUIRED)
  find_package(Fcitx5ModuleNotifications REQUIRED)
  set(VINPST_FCITX_BUILD_LOCALEDIR "${CMAKE_CURRENT_BINARY_DIR}/locale")
  set(VINPST_FCITX_RUNTIME_BUILD_LOCALEDIR
      "${VINPST_FCITX_BUILD_LOCALEDIR}"
      CACHE STRING "Runtime build-tree locale fallback embedded in the addon")
  set(VINPST_FCITX_RUNTIME_INSTALL_LOCALEDIR
      "${CMAKE_INSTALL_FULL_LOCALEDIR}"
      CACHE STRING "Runtime installed locale root embedded in the addon")
  set(VINPST_FCITX_ZH_CN_MO
      "${VINPST_FCITX_BUILD_LOCALEDIR}/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo")
  add_custom_command(
    OUTPUT "${VINPST_FCITX_ZH_CN_MO}"
    COMMAND ${CMAKE_COMMAND} -E make_directory
            "${VINPST_FCITX_BUILD_LOCALEDIR}/zh_CN/LC_MESSAGES"
    COMMAND "${GETTEXT_MSGFMT_EXECUTABLE}" -o "${VINPST_FCITX_ZH_CN_MO}"
            "${CMAKE_CURRENT_SOURCE_DIR}/po/zh_CN.po"
    DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/po/zh_CN.po"
    COMMENT "Compiling fcitx5-vinpst zh_CN translations"
    VERBATIM
  )
  add_custom_target(vinpst_fcitx_translations ALL
                    DEPENDS "${VINPST_FCITX_ZH_CN_MO}")
  install(
    FILES "${VINPST_FCITX_ZH_CN_MO}"
    DESTINATION "${CMAKE_INSTALL_LOCALEDIR}/zh_CN/LC_MESSAGES"
  )

  add_library(vinpst_fcitx_bridge_fcitx STATIC
    src/fcitx_i18n.cpp
    src/fcitx_config.cpp
    src/fcitx_daemon_signal_monitor.cpp
    src/fcitx_candidates.cpp
    src/fcitx_key_trigger.cpp
    src/fcitx_menu_filter.cpp
    src/fcitx_menu_projection.cpp
    src/fcitx_notifications.cpp
    src/fcitx_trigger_mode.cpp
    src/fcitx_outcome.cpp
    src/fcitx_selection.cpp
  )
  add_library(vinpst_fcitx_bridge::fcitx ALIAS vinpst_fcitx_bridge_fcitx)
  target_link_libraries(vinpst_fcitx_bridge_fcitx
    PUBLIC
      vinpst_fcitx_bridge::core
      Fcitx5::Core
    PRIVATE
      vinpst_fcitx_rust_ffi
      Fcitx5::Module::Notifications
  )
  target_compile_features(vinpst_fcitx_bridge_fcitx PUBLIC cxx_std_20)
  target_compile_definitions(
    vinpst_fcitx_bridge_fcitx
    PUBLIC
      FCITX_GETTEXT_DOMAIN="fcitx5-vinpst"
    PRIVATE
      VINPST_FCITX_BUILD_LOCALEDIR="${VINPST_FCITX_RUNTIME_BUILD_LOCALEDIR}"
      VINPST_FCITX_INSTALL_LOCALEDIR="${VINPST_FCITX_RUNTIME_INSTALL_LOCALEDIR}"
  )
  add_dependencies(vinpst_fcitx_bridge_fcitx vinpst_fcitx_translations)
  if(Fcitx5Core_VERSION VERSION_GREATER_EQUAL "5.1.9")
    target_compile_definitions(
      vinpst_fcitx_bridge_fcitx
      PUBLIC VINPST_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
    )
  endif()
  set_target_properties(vinpst_fcitx_bridge_fcitx PROPERTIES POSITION_INDEPENDENT_CODE ON)
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_fcitx)

  add_library(fcitx5_vinpst_addon MODULE
    src/fcitx_addon.cpp
    src/fcitx_addon_menu.cpp
    src/fcitx_addon_factory.cpp
  )
  target_link_libraries(fcitx5_vinpst_addon
    PRIVATE
      vinpst_fcitx_bridge::sdbus
      vinpst_fcitx_bridge::fcitx
      vinpst_fcitx_rust_ffi
      Fcitx5::Core
      Fcitx5::Module::DBus
  )
  target_compile_features(fcitx5_vinpst_addon PRIVATE cxx_std_20)
  if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    # Rust and bridge archives are implementation details of the Fcitx module.
    # Do not publish their symbols through the module's dynamic symbol table.
    target_link_options(fcitx5_vinpst_addon PRIVATE "LINKER:--exclude-libs,ALL")
  endif()
  if(Fcitx5ModuleClipboard_FOUND)
    target_link_libraries(fcitx5_vinpst_addon PRIVATE Fcitx5::Module::Clipboard)
    target_compile_definitions(fcitx5_vinpst_addon PRIVATE VINPST_FCITX_HAVE_CLIPBOARD)
  endif()
  if(Fcitx5Core_VERSION VERSION_GREATER_EQUAL "5.1.12")
    target_compile_definitions(
      fcitx5_vinpst_addon
      PRIVATE VINPST_FCITX5_CORE_HAVE_ADDON_FACTORY_V2
    )
  endif()
  set_target_properties(fcitx5_vinpst_addon PROPERTIES
    PREFIX ""
    OUTPUT_NAME "fcitx5-vinpst"
  )
  vinpst_bridge_apply_warnings(fcitx5_vinpst_addon)

  if(BUILD_TESTING AND VINPST_FCITX_BRIDGE_ENABLE_TESTS)
  add_executable(
    vinpst_fcitx_bridge_daemon_signal_monitor_smoke
    tests/fcitx_daemon_signal_monitor_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_daemon_signal_monitor_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_daemon_signal_monitor_smoke)
  find_program(DBUS_RUN_SESSION_EXECUTABLE dbus-run-session REQUIRED)
  add_test(
    NAME vinpst_fcitx_bridge_daemon_signal_monitor_smoke
    COMMAND "${DBUS_RUN_SESSION_EXECUTABLE}" --
            $<TARGET_FILE:vinpst_fcitx_bridge_daemon_signal_monitor_smoke>
  )

  add_executable(
    vinpst_fcitx_bridge_notifications_smoke
    tests/fcitx_notifications_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_notifications_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_notifications_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_notifications_smoke
    COMMAND vinpst_fcitx_bridge_notifications_smoke
  )

  add_executable(
    vinpst_fcitx_bridge_i18n_smoke
    tests/fcitx_i18n_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_i18n_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  add_dependencies(vinpst_fcitx_bridge_i18n_smoke vinpst_fcitx_translations)
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_i18n_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_i18n_smoke
    COMMAND vinpst_fcitx_bridge_i18n_smoke
  )
  set_tests_properties(
    vinpst_fcitx_bridge_i18n_smoke
    PROPERTIES ENVIRONMENT
      "LANGUAGE=zh_CN;LC_ALL=zh_CN.utf8;VINPST_FCITX_LOCALEDIR=${VINPST_FCITX_BUILD_LOCALEDIR}"
  )

  add_executable(
    vinpst_fcitx_bridge_config_smoke
    tests/fcitx_config_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_config_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_config_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_config_smoke
    COMMAND vinpst_fcitx_bridge_config_smoke
  )
  set_tests_properties(
    vinpst_fcitx_bridge_config_smoke
    PROPERTIES ENVIRONMENT
      "LANGUAGE=zh_CN;LC_ALL=zh_CN.utf8;VINPST_FCITX_LOCALEDIR=${VINPST_FCITX_BUILD_LOCALEDIR}"
  )

  add_executable(
    vinpst_fcitx_bridge_candidates_smoke
    tests/fcitx_candidates_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_candidates_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_candidates_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_candidates_smoke
    COMMAND vinpst_fcitx_bridge_candidates_smoke
  )

  add_executable(
    vinpst_fcitx_bridge_key_trigger_smoke
    tests/fcitx_key_trigger_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_key_trigger_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_key_trigger_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_key_trigger_smoke
    COMMAND vinpst_fcitx_bridge_key_trigger_smoke
  )

  add_executable(
    vinpst_fcitx_bridge_menu_filter_smoke
    tests/fcitx_menu_filter_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_menu_filter_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_menu_filter_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_menu_filter_smoke
    COMMAND vinpst_fcitx_bridge_menu_filter_smoke
  )

  add_executable(
    vinpst_fcitx_bridge_menu_paging_smoke
    tests/fcitx_menu_paging_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_menu_paging_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_menu_paging_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_menu_paging_smoke
    COMMAND vinpst_fcitx_bridge_menu_paging_smoke
  )

  add_executable(
    vinpst_fcitx_bridge_trigger_mode_smoke
    tests/fcitx_trigger_mode_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_trigger_mode_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_trigger_mode_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_trigger_mode_smoke
    COMMAND vinpst_fcitx_bridge_trigger_mode_smoke
  )

  add_executable(
    vinpst_fcitx_bridge_selection_smoke
    tests/fcitx_selection_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_selection_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_selection_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_selection_smoke
    COMMAND vinpst_fcitx_bridge_selection_smoke
  )

  add_executable(
    vinpst_fcitx_bridge_input_context_outcome_smoke
    tests/fcitx_input_context_outcome_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_input_context_outcome_smoke
    PRIVATE
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_input_context_outcome_smoke)
  add_test(
    NAME vinpst_fcitx_bridge_input_context_outcome_smoke
    COMMAND vinpst_fcitx_bridge_input_context_outcome_smoke
  )

  add_executable(
    vinpst_fcitx_addon_dbus_smoke
    tests/fcitx_addon_dbus_smoke.cpp
    src/fcitx_addon.cpp
    src/fcitx_addon_menu.cpp
    src/fcitx_i18n.cpp
    src/fcitx_config.cpp
    src/fcitx_daemon_signal_monitor.cpp
    src/fcitx_key_trigger.cpp
    src/fcitx_menu_filter.cpp
    src/fcitx_menu_projection.cpp
    src/fcitx_notifications.cpp
    src/fcitx_trigger_mode.cpp
    src/fcitx_selection.cpp
  )
  target_compile_definitions(
    vinpst_fcitx_addon_dbus_smoke
    PRIVATE
      FCITX_GETTEXT_DOMAIN="fcitx5-vinpst"
      VINPST_FCITX_BUILD_LOCALEDIR="${VINPST_FCITX_RUNTIME_BUILD_LOCALEDIR}"
      VINPST_FCITX_INSTALL_LOCALEDIR="${CMAKE_INSTALL_FULL_LOCALEDIR}"
  )
  add_dependencies(vinpst_fcitx_addon_dbus_smoke vinpst_fcitx_translations)

  target_link_libraries(
    vinpst_fcitx_addon_dbus_smoke
    PRIVATE
      vinpst_fcitx_bridge::sdbus
      vinpst_fcitx_rust_ffi
      Fcitx5::Core
      Fcitx5::Module::DBus
      Fcitx5::Module::Notifications
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_addon_dbus_smoke)

  add_executable(
    vinpst_fcitx_native_addon_dbus_smoke
    tests/native_fcitx_addon_dbus_smoke.cpp
    src/fcitx_addon.cpp
    src/fcitx_addon_menu.cpp
  )
  target_compile_definitions(
    vinpst_fcitx_native_addon_dbus_smoke
    PRIVATE
      VINPST_FCITX_BUILD_LOCALEDIR="${VINPST_FCITX_RUNTIME_BUILD_LOCALEDIR}"
      VINPST_FCITX_INSTALL_LOCALEDIR="${CMAKE_INSTALL_FULL_LOCALEDIR}"
  )
  add_dependencies(vinpst_fcitx_native_addon_dbus_smoke vinpst_fcitx_translations)
  target_link_libraries(
    vinpst_fcitx_native_addon_dbus_smoke
    PRIVATE
      vinpst_fcitx_bridge::sdbus
      vinpst_fcitx_bridge::fcitx
      vinpst_fcitx_rust_ffi
      Fcitx5::Core
      Fcitx5::Module::DBus
      Fcitx5::Module::Notifications
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_native_addon_dbus_smoke)
  endif()

  vinpst_bridge_install_dir(
    VINPST_FCITX_MODULE_INSTALL_DIR
    "${FCITX_INSTALL_LIBDIR}/fcitx5"
    "${CMAKE_INSTALL_LIBDIR}/fcitx5"
  )
  vinpst_bridge_install_dir(
    VINPST_FCITX_ADDON_INSTALL_DIR
    "${FCITX_INSTALL_PKGDATADIR}/addon"
    "${CMAKE_INSTALL_DATADIR}/fcitx5/addon"
  )
  install(TARGETS fcitx5_vinpst_addon DESTINATION "${VINPST_FCITX_MODULE_INSTALL_DIR}")
  install(
    FILES "${CMAKE_CURRENT_BINARY_DIR}/vinpst-addon.conf"
    DESTINATION "${VINPST_FCITX_ADDON_INSTALL_DIR}"
    RENAME vinpst.conf
  )
  if(VINPST_FCITX_BRIDGE_INSTALL_DBUS_SERVICE)
    install(
      FILES "${CMAKE_CURRENT_BINARY_DIR}/org.fcitx.Vinpst.service"
      DESTINATION "${CMAKE_INSTALL_DATADIR}/dbus-1/services"
    )
  endif()
  if(VINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE)
    set(VINPST_SYSTEMD_USER_UNIT_INSTALL_DIR
        "${VINPST_SYSTEMD_USER_UNIT_DIR}")
    install(
      FILES "${CMAKE_CURRENT_BINARY_DIR}/vinpst-daemon.service"
      DESTINATION "${VINPST_SYSTEMD_USER_UNIT_INSTALL_DIR}"
    )
  endif()
else()
  if(VINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE)
    message(FATAL_ERROR "Fcitx5Core is required for the addon module target")
  endif()
  message(STATUS "Fcitx5Core not found; skipping fcitx5-vinpst module target")
endif()

if(VINPST_FCITX_BRIDGE_ENABLE_FCITX_DEPS)
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

if(BUILD_TESTING AND VINPST_FCITX_BRIDGE_ENABLE_TESTS)
  function(vinpst_bridge_smoke_test target source)
    add_executable(${target} ${source})
    target_link_libraries(${target} PRIVATE vinpst_fcitx_bridge::core)
    vinpst_bridge_apply_warnings(${target})
    add_test(NAME ${target} COMMAND ${target})
  endfunction()

  add_executable(
    vinpst_fcitx_bridge_dbus_smoke
    tests/sd_bus_daemon_client_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_dbus_smoke
    PRIVATE
      vinpst_fcitx_bridge::sdbus
      vinpst_fcitx_rust_ffi
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_dbus_smoke)

  add_executable(
    vinpst_fcitx_bridge_native_dbus_smoke
    tests/native_frontend_bridge_dbus_smoke.cpp
  )
  target_link_libraries(
    vinpst_fcitx_bridge_native_dbus_smoke
    PRIVATE
      vinpst_fcitx_bridge::sdbus
      vinpst_fcitx_bridge::fcitx
      Fcitx5::Core
  )
  vinpst_bridge_apply_warnings(vinpst_fcitx_bridge_native_dbus_smoke)
endif()
