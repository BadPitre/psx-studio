/*
 * PSX Studio - SceneFormat v1 loader (Phases 2-5)
 * Format specification: docs/SCENE-FORMAT.md
 *
 * A scene is one contiguous .psc file on the CD: header, asset tables,
 * entities, script hash table, then embedded PMD/TIM blobs. Everything is
 * loaded into the per-scene arena with a single CD read, then fixed up in
 * place. Entity transforms are mutable (scripts, live tweaking): world
 * matrices are recomputed every frame by Scene_UpdateWorld().
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#ifndef PSXSTUDIO_SCENE_H
#define PSXSTUDIO_SCENE_H

#include <stdint.h>
#include <psxgpu.h>
#include <psxgte.h>

#include "engine.h"
#include "pmd.h"

#define PSC_VERSION		1
#define PSC_NO_INDEX	0xFFFF

#define SCENE_MAX_MODELS	16
#define SCENE_MAX_TEXTURES	8
#define SCENE_MAX_SCRIPTS	32

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
	uint16_t	script_count;		/* extension v1.1 (0 sur les anciens fichiers) */
	uint32_t	scripts_offset;		/* table de hashes FNV-1a 32 */
	uint32_t	lights_offset;		/* extension v1.2 : table des lumieres */
	uint16_t	light_count;		/* entites-lumieres (2 max, lignes GTE 1-2) */
	uint8_t		reserved[2];
} PscHeader;

/* Une entree de la table des lumieres (v1.2) : l'entite donne la
 * direction (elle eclaire le long de son axe -Z local), la couleur est
 * ici. */
typedef struct {
	uint16_t	entity;
	uint8_t		color[3];
	uint8_t		pad;
} PscLightRec;

/* Flags d'entite. */
#define ENTITY_FLAG_LIGHT	(1 << 0)
#define ENTITY_FLAG_CAMERA	(1 << 1)

#define SCENE_MAX_ENTITY_LIGHTS	2

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
	uint16_t	script;			/* script table index + 1, 0 = none */
} PscEntityRec;

/* Balise editeur ---------------------------------------------------------- */
/* Localisable dans un dump RAM (magic), decrit la table d'entites pour le
 * live tweaking depuis l'editeur via l'API web de PCSX-Redux. */

typedef struct {
	char		magic[12];		/* "PSXSTUDIOBCN" */
	uint16_t	version;		/* 1 */
	uint16_t	entity_size;	/* sizeof(Entity) */
	uint32_t	entities_addr;	/* adresse de entities[0] */
	uint16_t	entity_count;
	uint16_t	pos_offset;		/* offsetof(Entity, pos) */
	uint16_t	rot_offset;
	uint16_t	scale_offset;
} EditorBeacon;

extern EditorBeacon g_editor_beacon;

/* Runtime state ----------------------------------------------------------- */

typedef struct {
	const PscHeader*	header;
	PmdModel			models[SCENE_MAX_MODELS];
	/* AABB modele (espace local, avant echelle) pour la physique. */
	SVECTOR				model_min[SCENE_MAX_MODELS];
	SVECTOR				model_max[SCENE_MAX_MODELS];
	Entity*				entities;		/* arena-allocated, mutable */
	int					entity_count;
	/* Scripts resolus par hash (index table -> registre du jeu). */
	const ScriptDef*	scripts[SCENE_MAX_SCRIPTS];
	/* Lumieres : ligne 0 = soleil des settings (fixe), lignes 1-2 =
	 * entites-lumieres, re-derivees chaque frame de leur rotation. */
	MATRIX				light_mtx;
	int16_t				light_entities[SCENE_MAX_ENTITY_LIGHTS];
	int					light_entity_count;
	/* Premiere entite camera (-1 : aucune) : vue initiale de la scene. */
	int16_t				camera_entity;
	CVECTOR				background;
} Scene;

/* Load a .psc file from the CD into the scene arena (which is reset).
 * Uploads textures to VRAM, fixes up models, resolves scripts against
 * g_scripts and fills the editor beacon. Returns 0 on success. */
int Scene_LoadFromCd(Scene* scene, const char* path);

/* Recompute every entity world matrix from the local transforms (call
 * once per frame, before Scene_Draw). */
void Scene_UpdateWorld(Scene* scene);

/* Distance d'affichage : les entites au-dela (en Z vue) ne sont pas
 * dessinees. 0 = illimitee. Posee par Scene_ApplyCamera. */
void Scene_SetDrawDistance(int32_t d);

/* Appelle on_start / on_update des scripts attaches. */
void Scene_StartScripts(Scene* scene);
void Scene_UpdateScripts(Scene* scene);

/* Draw every visible entity. `view` is the world-to-camera matrix. */
uint8_t* Scene_Draw(const Scene* scene, const MATRIX* view, uint32_t* ot,
	int ot_length, uint8_t* packet, uint8_t* packet_limit);

int Scene_TriangleCount(const Scene* scene);

/* Read a whole CD file into the scene arena WITHOUT resetting it. */
void* Scene_ReadFileToArena(const char* path, uint32_t* size_out);

/* Acces global (API scripts) : derniere scene chargee. */
Scene* Scene_Current(void);

#endif
