/*
 * PSX Studio - SceneFormat v1 loader & gameplay core (Phases 2-5)
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
_Static_assert(sizeof(PscLightRec) == 6, "light record must be 6 bytes");
_Static_assert(offsetof(PscHeader, lights_offset) == 56, "v1.2 layout");

/* Scene arena ------------------------------------------------------------- */
/* All per-scene data (the raw .psc file + runtime entity array) lives here.
 * Loading a scene resets the arena: no generic allocator, no leaks. */

#define ARENA_SIZE (1024 * 1024)

static uint8_t	arena[ARENA_SIZE] __attribute__((aligned(2048)));
static size_t	arena_used;

static Scene*	current_scene;

EditorBeacon g_editor_beacon;

static void* Arena_Alloc(size_t size)
{
	size = (size + 3) & ~(size_t)3;
	assert(arena_used + size <= ARENA_SIZE);

	void* ptr = &arena[arena_used];
	arena_used += size;
	return ptr;
}

/* Fixed-point helpers ----------------------------------------------------- */

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

/* Scripts ----------------------------------------------------------------- */

uint32_t Script_Hash(const char* name)
{
	uint32_t h = 0x811c9dc5u;
	for (const char* p = name; *p; p++)
	{
		char c = *p;
		if (c >= 'A' && c <= 'Z')
			c += 'a' - 'A';
		h ^= (uint32_t)(uint8_t)c;
		h *= 0x01000193u;
	}
	return h;
}

static const ScriptDef* ResolveScript(uint32_t hash)
{
	for (int i = 0; i < g_script_count; i++)
	{
		if (Script_Hash(g_scripts[i].name) == hash)
			return &g_scripts[i];
	}
	return 0;
}

/* Loading ----------------------------------------------------------------- */

void* Scene_ReadFileToArena(const char* path, uint32_t* size_out)
{
	CdlFILE file;
	if (!CdSearchFile(&file, path))
		return 0;

	size_t sectors = (file.size + 2047) / 2048;
	uint8_t* data = (uint8_t*)Arena_Alloc(sectors * 2048);

	CdControl(CdlSetloc, &file.pos, 0);
	CdRead(sectors, (uint32_t*)data, CdlModeSpeed);
	if (CdReadSync(0, 0) < 0)
		return 0;

	if (size_out)
		*size_out = file.size;
	return data;
}

int Scene_LoadFromCd(Scene* scene, const char* path)
{
	/* Invalider la balise pendant le rechargement. */
	g_editor_beacon.magic[0] = 0;

	arena_used = 0;
	uint8_t* data = (uint8_t*)Scene_ReadFileToArena(path, 0);
	if (!data)
		return -1;

	const PscHeader* header = (const PscHeader*)data;
	if (memcmp(header->magic, "PSC1", 4) != 0)
		return -3;
	if (header->version != PSC_VERSION)
		return -4;
	if (header->model_count > SCENE_MAX_MODELS ||
		header->texture_count > SCENE_MAX_TEXTURES ||
		header->script_count > SCENE_MAX_SCRIPTS)
		return -5;

	scene->header = header;

	const PscModelEntry* model_table =
		(const PscModelEntry*)(data + header->models_offset);
	const PscTextureEntry* texture_table =
		(const PscTextureEntry*)(data + header->textures_offset);
	const PscEntityRec* entity_table =
		(const PscEntityRec*)(data + header->entities_offset);
	const uint32_t* script_table =
		(const uint32_t*)(data + header->scripts_offset);

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
		/* Bit 9 = dithering : le tpage d'un polygone texture remplace
		 * l'etat du DRAWENV, sans ce bit chaque prim texturee
		 * redesactiverait le dithering demande par dtd=1. */
		tpage[i] = getTPage(tim.mode & 0x3, 0, tim.prect->x, tim.prect->y)
			| (1 << 9);
	}
	DrawSync(0);

	/* Fix up every model in place + AABB pour la physique. */
	for (int i = 0; i < header->model_count; i++)
	{
		uint16_t tex = model_table[i].texture;
		int err = Pmd_Load(&scene->models[i], data + model_table[i].offset,
			tex != PSC_NO_INDEX ? tpage[tex] : 0,
			tex != PSC_NO_INDEX ? clut[tex] : 0);
		if (err != 0)
			return -7;

		SVECTOR mn = { 32767, 32767, 32767, 0 };
		SVECTOR mx = { -32768, -32768, -32768, 0 };
		const PmdModel* model = &scene->models[i];
		for (int v = 0; v < model->header->vertex_count; v++)
		{
			const SVECTOR* p = &model->verts[v];
			if (p->vx < mn.vx) mn.vx = p->vx;
			if (p->vy < mn.vy) mn.vy = p->vy;
			if (p->vz < mn.vz) mn.vz = p->vz;
			if (p->vx > mx.vx) mx.vx = p->vx;
			if (p->vy > mx.vy) mx.vy = p->vy;
			if (p->vz > mx.vz) mx.vz = p->vz;
		}
		scene->model_min[i] = mn;
		scene->model_max[i] = mx;
	}

	/* Resolution des scripts (hash -> registre compile avec le jeu). */
	for (int i = 0; i < header->script_count; i++)
		scene->scripts[i] = ResolveScript(script_table[i]);

	/* Entites : transforms locales mutables. */
	scene->entity_count = header->entity_count;
	scene->entities = (Entity*)Arena_Alloc(sizeof(Entity) * header->entity_count);
	for (int i = 0; i < header->entity_count; i++)
	{
		const PscEntityRec* rec = &entity_table[i];
		Entity* ent = &scene->entities[i];
		memset(ent, 0, sizeof(Entity));

		ent->pos.vx = rec->pos.vx;
		ent->pos.vy = rec->pos.vy;
		ent->pos.vz = rec->pos.vz;
		ent->rot = rec->rot;
		ent->scale = rec->scale;
		ent->model = (rec->model == PSC_NO_INDEX) ? -1 : (int16_t)rec->model;
		ent->parent = (rec->parent == PSC_NO_INDEX) ? -1 : (int16_t)rec->parent;
		ent->script = rec->script;
		ent->flags = rec->flags;
		ent->visible = 1;
		ent->solid = (ent->model >= 0);
	}

	/* Premiere entite camera : vue initiale de la scene. */
	scene->camera_entity = -1;
	for (int i = 0; i < scene->entity_count; i++)
	{
		if (scene->entities[i].flags & ENTITY_FLAG_CAMERA)
		{
			scene->camera_entity = (int16_t)i;
			break;
		}
	}

	/* Eclairage : ligne 0 = soleil des settings ; lignes 1-2 = entites-
	 * lumieres (v1.2), direction re-derivee chaque frame de leur rotation
	 * dans Scene_UpdateWorld. Les couleurs vont dans les colonnes de la
	 * matrice couleur GTE. */
	memset(&scene->light_mtx, 0, sizeof(MATRIX));
	scene->light_mtx.m[0][0] = header->light_toward[0];
	scene->light_mtx.m[0][1] = header->light_toward[1];
	scene->light_mtx.m[0][2] = header->light_toward[2];

	MATRIX color_mtx = {{{0}}};
	for (int c = 0; c < 3; c++)
		color_mtx.m[c][0] = (int16_t)(((int32_t)header->light_color[c] << 12) / 255);

	scene->light_entity_count = 0;
	if (header->lights_offset != 0)
	{
		const PscLightRec* light_table =
			(const PscLightRec*)(data + header->lights_offset);
		for (int i = 0; i < header->light_count &&
			scene->light_entity_count < SCENE_MAX_ENTITY_LIGHTS; i++)
		{
			if (light_table[i].entity >= header->entity_count)
				continue;
			int slot = scene->light_entity_count++;
			scene->light_entities[slot] = (int16_t)light_table[i].entity;
			for (int c = 0; c < 3; c++)
				color_mtx.m[c][1 + slot] =
					(int16_t)(((int32_t)light_table[i].color[c] << 12) / 255);
		}
	}
	gte_SetColorMatrix(&color_mtx);
	gte_SetBackColor(header->ambient[0], header->ambient[1], header->ambient[2]);

	scene->background.r = header->background[0];
	scene->background.g = header->background[1];
	scene->background.b = header->background[2];

	current_scene = scene;
	Scene_UpdateWorld(scene);

	/* Balise editeur (live tweaking) : etat coherent d'abord, magic en
	 * dernier — l'editeur scanne le dump RAM pour la trouver. Le magic
	 * complet ne doit exister nulle part ailleurs en RAM : on ne stocke
	 * que la queue en .rodata et le 'P' initial est ecrit tout a la fin. */
	g_editor_beacon.version = 1;
	g_editor_beacon.entity_size = sizeof(Entity);
	g_editor_beacon.entities_addr = (uint32_t)scene->entities;
	g_editor_beacon.entity_count = header->entity_count;
	g_editor_beacon.pos_offset = offsetof(Entity, pos);
	g_editor_beacon.rot_offset = offsetof(Entity, rot);
	g_editor_beacon.scale_offset = offsetof(Entity, scale);
	memcpy(g_editor_beacon.magic + 1, "SXSTUDIOBCN", 11);
	g_editor_beacon.magic[0] = 'P';

	return 0;
}

/* Per-frame updates ------------------------------------------------------- */

void Scene_UpdateWorld(Scene* scene)
{
	for (int i = 0; i < scene->entity_count; i++)
	{
		Entity* ent = &scene->entities[i];

		SVECTOR rot = ent->rot;
		MATRIX local;
		RotMatrix(&rot, &local);
		memcpy(ent->light_rot.m, local.m, sizeof(local.m));

		/* Scale is applied to vertices first: scale the columns. */
		const int16_t scale[3] = { ent->scale.vx, ent->scale.vy, ent->scale.vz };
		for (int r = 0; r < 3; r++)
			for (int c = 0; c < 3; c++)
				local.m[r][c] = (int16_t)(((int32_t)local.m[r][c] * scale[c]) >> 12);

		int32_t pos[3] = { ent->pos.vx, ent->pos.vy, ent->pos.vz };

		if (ent->parent < 0)
		{
			memcpy(ent->world.m, local.m, sizeof(local.m));
			ent->world.t[0] = pos[0];
			ent->world.t[1] = pos[1];
			ent->world.t[2] = pos[2];
		}
		else
		{
			const Entity* parent = &scene->entities[ent->parent];
			MulMat3(&parent->world, &local, &ent->world);
			MulMat3(&parent->light_rot, &ent->light_rot, &ent->light_rot);

			int32_t world_pos[3];
			MulMatVec(&parent->world, pos, world_pos);
			ent->world.t[0] = world_pos[0] + parent->world.t[0];
			ent->world.t[1] = world_pos[1] + parent->world.t[1];
			ent->world.t[2] = world_pos[2] + parent->world.t[2];
		}
	}

	/* Entites-lumieres : une lumiere eclaire le long de son axe -Z local,
	 * le GTE veut le vecteur VERS la source = +Z monde = 3e colonne de la
	 * rotation monde. Tourner l'entite (script, gizmo, live tweak) change
	 * donc l'eclairage en direct. */
	for (int l = 0; l < scene->light_entity_count; l++)
	{
		const Entity* e = &scene->entities[scene->light_entities[l]];
		scene->light_mtx.m[1 + l][0] = e->light_rot.m[0][2];
		scene->light_mtx.m[1 + l][1] = e->light_rot.m[1][2];
		scene->light_mtx.m[1 + l][2] = e->light_rot.m[2][2];
	}
}


void Scene_StartScripts(Scene* scene)
{
	for (int i = 0; i < scene->entity_count; i++)
	{
		Entity* ent = &scene->entities[i];
		if (ent->script == 0)
			continue;
		const ScriptDef* def = scene->scripts[ent->script - 1];
		if (def && def->on_start)
			def->on_start(ent);
	}
}

void Scene_UpdateScripts(Scene* scene)
{
	for (int i = 0; i < scene->entity_count; i++)
	{
		Entity* ent = &scene->entities[i];
		if (ent->script == 0)
			continue;
		const ScriptDef* def = scene->scripts[ent->script - 1];
		if (def && def->on_update)
			def->on_update(ent);
	}
}

/* API scripts ------------------------------------------------------------- */

Scene* Scene_Current(void)
{
	return current_scene;
}

int Scene_EntityCount(void)
{
	return current_scene ? current_scene->entity_count : 0;
}

Entity* Scene_GetEntity(int index)
{
	if (!current_scene || index < 0 || index >= current_scene->entity_count)
		return 0;
	return &current_scene->entities[index];
}

Entity* Scene_FindByScript(const char* name)
{
	if (!current_scene)
		return 0;
	uint32_t hash = Script_Hash(name);
	const PscHeader* header = current_scene->header;
	const uint32_t* table =
		(const uint32_t*)((const uint8_t*)header + header->scripts_offset);

	for (int i = 0; i < current_scene->entity_count; i++)
	{
		Entity* ent = &current_scene->entities[i];
		if (ent->script != 0 && table[ent->script - 1] == hash)
			return ent;
	}
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
		const Entity* ent = &scene->entities[i];
		if (ent->model < 0 || !ent->visible)
			continue;

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
