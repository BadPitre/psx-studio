# PSX Studio - Phase 2: scene player
# MIT License - build script based on the PSn00bSDK template (MPL)

cmake_minimum_required(VERSION 3.21)

project(
	player
	LANGUAGES    C CXX ASM
	VERSION      0.1.0
	DESCRIPTION  "PSX Studio Phase 2 - SceneFormat v1 player (CD loading, hierarchy, scene switching)"
)

psn00bsdk_add_executable(player GPREL
	main.c
	scene.c
	${PROJECT_SOURCE_DIR}/../engine/pmd.c
)
target_include_directories(player PRIVATE ${PROJECT_SOURCE_DIR}/../engine)

# The demo scenes are committed; to regenerate them:
#   cd pipeline && cargo run --example gen_scenes -- samples ../runtime/player/assets
psn00bsdk_add_cd_image(
	iso      # Target name
	player   # Output: player.bin + player.cue
	iso.xml
	DEPENDS player system.cnf assets/scene0.psc assets/scene1.psc
)
