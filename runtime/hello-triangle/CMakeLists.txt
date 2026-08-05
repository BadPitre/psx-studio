# PSX Studio - Phase 0: hello-triangle
# MIT License - build script based on the PSn00bSDK template (MPL)

cmake_minimum_required(VERSION 3.21)

project(
	hello-triangle
	LANGUAGES    C CXX ASM
	VERSION      0.1.0
	DESCRIPTION  "PSX Studio Phase 0 - rotating 3D triangle"
)

psn00bsdk_add_executable(hello GPREL main.c)

psn00bsdk_add_cd_image(
	iso      # Target name
	hello    # Output: hello.bin + hello.cue
	iso.xml
	DEPENDS hello system.cnf
)
