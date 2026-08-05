# PSX Studio - Phase 1: poc-renderer
# MIT License - build script based on the PSn00bSDK template (MPL)

cmake_minimum_required(VERSION 3.21)

project(
	poc-renderer
	LANGUAGES    C CXX ASM
	VERSION      0.1.0
	DESCRIPTION  "PSX Studio Phase 1 - PMD model viewer (orbital camera, GTE lighting)"
)

psn00bsdk_add_executable(poc GPREL main.c pmd.c)

# Assets produced by the Rust pipeline (pipeline/psxpipe), committed so the
# runtime builds without Rust installed. To regenerate:
#   cd pipeline
#   cargo run -- gltf2pmd samples/cube.gltf -o ../runtime/poc-renderer/assets/cube.pmd
#   cargo run -- png2tim  samples/checker.png -o ../runtime/poc-renderer/assets/checker.tim
psn00bsdk_target_incbin(poc PRIVATE cube_pmd    ${PROJECT_SOURCE_DIR}/assets/cube.pmd)
psn00bsdk_target_incbin(poc PRIVATE checker_tim ${PROJECT_SOURCE_DIR}/assets/checker.tim)

psn00bsdk_add_cd_image(
	iso      # Target name
	poc      # Output: poc.bin + poc.cue
	iso.xml
	DEPENDS poc system.cnf
)
