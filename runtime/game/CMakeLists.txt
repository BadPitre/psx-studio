# PSX Studio - Phase 5: game (boucle de jeu + scripts C)
# MIT License - build script based on the PSn00bSDK template (MPL)

cmake_minimum_required(VERSION 3.21)

project(
	game
	LANGUAGES    C CXX ASM
	VERSION      0.1.0
	DESCRIPTION  "PSX Studio Phase 5 - demo village jouable (scripts, collisions, dialogue)"
)

psn00bsdk_add_executable(game GPREL
	main.c
	scripts/player.c
	scripts/npc.c
	scripts/torche.c
	scripts/hud.c
	scripts/dialogue.c
	scripts/pause.c
	scripts/registry.c
	${PROJECT_SOURCE_DIR}/../engine/pmd.c
	${PROJECT_SOURCE_DIR}/../engine/scene.c
	${PROJECT_SOURCE_DIR}/../engine/ui.c
	${PROJECT_SOURCE_DIR}/../engine/gameapi.c
	${PROJECT_SOURCE_DIR}/../engine/vm.c
	${PROJECT_SOURCE_DIR}/../engine/sfx.c
)
target_include_directories(game PRIVATE ${PROJECT_SOURCE_DIR}/../engine)

# Les scenes/SFX sont ceux du player (regeneres par gen_scenes).
psn00bsdk_add_cd_image(
	iso      # Target name
	game     # Output: game.bin + game.cue
	iso.xml
	DEPENDS game system.cnf
)
