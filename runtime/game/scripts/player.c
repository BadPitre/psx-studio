/*
 * Script "player" : deplacement au D-pad avec collisions, camera suiveuse,
 * interaction avec le PNJ (X pres de lui).
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include "engine.h"
#include "sfx.h"

extern SfxSample g_sfx_blip;

#define WALK_SPEED		5
#define TALK_DIST		130
#define CAM_BACK		340
#define CAM_UP			200

static Entity* npc;

/* Orientation du perso selon la direction de marche (8 directions).
 * Le modele fait face a -Z ; 4096 = un tour. */
static int FacingFor(int dx, int dz)
{
	if (dz > 0)
	{
		if (dx > 0) return 2048 + 512;
		if (dx < 0) return 2048 - 512;
		return 2048;
	}
	if (dz < 0)
	{
		if (dx > 0) return 4096 - 512;
		if (dx < 0) return 512;
		return 0;
	}
	if (dx > 0) return 3072;
	if (dx < 0) return 1024;
	return -1;
}

void Player_Start(Entity* self)
{
	npc = Scene_FindByScript("npc");
	(void)self;
}

void Player_Update(Entity* self)
{
	uint16_t held = Input_Held();
	uint16_t pressed = Input_Pressed();

	/* Dialogue ouvert : X ferme, et on ne bouge pas. */
	if (Dialog_IsOpen())
	{
		if (pressed & PAD_CROSS)
			Dialog_Close();
	}
	else
	{
		int dx = 0;
		int dz = 0;
		if (held & PAD_UP)    dz += WALK_SPEED;
		if (held & PAD_DOWN)  dz -= WALK_SPEED;
		if (held & PAD_LEFT)  dx -= WALK_SPEED;
		if (held & PAD_RIGHT) dx += WALK_SPEED;

		if (dx != 0 || dz != 0)
		{
			int facing = FacingFor(dx, dz);
			if (facing >= 0)
				self->rot.vy = (int16_t)facing;
			Physics_MoveAndSlide(self, dx, dz);
		}

		/* Parler au PNJ. */
		if (npc && (pressed & PAD_CROSS) &&
			Entity_Dist2XZ(self, npc) < TALK_DIST * TALK_DIST)
		{
			Dialog_Show("BONJOUR VOYAGEUR !\n"
				"BIENVENUE A CROUTON-SUR-MIE,\n"
				"LE PLUS BEAU VILLAGE DE LA PS1.");
			Sfx_Play(&g_sfx_blip);
		}
	}

	/* Portail : franchir le bord nord du terrain demande la bascule
	 * vers la scene prechargee (streaming, bord a ~660). */
	if (self->pos.vz > 590)
		g_scene_switch_request = 1;

	/* Camera suiveuse : derriere et au-dessus du perso (+Y = bas). */
	Camera_Set(self->pos.vx, self->pos.vy - CAM_UP, self->pos.vz - CAM_BACK,
		0, 240);
}
