/*
 * Script "hud" (attache au canvas HUD) : branche l'UI au gameplay — la
 * jauge Filled du canvas suit la sante du joueur. C'est le modele des
 * scripts UI : retrouver ses widgets par la hierarchie, puis les piloter
 * chaque frame par l'API Ui_* (docs/UI-SYSTEM.md §6).
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include "engine.h"

/* Sante 4.12 (4096 = pleine), entretenue par le script player. */
extern int g_player_health;

static Entity* jauge;

static int IsDescendantOf(const Entity* e, int root)
{
	int p = e->parent;
	while (p >= 0 && p != root)
		p = Scene_GetEntity(p)->parent;
	return p == root;
}

void Hud_Start(Entity* self)
{
	int root = (int)(self - Scene_GetEntity(0));

	jauge = 0;
	for (int i = 0; i < Scene_EntityCount(); i++)
	{
		Entity* e = Scene_GetEntity(i);
		if (IsDescendantOf(e, root) && Ui_ImageType(e) == 3)
		{
			jauge = e;
			return;
		}
	}
}

void Hud_Update(Entity* self)
{
	(void)self;
	if (jauge)
		Ui_SetFill(jauge, g_player_health);
}
