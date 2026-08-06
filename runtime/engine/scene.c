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
_Static_assert(offsetof(PscHeader, ui_count) == 18, "v1.3 layout");
_Static_assert(offsetof(PscHeader, font_count) == 62, "v1.3 layout");
_Static_assert(sizeof(PscUiRec) == 40, "v1.3 UI record");

/* Scene arena ------------------------------------------------------------- */
/* All per-scene data (the raw .psc file + runtime entity array) lives here.
 * Loading a scene resets the arena: no generic allocator, no leaks.
 *
 * Streaming (lot B) : DEUX arenes en bascule. La scene active vit dans
 * l'une pendant que la suivante se precharge en asynchrone dans l'autre
 * (CdRead ne bloque pas ; on poll CdReadSync(1) chaque frame). Le
 * changement de scene devient un simple parse local, sans lecture CD. */

#define ARENA_SIZE (512 * 1024)

static uint8_t	arenas[2][ARENA_SIZE] __attribute__((aligned(2048)));
static size_t	arena_used;
static int		active_arena;

/* Etat du prechargement dans l'arene inactive. */
static struct {
	int			busy;		/* lecture CD en cours ou terminee */
	int			done;
	uint32_t	size;
} preload;

static Scene*	current_scene;

/* Distance d'affichage courante (0 = illimitee), posee par la camera de
 * scene via Scene_SetDrawDistance. */
static int32_t	draw_distance;

void Scene_SetDrawDistance(int32_t d)
{
	draw_distance = d;
}

EditorBeacon g_editor_beacon;

static void* Arena_Alloc(size_t size)
{
	size = (size + 3) & ~(size_t)3;
	assert(arena_used + size <= ARENA_SIZE);

	void* ptr = &arenas[active_arena][arena_used];
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

/* Parse + fixup d'un .psc deja present dans l'arene active (aucune
 * lecture CD : c'est la moitie instantanee du chargement). */
static int Scene_Parse(Scene* scene, uint8_t* data)
{
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
			scene->tex_clut[i] = clut[i] = getClut(tim.crect->x, tim.crect->y);
		}
		/* Bit 9 = dithering : le tpage d'un polygone texture remplace
		 * l'etat du DRAWENV, sans ce bit chaque prim texturee
		 * redesactiverait le dithering demande par dtd=1. */
		scene->tex_tpage[i] = tpage[i] =
			getTPage(tim.mode & 0x3, 0, tim.prect->x, tim.prect->y)
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
		/* v1.2 : FOV camera dans le pad position, distance d'affichage
		 * dans le pad rotation, rayon de torche dans le pad echelle. */
		ent->cam_fov = (uint16_t)rec->pos.pad;
		ent->cam_draw = (uint16_t)rec->rot.pad;
		ent->light_radius = (uint16_t)rec->scale.pad;
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
	scene->point_light_count = 0;
	if (header->lights_offset != 0)
	{
		const PscLightRec* light_table =
			(const PscLightRec*)(data + header->lights_offset);
		for (int i = 0; i < header->light_count; i++)
		{
			if (light_table[i].entity >= header->entity_count)
				continue;
			/* Intensite en pourcent (pad, 0 = 100) : la matrice couleur
			 * GTE est en 4.12, une lumiere peut depasser 100 %. */
			int intensity = light_table[i].pad ? light_table[i].pad : 100;
			uint16_t eflags = scene->entities[light_table[i].entity].flags;

			if (eflags & ENTITY_FLAG_LIGHT_POINT)
			{
				if (scene->point_light_count >= SCENE_MAX_POINT_LIGHTS)
					continue;
				int slot = scene->point_light_count++;
				scene->point_lights[slot].entity =
					(int16_t)light_table[i].entity;
				for (int c = 0; c < 3; c++)
					scene->point_lights[slot].color[c] = (int16_t)(
						(((int32_t)light_table[i].color[c] << 12) / 255
							* intensity) / 100);
			}
			else
			{
				if (scene->light_entity_count >= SCENE_MAX_ENTITY_LIGHTS)
					continue;
				int slot = scene->light_entity_count++;
				scene->light_entities[slot] = (int16_t)light_table[i].entity;
				for (int c = 0; c < 3; c++)
					color_mtx.m[c][1 + slot] = (int16_t)(
						(((int32_t)light_table[i].color[c] << 12) / 255
							* intensity) / 100);
			}
		}
	}
	/* UI (v1.3) : la table suit les lumieres (alignee 4), puis les
	 * polices puis les chaines. Les atlas de police sont uploades comme
	 * les textures. */
	Ui_Reset();
	scene->ui_count = 0;
	scene->font_count = 0;
	if (header->ui_count > 0 && header->lights_offset != 0)
	{
		uint32_t ui_off = (header->lights_offset
			+ header->light_count * sizeof(PscLightRec) + 3) & ~3u;
		uint32_t fonts_off = ui_off + header->ui_count * sizeof(PscUiRec);
		uint32_t strings_off = fonts_off + header->font_count * 8;
		scene->ui = (const PscUiRec*)(data + ui_off);
		scene->ui_count = header->ui_count <= SCENE_MAX_UI
			? header->ui_count : SCENE_MAX_UI;
		scene->ui_strings = (const char*)(data + strings_off);
		for (int i = 0; i < header->font_count && i < SCENE_MAX_FONTS; i++)
		{
			uint32_t off = *(const uint32_t*)(data + fonts_off + i * 8);
			const uint8_t* fnt = data + off;
			if (fnt[0] != 'F' || fnt[1] != 'N' || fnt[2] != 'T')
				continue;
			UiFont* font = &scene->fonts[scene->font_count++];
			font->cell_w = fnt[4];
			font->cell_h = fnt[5];
			font->first = fnt[6];
			font->count = fnt[7];
			font->advances = fnt + 8;
			font->clut = 0;
			uint32_t tim_off = (8u + fnt[7] + 3u) & ~3u;
			TIM_IMAGE tim;
			if (GetTimInfo((const uint32_t*)(fnt + tim_off), &tim) != 0)
			{
				scene->font_count--;
				continue;
			}
			LoadImage(tim.prect, tim.paddr);
			if (tim.mode & 0x8)
			{
				LoadImage(tim.crect, tim.caddr);
				font->clut = getClut(tim.crect->x, tim.crect->y);
			}
			font->tpage = getTPage(tim.mode & 0x3, 0, tim.prect->x, tim.prect->y);
			/* Texels par mot : 4 en 4bpp, 2 en 8bpp. */
			int shift = (tim.mode & 0x3) == 0 ? 2 : 1;
			font->u0 = (uint8_t)((tim.prect->x % 64) << shift);
			font->v0 = (uint8_t)(tim.prect->y % 256);
		}
	}
	DrawSync(0);

	scene->color_mtx = color_mtx;
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

int Scene_LoadFromCd(Scene* scene, const char* path)
{
	/* Invalider la balise pendant le rechargement. */
	g_editor_beacon.magic[0] = 0;

	/* Un seul laser : si un prechargement est en vol, le laisser finir
	 * avant de lancer une lecture bloquante (puis l'abandonner). */
	if (preload.busy && !preload.done)
		CdReadSync(0, 0);
	preload.busy = 0;
	preload.done = 0;

	arena_used = 0;
	uint8_t* data = (uint8_t*)Scene_ReadFileToArena(path, 0);
	if (!data)
		return -1;
	return Scene_Parse(scene, data);
}

/* Streaming (lot B) ------------------------------------------------------- */

int Scene_Preload(const char* path)
{
	if (preload.busy)
		return -1;

	CdlFILE file;
	if (!CdSearchFile(&file, path))
		return -1;
	size_t sectors = (file.size + 2047) / 2048;
	if (sectors * 2048 > ARENA_SIZE)
		return -2;

	CdControl(CdlSetloc, &file.pos, 0);
	CdRead((int)sectors, (uint32_t*)arenas[active_arena ^ 1], CdlModeSpeed);
	preload.busy = 1;
	preload.done = 0;
	preload.size = file.size;
	return 0;
}

int Scene_PreloadReady(void)
{
	if (!preload.busy)
		return -1;
	if (preload.done)
		return 1;
	int remaining = CdReadSync(1, 0);
	if (remaining < 0)
	{
		preload.busy = 0;
		return -1;
	}
	if (remaining == 0)
	{
		preload.done = 1;
		return 1;
	}
	return 0;
}

int Scene_ActivatePreloaded(Scene* scene)
{
	if (Scene_PreloadReady() != 1)
		return -1;
	preload.busy = 0;
	preload.done = 0;

	g_editor_beacon.magic[0] = 0;
	active_arena ^= 1;
	arena_used = ((preload.size + 2047) / 2048) * 2048;
	return Scene_Parse(scene, arenas[active_arena]);
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

		/* Distance d'affichage (camera v1.2) : culling par entite, avec
		 * une marge d'un demi-modele pour ne pas couper trop tot. */
		if (draw_distance > 0 && comp.t[2] - 300 > draw_distance)
			continue;

		/* Lighting: world light direction against the entity's rotation
		 * (unscaled, so non-uniform scale doesn't skew intensities). */
		MATRIX wl = scene->light_mtx;
		MATRIX cmx = scene->color_mtx;

		/* Torches : approximation d'epoque — chaque objet convertit les
		 * lumieres ponctuelles a portee en directionnelles locales sur
		 * les lignes GTE restantes (direction torche->objet, couleur
		 * attenuee par la distance). */
		int rows = 1 + scene->light_entity_count;
		for (int p = 0; p < scene->point_light_count && rows < 3; p++)
		{
			const Entity* le = &scene->entities[scene->point_lights[p].entity];
			int32_t radius = le->light_radius;
			if (!le->visible || radius <= 0 || le == ent)
				continue;

			int32_t d[3] = {
				le->world.t[0] - ent->world.t[0],
				le->world.t[1] - ent->world.t[1],
				le->world.t[2] - ent->world.t[2],
			};
			/* Distance approchee (octogonale) : max + (reste >> 1),
			 * assez precise pour une attenuation, sans racine carree. */
			int32_t ax = d[0] < 0 ? -d[0] : d[0];
			int32_t ay = d[1] < 0 ? -d[1] : d[1];
			int32_t az = d[2] < 0 ? -d[2] : d[2];
			int32_t mx = ax > ay ? (ax > az ? ax : az) : (ay > az ? ay : az);
			int32_t dist = mx + ((ax + ay + az - mx) >> 1);
			if (dist <= 0 || dist >= radius)
				continue;

			int32_t falloff = ((radius - dist) << 12) / radius;
			for (int c = 0; c < 3; c++)
			{
				/* Vers la source, normalise ~4.12. */
				wl.m[rows][c] = (int16_t)(d[c] * 4096 / dist);
				cmx.m[c][rows] = (int16_t)(
					((int32_t)scene->point_lights[p].color[c] * falloff) >> 12);
			}
			rows++;
		}

		MATRIX light;
		MulMat3(&wl, &ent->light_rot, &light);

		gte_SetRotMatrix(&comp);
		gte_SetTransMatrix(&comp);
		gte_SetLightMatrix(&light);
		gte_SetColorMatrix(&cmx);

		packet = Pmd_Draw(&scene->models[ent->model], ot, ot_length,
			packet, packet_limit);
	}

	/* Halos additifs des torches : un losange semi-transparent (mode
	 * B+F du GPU) projete a la position de chaque lumiere ponctuelle. */
	for (int p = 0; p < scene->point_light_count; p++)
	{
		const Entity* le = &scene->entities[scene->point_lights[p].entity];
		if (!le->visible || le->light_radius == 0)
			continue;
		if (packet + sizeof(DR_TPAGE) + sizeof(POLY_F4) > packet_limit)
			break;

		SVECTOR posw = {
			(int16_t)le->world.t[0],
			(int16_t)le->world.t[1],
			(int16_t)le->world.t[2],
			0,
		};
		gte_SetRotMatrix(view);
		gte_SetTransMatrix(view);
		gte_ldv0(&posw);
		gte_rtps();
		int32_t otz;
		gte_stotz(&otz);
		if (otz <= 2 || otz >= ot_length)
			continue;
		DVECTOR sxy;
		gte_stsxy0(&sxy);

		int32_t sz;
		gte_stsz(&sz);
		if (sz <= 0)
			continue;
		int32_t r = (le->light_radius * 40) / sz;
		if (r < 2)
			r = 2;
		if (r > 96)
			r = 96;

		/* Mode additif : le tpage de l'etat (abr = 1) porte le blending
		 * des primitives non texturees. */
		DR_TPAGE* tp = (DR_TPAGE*)packet;
		setDrawTPage(tp, 0, 1, getTPage(0, 1, 0, 0));
		addPrim(&ot[otz], tp);
		packet += sizeof(DR_TPAGE);

		int32_t hc[3];
		for (int c = 0; c < 3; c++)
		{
			hc[c] = scene->point_lights[p].color[c] >> 5;
			if (hc[c] > 255)
				hc[c] = 255;
		}
		POLY_F4* halo = (POLY_F4*)packet;
		setPolyF4(halo);
		setSemiTrans(halo, 1);
		setRGB0(halo, (uint8_t)hc[0], (uint8_t)hc[1], (uint8_t)hc[2]);
		setXY4(halo,
			sxy.vx, sxy.vy - r,
			sxy.vx - r, sxy.vy,
			sxy.vx + r, sxy.vy,
			sxy.vx, sxy.vy + r);
		addPrim(&ot[otz], halo);
		packet += sizeof(POLY_F4);
	}

	packet = Ui_Draw(scene, ot, packet, packet_limit);

	return packet;
}
