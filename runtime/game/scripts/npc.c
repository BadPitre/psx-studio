/*
 * Script "npc" : le villageois — leger flottement et pivote vers le
 * joueur quand il est proche.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include <psxgte.h>
#include "engine.h"

#define NOTICE_DIST 200

static Entity* player;
static int32_t base_y;
static int tick;

void Npc_Start(Entity* self)
{
	player = Scene_FindByScript("player");
	base_y = self->pos.vy;
}

void Npc_Update(Entity* self)
{
	/* Petit flottement idle (tres PS1). */
	tick = (tick + 40) & 4095;
	self->pos.vy = base_y + (isin(tick) >> 10);

	/* Se tourne face au joueur quand il approche (8 directions). */
	if (player && Entity_Dist2XZ(self, player) < NOTICE_DIST * NOTICE_DIST)
	{
		int32_t dx = player->pos.vx - self->pos.vx;
		int32_t dz = player->pos.vz - self->pos.vz;
		int yaw;
		if (dz > 0)
			yaw = (dx > dz / 2) ? 2560 : (dx < -dz / 2 ? 1536 : 2048);
		else
			yaw = (dx > -dz / 2) ? 3584 : (dx < dz / 2 ? 512 : 0);
		self->rot.vy = (int16_t)yaw;
	}
}
