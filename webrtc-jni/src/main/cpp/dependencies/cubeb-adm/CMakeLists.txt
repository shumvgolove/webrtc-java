cmake_minimum_required(VERSION 3.13)
project(cubeb-adm)

find_program(CARGO_EXECUTABLE cargo)
if(NOT CARGO_EXECUTABLE)
    message(FATAL_ERROR "cargo not found; the cubeb audio device module needs a Rust toolchain")
endif()

get_filename_component(CUBEB_ADM_CRATE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../rust" ABSOLUTE)
get_filename_component(SIGNAL_WEBRTC_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../signal-webrtc" ABSOLUTE)
set(RINGRTC_BRIDGE_SOURCE "${SIGNAL_WEBRTC_DIR}/ringrtc/rffi/src/audio_device.cc")
set(RINGRTC_RFFI_API_DIR "${CUBEB_ADM_CRATE_DIR}/third_party/ringrtc/src/rust/src/webrtc_include")
set(RINGRTC_RFFI_API_HEADERS audio_device_intf.h rffi_defs.h)

# Short, as MSBuild in cubeb's CMake cannot create its logs under paths longer than MAX_PATH.
set(CUBEB_ADM_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
set(CUBEB_ADM_LIB "${CUBEB_ADM_CARGO_TARGET_DIR}/${CUBEB_ADM_RUST_TARGET}/release/${CMAKE_STATIC_LIBRARY_PREFIX}cubeb_adm${CMAKE_STATIC_LIBRARY_SUFFIX}")

string(REPLACE "-" "_" RUST_TARGET_ENV "${CUBEB_ADM_RUST_TARGET}")
string(TOUPPER "${RUST_TARGET_ENV}" RUST_TARGET_ENV_UPPER)

set(CARGO_ENV "")

if(LINUX)
    # Build cubeb's C++ with the JNI compiler against webrtc's bundled libc++
    # instead of libstdc++, so the library carries a single C++ runtime.
    get_target_property(WEBRTC_DEFINITIONS webrtc INTERFACE_COMPILE_DEFINITIONS)
    list(FILTER WEBRTC_DEFINITIONS INCLUDE REGEX "^_LIBCPP_")
    get_target_property(WEBRTC_LIBCXX_INCLUDE_DIRS webrtc INTERFACE_INCLUDE_DIRECTORIES)
    list(FILTER WEBRTC_LIBCXX_INCLUDE_DIRS INCLUDE REGEX "libc\\+\\+")

    set(CUBEB_CFLAGS "")
    if(CMAKE_SYSROOT)
        set(CUBEB_CFLAGS "--sysroot=${CMAKE_SYSROOT}")

        string(REPLACE "-unknown-" "-" MULTIARCH "${CUBEB_ADM_RUST_TARGET}")

        # pkg-config must only see the sysroot, or cubeb links libraries of the
        # build host (speexdsp) and probes backends the sysroot does not have.
        list(APPEND CARGO_ENV
            "PKG_CONFIG_ALLOW_CROSS=1"
            "PKG_CONFIG_SYSROOT_DIR=${CMAKE_SYSROOT}"
            "PKG_CONFIG_LIBDIR=${CMAKE_SYSROOT}/usr/lib/${MULTIARCH}/pkgconfig:${CMAKE_SYSROOT}/usr/lib/pkgconfig:${CMAKE_SYSROOT}/usr/share/pkgconfig"
        )
    endif()

    set(CUBEB_CXXFLAGS "${CUBEB_CFLAGS} -nostdinc++")
    foreach(DIR ${WEBRTC_LIBCXX_INCLUDE_DIRS})
        string(APPEND CUBEB_CXXFLAGS " -isystem ${DIR}")
    endforeach()
    foreach(DEFINITION ${WEBRTC_DEFINITIONS})
        string(APPEND CUBEB_CXXFLAGS " -D${DEFINITION}")
    endforeach()

    list(APPEND CARGO_ENV
        "CC_${RUST_TARGET_ENV}=${CMAKE_C_COMPILER}"
        "CXX_${RUST_TARGET_ENV}=${CMAKE_CXX_COMPILER}"
        "AR_${RUST_TARGET_ENV}=${CMAKE_AR}"
        "CFLAGS_${RUST_TARGET_ENV}=${CUBEB_CFLAGS}"
        "CXXFLAGS_${RUST_TARGET_ENV}=${CUBEB_CXXFLAGS}"
        # cubeb's CMake links test executables, which the host ld cannot do for a cross target.
        "LDFLAGS=-fuse-ld=lld"
    )
elseif(APPLE AND CMAKE_OSX_DEPLOYMENT_TARGET)
    list(APPEND CARGO_ENV "MACOSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}")
elseif(WIN32)
    # Match the static MSVC runtime (/MT) of the JNI library.
    list(APPEND CARGO_ENV "CARGO_TARGET_${RUST_TARGET_ENV_UPPER}_RUSTFLAGS=-Ctarget-feature=+crt-static")
    # Inside MSBuild, cc-rs uses cl.exe with the inherited INCLUDE of the ClangCL
    # toolset, which lists clang's headers first; without it, cc-rs sets up MSVC itself.
    list(APPEND CARGO_ENV "--unset=VSTEL_MSBuildProjectFullPath")
endif()

# License texts of the code linked in, installed beside webrtc's (see ../../CMakeLists.txt).
set(LICENSE_DIR "${CMAKE_CURRENT_BINARY_DIR}/licenses")
set(CUBEB_DIR "${CUBEB_ADM_CRATE_DIR}/third_party/cubeb-rs/cubeb-sys/libcubeb")
set(CUBEB_BACKEND_BUILD_DIR "${CUBEB_ADM_CARGO_TARGET_DIR}/${CUBEB_ADM_RUST_TARGET}/release/build/cubeb-sys-*/out/libcubeb/src")

configure_file("${CUBEB_ADM_CRATE_DIR}/third_party/ringrtc/LICENSE" "${LICENSE_DIR}/ringrtc/LICENSE" COPYONLY)
configure_file("${CUBEB_ADM_CRATE_DIR}/third_party/cubeb-rs/LICENSE" "${LICENSE_DIR}/cubeb-rs/LICENSE" COPYONLY)
configure_file("${CUBEB_DIR}/LICENSE" "${LICENSE_DIR}/cubeb/LICENSE" COPYONLY)

# The bridge and the bundled speex resampler state their licenses in their source headers.
foreach(NOTICE_SOURCE "signal-webrtc;${RINGRTC_BRIDGE_SOURCE}" "speex;${CUBEB_DIR}/subprojects/speex/resample.c")
    list(GET NOTICE_SOURCE 0 NOTICE_NAME)
    list(GET NOTICE_SOURCE 1 NOTICE_FILE)
    file(READ "${NOTICE_FILE}" SOURCE_TEXT)
    string(REGEX MATCH "^/\\*([^*]|\\*+[^*/])*\\*+/" NOTICE "${SOURCE_TEXT}")
    file(WRITE "${LICENSE_DIR}/${NOTICE_NAME}/NOTICE" "${NOTICE}\n")
endforeach()

execute_process(COMMAND rustc --print sysroot WORKING_DIRECTORY "${CUBEB_ADM_CRATE_DIR}"
    OUTPUT_VARIABLE RUST_SYSROOT OUTPUT_STRIP_TRAILING_WHITESPACE)
configure_file("${RUST_SYSROOT}/share/doc/rust/COPYRIGHT-library.html" "${LICENSE_DIR}/rust-std/COPYRIGHT-library.html" COPYONLY)

# Collected anew after each cargo build, so a version bump leaves no stale texts.
set(CRATE_LICENSE_COMMANDS
    COMMAND ${CMAKE_COMMAND} -E remove_directory ${LICENSE_DIR}/crates
    COMMAND ${CMAKE_COMMAND} -DCARGO=${CARGO_EXECUTABLE} -DMANIFEST=${CUBEB_ADM_CRATE_DIR}/Cargo.toml
        -DTARGET=${CUBEB_ADM_RUST_TARGET} -DOUTPUT=${LICENSE_DIR}/crates -P ${CMAKE_CURRENT_SOURCE_DIR}/CrateLicenses.cmake
)

# cubeb links its Rust backend for the platform, which cargo builds inside cubeb-sys's OUT_DIR.
if(LINUX)
    set(CUBEB_BACKEND cubeb-pulse-rs)
elseif(APPLE)
    set(CUBEB_BACKEND cubeb-coreaudio-rs)
endif()

if(CUBEB_BACKEND)
    configure_file("${CUBEB_DIR}/src/${CUBEB_BACKEND}/LICENSE" "${LICENSE_DIR}/${CUBEB_BACKEND}/LICENSE" COPYONLY)
    list(APPEND CRATE_LICENSE_COMMANDS
        COMMAND ${CMAKE_COMMAND} -DCARGO=${CARGO_EXECUTABLE} -DMANIFEST=${CUBEB_BACKEND_BUILD_DIR}/${CUBEB_BACKEND}/Cargo.toml
            -DFEATURES=gecko-in-tree -DTARGET=${CUBEB_ADM_RUST_TARGET} -DOUTPUT=${LICENSE_DIR}/crates
            -P ${CMAKE_CURRENT_SOURCE_DIR}/CrateLicenses.cmake
    )
endif()

install(DIRECTORY "${LICENSE_DIR}/" DESTINATION "${CMAKE_INSTALL_PREFIX}/META-INF/licenses/cubeb-adm" COMPONENT Runtime)

# Cargo decides what to rebuild; running it is a no-op when nothing changed.
# The crate directory is the working directory so its rust-toolchain.toml applies.
add_custom_target(cubeb-adm-cargo
    COMMAND ${CMAKE_COMMAND} -E env ${CARGO_ENV}
        "${CARGO_EXECUTABLE}" build --release --locked
        --target ${CUBEB_ADM_RUST_TARGET}
        --target-dir "${CUBEB_ADM_CARGO_TARGET_DIR}"
    ${CRATE_LICENSE_COMMANDS}
    WORKING_DIRECTORY "${CUBEB_ADM_CRATE_DIR}"
    BYPRODUCTS "${CUBEB_ADM_LIB}"
    USES_TERMINAL
    VERBATIM
)

# The bridge includes the RingRTC headers as "rffi/api/<name>", a path that
# exists only in Signal's webrtc checkout; forward it to the submodule.
set(RFFI_FORWARDING_INCLUDE_DIR "${CMAKE_CURRENT_BINARY_DIR}/include")
foreach(HEADER ${RINGRTC_RFFI_API_HEADERS})
    file(GENERATE
        OUTPUT "${RFFI_FORWARDING_INCLUDE_DIR}/rffi/api/${HEADER}"
        CONTENT "#include \"${RINGRTC_RFFI_API_DIR}/${HEADER}\"\n"
    )
endforeach()

add_library(${PROJECT_NAME} INTERFACE)
add_dependencies(${PROJECT_NAME} cubeb-adm-cargo)
file(GLOB CUBEB_ADM_SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/../../src/cubeb/*.cpp")
target_sources(${PROJECT_NAME} INTERFACE "${RINGRTC_BRIDGE_SOURCE}" ${CUBEB_ADM_SOURCES})
target_include_directories(${PROJECT_NAME} INTERFACE "${SIGNAL_WEBRTC_DIR}/ringrtc" "${RFFI_FORWARDING_INCLUDE_DIR}")
target_link_libraries(${PROJECT_NAME} INTERFACE "${CUBEB_ADM_LIB}")

# The native libraries the staticlib needs, from `cargo rustc -- --print native-static-libs`.
if(LINUX)
    # stdc++ and gcc_s are left out: cubeb's C++ uses the bundled libc++ and
    # -static-libgcc already provides the unwinder.
    target_link_libraries(${PROJECT_NAME} INTERFACE pulse util rt pthread m dl)
elseif(APPLE)
    target_link_libraries(${PROJECT_NAME} INTERFACE "-framework AudioUnit" "-framework CoreAudio" "-framework CoreServices")
elseif(WIN32)
    target_link_libraries(${PROJECT_NAME} INTERFACE avrt ksuser ole32 user32 winmm ntdll userenv ws2_32 dbghelp)
endif()
