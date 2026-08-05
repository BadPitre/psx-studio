/*
 * PSX Studio - SceneFormat v1 loader (Phase 2)
 * Format specification: docs/SCENE-FORMAT.md
 *
 * A scene is one contiguous .psc file on the CD: header, asset tables,
 * entities, then embedded PMD/TIM blobs. Everything is loaded into the
 * per-scene arena with a single CD read, then fixed up in place.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#ifndef PSXSTUDIO_SCENE_H
#define PSXSTUDIO_SCENE_H

#include <stdint.h>
#include <psxgpu.h>
#include <psxgte.h>

#include "pmd.h"

#define PSC_VERSION		1
#define PSC_NO_INDEX	0xFFFF

#define SCENE_MAX_MODELS	16
#define SCENE_MAX_TEXTURES	8

/* On-disk structures ------------------------------------------------------ */

typedef struct {
	char		magic[4];		/* "PSC1" */
	uint16_t	version;
	uint16_t	flags;
	uint32_t	total_size;
	uint16_t	model_count;
	uint16_t	texture_count;
	uint16_t	entity_count;
	uint16_t	pad;
	uint32_t	models_offset;
	uint32_t	textures_offset;
	uint32_t	entities_offset;
	uint8_t		background[4];
	uint8_t		ambient[4];
	uint8_t		light_color[4];
	int16_t		light_toward[3];	/* 4.12, world space, points AT the source */
	uint8_t		reserved[14];
} PscHeader;

typedef struct {
	uint32_t	offset;
	uint32_t	size;
	uint16_t	texture;		/* texture table index or PSC_NO_INDEX */
	uint16_t	pad;
} PscModelEntry;

typedef struct {
	uint32_t	offset;
	uint32_t	size;
} PscTextureEntry;

typedef struct {
	SVECTOR		pos;
	SVECTOR		rot;			/* 4096 = full turn */
	SVECTOR		scale;			/* 4.12 */
	uint16_t	parent;			/* entity index (sorted: always < own) or PSC_NO_INDEX */
	uint16_t	model;			/* model table index or PSC_NO_INDEX */
	uint16_t	flags;
	uint16_t	pad;
} PscEntityRec;

/* Runtime state ----------------------------------------------------------- */

typedef struct {
	MATRIX	world;			/* rotation*scale + world translation */
	MATRIX	light_rot;		/* rotation only (unscaled), for lighting */
	int16_t	model;			/* model index or -1 */
} SceneEntity;

typedef struct {
	const PscHeader*	header;
	PmdModel			models[SCENE_MAX_MODELS];
	SceneEntity*		entities;		/* arena-allocated */
	int					entity_count;
	MATRIX				light_mtx;		/* row 0 = vector toward the light */
	CVECTOR				background;
} Scene;

/* Load a .psc file from the CD into the scene arena (which is reset).
 * Uploads textures to VRAM and fixes up models in place.
 * `path` is an ISO9660 path such as "\\SCENE0.PSC;1".
 * Returns 0 on success, negative on error. */
int Scene_LoadFromCd(Scene* scene, const char* path);

/* Draw every entity. `view` is the world-to-camera matrix (rotation +
 * translation). GTE color matrix / back color are set at load time.
 * Returns the advanced packet pointer. */
uint8_t* Scene_Draw(const Scene* scene, const MATRIX* view, uint32_t* ot,
	int ot_length, uint8_t* packet, uint8_t* packet_limit);

int Scene_TriangleCount(const Scene* scene);

#endif
