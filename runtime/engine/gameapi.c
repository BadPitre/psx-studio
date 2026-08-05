/*
 * PSX Studio - API gameplay : input, camera script, dialogue, physique.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <psxapi.h>
#include <psxgpu.h>
#include <psxgte.h>
#include <psxpad.h>

#include "engine.h"
#include "gameapi.h"
#include "scene.h"

/* Input ------------------------------------------------------------------- */

static uint8_t	pad_buff[2][34];
static uint16_t	input_held;
static uint16_t	input_pressed;

void Input_Init(void)
{
	InitPAD(pad_buff[0], 34, pad_buff[1], 34);
	StartPAD();
	ChangeClearPAD(0);
}

void Input_Update(void)
{
	const PADTYPE* pad = (const PADTYPE*)pad_buff[0];
	uint16_t held = 0;

	if (pad->stat == 0 &&
		(pad->type == 0x4 || pad->type == 0x5 || pad->type == 0x7))
	{
		held = (uint16_t)~pad->btn;
	}
	input_pressed = held & ~input_held;
	input_held = held;
}

uint16_t Input_Held(void)
{
	return input_held;
}

uint16_t Input_Pressed(void)
{
	return input_pressed;
}

/* Camera ------------------------------------------------------------------ */

static VECTOR	cam_pos = { 0, -140, -420, 0 };
static int		cam_yaw = 0;
static int		cam_pitch = 170;

void Camera_Set(int32_t x, int32_t y, int32_t z, int yaw, int pitch)
{
	cam_pos.vx = x;
	cam_pos.vy = y;
	cam_pos.vz = z;
	cam_yaw = yaw & 4095;
	cam_pitch = pitch;
}

void Camera_GetViewMatrix(MATRIX* view)
{
	SVECTOR rot = { (int16_t)cam_pitch, (int16_t)cam_yaw, 0, 0 };
	RotMatrix(&rot, view);

	VECTOR neg = { -cam_pos.vx, -cam_pos.vy, -cam_pos.vz, 0 };
	VECTOR t;
	ApplyMatrixLV(view, &neg, &t);
	TransMatrix(view, &t);
}

/* Dialogue ---------------------------------------------------------------- */

#define DIALOG_MAX_LINES 3

static const char* dialog_text;

void Dialog_Show(const char* text)
{
	dialog_text = text;
}

int Dialog_IsOpen(void)
{
	return dialog_text != 0;
}

void Dialog_Close(void)
{
	dialog_text = 0;
}

uint8_t* Dialog_Draw(uint32_t* ot, uint8_t* packet)
{
	if (!dialog_text)
		return packet;

	/* Fond semi-transparent, au-dessus de la 3D (index 1, le texte de la
	 * police est a l'index 0 et se dessine par-dessus). */
	TILE* tile = (TILE*)packet;
	setTile(tile);
	setSemiTrans(tile, 1);
	setRGB0(tile, 12, 12, 32);
	setXY0(tile, 8, 168);
	setWH(tile, 304, 60);
	addPrim(&ot[1], tile);
	packet += sizeof(TILE);

	/* Jusqu'a 3 lignes separees par '\n' (FntSort ne gere pas les sauts). */
	char line[40];
	const char* src = dialog_text;
	int row = 0;
	while (*src && row < DIALOG_MAX_LINES)
	{
		int len = 0;
		while (src[len] && src[len] != '\n' && len < (int)sizeof(line) - 1)
			len++;
		memcpy(line, src, len);
		line[len] = 0;
		packet = (uint8_t*)FntSort(&ot[0], packet, 16, 176 + row * 14, line);
		src += len;
		if (*src == '\n')
			src++;
		row++;
	}
	packet = (uint8_t*)FntSort(&ot[0], packet, 232, 176 + DIALOG_MAX_LINES * 14,
		"[X] FERMER");
	return packet;
}

/* Physique ---------------------------------------------------------------- */

int32_t Entity_Dist2XZ(const Entity* a, const Entity* b)
{
	int32_t dx = a->world.t[0] - b->world.t[0];
	int32_t dz = a->world.t[2] - b->world.t[2];
	return dx * dx + dz * dz;
}

/* AABB monde (axes X/Z) d'une entite : bounds du modele x echelle propre,
 * centres sur la position monde. La rotation est ignoree (physique
 * volontairement simple, cf. doc projet). */
static void EntityBoxXZ(const Scene* scene, const Entity* ent,
	int32_t* min_x, int32_t* max_x, int32_t* min_z, int32_t* max_z)
{
	const SVECTOR* mn = &scene->model_min[ent->model];
	const SVECTOR* mx = &scene->model_max[ent->model];

	int32_t hx = (((int32_t)(mx->vx - mn->vx) * ent->scale.vx) >> 12) / 2;
	int32_t hz = (((int32_t)(mx->vz - mn->vz) * ent->scale.vz) >> 12) / 2;
	int32_t cx = ent->world.t[0] + (((int32_t)(mx->vx + mn->vx) * ent->scale.vx) >> 12) / 2;
	int32_t cz = ent->world.t[2] + (((int32_t)(mx->vz + mn->vz) * ent->scale.vz) >> 12) / 2;

	*min_x = cx - hx;
	*max_x = cx + hx;
	*min_z = cz - hz;
	*max_z = cz + hz;
}

static int CollidesAt(const Scene* scene, const Entity* self,
	int32_t x, int32_t z)
{
	int32_t s_min_x, s_max_x, s_min_z, s_max_z;
	EntityBoxXZ(scene, self, &s_min_x, &s_max_x, &s_min_z, &s_max_z);
	/* Deplacer la boite a la position candidate. */
	int32_t ox = x - self->world.t[0];
	int32_t oz = z - self->world.t[2];
	s_min_x += ox; s_max_x += ox;
	s_min_z += oz; s_max_z += oz;

	for (int i = 0; i < scene->entity_count; i++)
	{
		const Entity* other = &scene->entities[i];
		if (other == self || other->model < 0 || !other->solid || !other->visible)
			continue;
		/* Le sol (tres plat et tres large) ne bloque pas la marche. */
		const SVECTOR* mn = &scene->model_min[other->model];
		const SVECTOR* mx = &scene->model_max[other->model];
		if (mx->vy - mn->vy < 8)
			continue;

		int32_t o_min_x, o_max_x, o_min_z, o_max_z;
		EntityBoxXZ(scene, other, &o_min_x, &o_max_x, &o_min_z, &o_max_z);
		if (s_min_x < o_max_x && o_min_x < s_max_x &&
			s_min_z < o_max_z && o_min_z < s_max_z)
			return 1;
	}
	return 0;
}

void Physics_MoveAndSlide(Entity* e, int32_t dx, int32_t dz)
{
	Scene* scene = Scene_Current();
	if (!scene || e->model < 0)
	{
		e->pos.vx += dx;
		e->pos.vz += dz;
		return;
	}

	/* Axe par axe : bloque sur un mur, glisse le long. */
	if (dx != 0 && !CollidesAt(scene, e, e->pos.vx + dx, e->pos.vz))
		e->pos.vx += dx;
	if (dz != 0 && !CollidesAt(scene, e, e->pos.vx, e->pos.vz + dz))
		e->pos.vz += dz;
}
