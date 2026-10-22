# Copies the license files of the crates.io packages that a built manifest
# links into the target from cargo's registry sources to OUTPUT/<name>-<version>.
# MANIFEST may be a glob, as cubeb-sys builds its Rust backends in its OUT_DIR;
# the most recently written match is used.
#
#   cmake -DCARGO=<cargo> -DMANIFEST=<Cargo.toml> -DTARGET=<triple> -DOUTPUT=<dir> [-DFEATURES=<list>] -P CrateLicenses.cmake

if(DEFINED ENV{CARGO_HOME})
    set(CARGO_HOME "$ENV{CARGO_HOME}")
elseif(WIN32)
    set(CARGO_HOME "$ENV{USERPROFILE}/.cargo")
else()
    set(CARGO_HOME "$ENV{HOME}/.cargo")
endif()

file(GLOB MANIFESTS "${MANIFEST}")
set(NEWEST 0)
foreach(CANDIDATE ${MANIFESTS})
    file(TIMESTAMP "${CANDIDATE}" MODIFIED "%s")
    if(MODIFIED GREATER NEWEST)
        set(NEWEST ${MODIFIED})
        set(MANIFEST_PATH "${CANDIDATE}")
    endif()
endforeach()

if(NOT MANIFEST_PATH)
    message(FATAL_ERROR "No manifest matches ${MANIFEST}")
endif()

if(FEATURES)
    set(FEATURE_ARGS --features "${FEATURES}")
endif()

execute_process(
    COMMAND "${CARGO}" tree --frozen --manifest-path "${MANIFEST_PATH}" ${FEATURE_ARGS}
        -e normal,no-proc-macro --target "${TARGET}" --prefix none --format "{p}"
    OUTPUT_VARIABLE PACKAGES
    RESULT_VARIABLE RESULT
)
if(NOT RESULT EQUAL 0)
    message(FATAL_ERROR "cargo tree failed for ${MANIFEST_PATH}")
endif()

string(REPLACE "\n" ";" PACKAGES "${PACKAGES}")

foreach(PACKAGE ${PACKAGES})
    # Path dependencies are printed with their directory and are skipped; their
    # licenses come from the submodules.
    if(NOT PACKAGE MATCHES "^([^ ]+) v([^ ]+)( \\(\\*\\))?$")
        continue()
    endif()

    set(PACKAGE_ID "${CMAKE_MATCH_1}-${CMAKE_MATCH_2}")
    file(GLOB SOURCE_DIRS "${CARGO_HOME}/registry/src/*/${PACKAGE_ID}")

    if(NOT SOURCE_DIRS)
        message(FATAL_ERROR "${PACKAGE_ID} is not in ${CARGO_HOME}/registry/src")
    endif()

    list(GET SOURCE_DIRS 0 SOURCE_DIR)
    file(GLOB LICENSE_FILES LIST_DIRECTORIES false "${SOURCE_DIR}/LICENSE*" "${SOURCE_DIR}/license*"
        "${SOURCE_DIR}/COPYING*" "${SOURCE_DIR}/NOTICE*" "${SOURCE_DIR}/UNLICENSE*")

    if(NOT LICENSE_FILES)
        message(FATAL_ERROR "${SOURCE_DIR} has no license file")
    endif()

    file(COPY ${LICENSE_FILES} DESTINATION "${OUTPUT}/${PACKAGE_ID}")
endforeach()
