/*
 * PSX Studio - SceneFormat v1 loader (Phase 2)
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <psxcd.h>
#include <psxgpu.h>
#include <psxgte.h>
#include <inline_c.h>

#include "scene.h"

_Static_assert(sizeof(PscHeader) == 64, "PSC header must be 64 bytes");
_Static_assert(sizeof(PscModelEntry) == 12, "model entry must be 12 bytes");
_Static_assert(sizeof(PscTextureEntry) == 8, "texture entry must be 8 bytes");
_Static_assert(sizeof(PscEntityRec) == 32, "entity record must be 32 bytes");

/* Scene arena ------------------------------------------------------------- */
/* All per-scene data (the raw .psc file + runtime entity array) lives here.
 * Loading a scene resets the arena: no generic allocator, no leaks. */

#define ARENA_SIZE (1024 * 1024)

static uint8_t	arena[ARENA_SIZE] __attribute__((aligned(2048)));
static size_t	arena_used;

static void* Arena_Alloc(size_t size)
{
	size = (size + 3) & ~(size_t)3;
	assert(arena_used + size <= ARENA_SIZE);

	void* ptr = &arena[arena_used];
	arena_used += size;
	return ptr;
}

/* Fixed-point helpers (load-time only, plain C is plenty fast) ------------ */

static void MulMat3(const MATRIX* a, const MATRIX* b, MATRIX* out)
{
	MATRIX tmp;
	for (int r = 0; r < 3; r++)
	{
		for (int c = 0; c < 3; c++)
		{
			int32_t sum = 0;
			for (int k = 0; k < 3; k++)
				sum += (int32_t)a->m[r][k] * b->m[k][c];
			tmp.m[r][c] = (int16_t)(sum >> 12);
		}
	}
	memcpy(out->m, tmp.m, sizeof(tmp.m));
}

/* out = a.rot * v (4.12 rotation applied to an integer vector) */
static void MulMatVec(const MATRIX* a, const int32_t v[3], int32_t out[3])
{
	int32_t tmp[3];
	for (int r = 0; r < 3; r++)
	{
		int32_t sum = 0;
		for (int k = 0; k < 3; k++)
			sum += a->m[r][k] * v[k];
		tmp[r] = sum >> 12;
	}
	out[0] = tmp[0];
	out[1] = tmp[1];
	out[2] = tmp[2];
}

/* Loading ----------------------------------------------------------------- */

int Scene_LoadFromCd(Scene* scene, const char* path)
{
	CdlFILE file;
	if (!CdSearchFile(&file, path))
		return -1;

	arena_used = 0;
	size_t sectors = (file.size + 2047) / 2048;
	uint8_t* data = (uint8_t*)Arena_Alloc(sectors * 2048);

	CdControl(CdlSetloc, &file.pos, 0);
	CdRead(sectors, (uint32_t*)data, CdlModeSpeed);
	if (CdReadSync(0, 0) < 0)
		return -2;

	const PscHeader* header = (const PscHeader*)data;
	if (memcmp(header->magic, "PSC1", 4) != 0)
		return -3;
	if (header->version != PSC_VERSION)
		return -4;
	if (header->model_count > SCENE_MAX_MODELS ||
		header->texture_count > SCENE_MAX_TEXTURES)
		return -5;

	scene->header = header;

	const PscModelEntry* model_table =
		(const PscModelEntry*)(data + header->models_offset);
	const PscTextureEntry* texture_table =
		(const PscTextureEntry*)(data + header->textures_offset);
	const PscEntityRec* entity_table =
		(const PscEntityRec*)(data + header->entities_offset);

	/* Upload every texture to VRAM and record its tpage/CLUT. */
	uint16_t tpage[SCENE_MAX_TEXTURES] = {0};
	uint16_t clut[SCENE_MAX_TEXTURES] = {0};
	for (int i = 0; i < header->texture_count; i++)
	{
		TIM_IMAGE tim;
		if (GetTimInfo((const uint32_t*)(data + texture_table[i].offset), &tim) != 0)
			return -6;
		LoadImage(tim.prect, tim.paddr);
		if (tim.mode & 0x8)
		{
			LoadImage(tim.crect, tim.caddr);
			clut[i] = getClut(tim.crect->x, tim.crect->y);
		}
		tpage[i] = getTPage(tim.mode & 0x3, 0, tim.prect->x, tim.prect->y);
	}
	DrawSync(0);

	/* Fix up every model in place. */
	for (int i = 0; i < header->model_count; i++)
	{
		uint16_t tex = model_table[i].texture;
		int err = Pmd_Load(&scene->models[i], data + model_table[i].offset,
			tex != PSC_NO_INDEX ? tpage[tex] : 0,
			tex != PSC_NO_INDEX ? clut[tex] : 0);
		if (err != 0)
			return -7;
	}

	/* Compute world matrices in one forward pass (parents come first). */
	scene->entity_count = header->entity_count;
	scene->entities =
		(SceneEntity*)Arena_Alloc(sizeof(SceneEntity) * header->entity_count);
	for (int i = 0; i < header->entity_count; i++)
	{
		const PscEntityRec* rec = &entity_table[i];
		SceneEntity* ent = &scene->entities[i];

		SVECTOR rot = rec->rot;
		MATRIX local;
		RotMatrix(&rot, &local);
		memcpy(ent->light_rot.m, local.m, sizeof(local.m));

		/* Scale is applied to vertices first: scale the columns. */
		const int16_t scale[3] = { rec->scale.vx, rec->scale.vy, rec->scale.vz };
		for (int r = 0; r < 3; r++)
			for (int c = 0; c < 3; c++)
				local.m[r][c] = (int16_t)(((int32_t)local.m[r][c] * scale[c]) >> 12);

		ent->model = (rec->model == PSC_NO_INDEX) ? -1 : (int16_t)rec->model;

		int32_t pos[3] = { rec->pos.vx, rec->pos.vy, rec->pos.vz };

		if (rec->parent == PSC_NO_INDEX)
		{
			memcpy(ent->world.m, local.m, sizeof(local.m));
			ent->world.t[0] = pos[0];
			ent->world.t[1] = pos[1];
			ent->world.t[2] = pos[2];
		}
		else
		{
			const SceneEntity* parent = &scene->entities[rec->parent];
			MulMat3(&parent->world, &local, &ent->world);
			MulMat3(&parent->light_rot, &ent->light_rot, &ent->light_rot);

			int32_t world_pos[3];
			MulMatVec(&parent->world, pos, world_pos);
			ent->world.t[0] = world_pos[0] + parent->world.t[0];
			ent->world.t[1] = world_pos[1] + parent->world.t[1];
			ent->world.t[2] = world_pos[2] + parent->world.t[2];
		}
	}

	/* Scene lighting: one directional light + ambient + background. */
	memset(&scene->light_mtx, 0, sizeof(MATRIX));
	scene->light_mtx.m[0][0] = header->light_toward[0];
	scene->light_mtx.m[0][1] = header->light_toward[1];
	scene->light_mtx.m[0][2] = header->light_toward[2];

	MATRIX color_mtx = {{{0}}};
	for (int c = 0; c < 3; c++)
		color_mtx.m[c][0] = (int16_t)(((int32_t)header->light_color[c] << 12) / 255);
	gte_SetColorMatrix(&color_mtx);
	gte_SetBackColor(header->ambient[0], header->ambient[1], header->ambient[2]);

	scene->background.r = header->background[0];
	scene->background.g = header->background[1];
	scene->background.b = header->background[2];

	return 0;
}

/* Drawing ----------------------------------------------------------------- */

int Scene_TriangleCount(const Scene* scene)
{
	int total = 0;
	for (int i = 0; i < scene->entity_count; i++)
	{
		if (scene->entities[i].model >= 0)
			total += Pmd_TriangleCount(&scene->models[scene->entities[i].model]);
	}
	return total;
}

uint8_t* Scene_Draw(const Scene* scene, const MATRIX* view, uint32_t* ot,
	int ot_length, uint8_t* packet, uint8_t* packet_limit)
{
	/* Reverse order: within an equal OT bucket, primitives added LAST are
	 * drawn FIRST (addPrim prepends). Iterating in reverse means entities
	 * listed first in the scene (typically the ground) end up underneath
	 * when average-Z ties occur. */
	for (int i = scene->entity_count - 1; i >= 0; i--)
	{
		const SceneEntity* ent = &scene->entities[i];
		if (ent->model < 0)
			continue;

		/* Compose camera * world in plain C: no GTE register ordering
		 * hazards, and only a handful of entities per scene for now. */
		MATRIX comp;
		MulMat3(view, &ent->world, &comp);

		int32_t world_t[3] = { ent->world.t[0], ent->world.t[1], ent->world.t[2] };
		int32_t view_t[3];
		MulMatVec(view, world_t, view_t);
		comp.t[0] = view_t[0] + view->t[0];
		comp.t[1] = view_t[1] + view->t[1];
		comp.t[2] = view_t[2] + view->t[2];

		/* Lighting: world light direction against the entity's rotation
		 * (unscaled, so non-uniform scale doesn't skew intensities). */
		MATRIX light;
		MulMat3(&scene->light_mtx, &ent->light_rot, &light);

		gte_SetRotMatrix(&comp);
		gte_SetTransMatrix(&comp);
		gte_SetLightMatrix(&light);

		packet = Pmd_Draw(&scene->models[ent->model], ot, ot_length,
			packet, packet_limit);
	}

	return packet;
}
