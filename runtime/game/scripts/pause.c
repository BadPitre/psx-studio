/*
 * Script "pause" (attache a un canvas UI inactif) : menu pause a boutons.
 * START ouvre/ferme, le D-pad deplace le focus (navigation geometrique du
 * moteur), X active le bouton focalise : REPRENDRE ferme, QUITTER demande
 * la bascule vers la scene prechargee (streaming). Le moteur surligne le
 * bouton focalise ; le jeu decide quoi faire de l'input — docs/UI-SYSTEM.md.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include "engine.h"

/* Lu par le script player : fige deplacement et interactions. */
int g_ui_pause_open;

#define PAUSE_MAX_BTN 4

static Entity*	btns[PAUSE_MAX_BTN];
static int		btn_count;

static int IsDescendantOf(const Entity* e, int root)
{
	int p = e->parent;
	while (p >= 0 && p != root)
		p = Scene_GetEntity(p)->parent;
	return p == root;
}

static void Close(Entity* self)
{
	g_ui_pause_open = 0;
	Ui_SetActive(self, 0);
	Ui_FocusClear();
}

void Pause_Start(Entity* self)
{
	int root = (int)(self - Scene_GetEntity(0));

	btn_count = 0;
	g_ui_pause_open = 0;
	for (int i = 0; i < Scene_EntityCount() && btn_count < PAUSE_MAX_BTN; i++)
	{
		Entity* e = Scene_GetEntity(i);
		if (IsDescendantOf(e, root) && (Ui_Components(e) & UI_COMP_BUTTON))
			btns[btn_count++] = e;
	}
	Ui_SetActive(self, 0);
}

void Pause_Update(Entity* self)
{
	uint16_t pressed = Input_Pressed();

	if (!g_ui_pause_open)
	{
		if ((pressed & PAD_START) && !Dialog_IsOpen())
		{
			g_ui_pause_open = 1;
			Ui_SetActive(self, 1);
			Ui_FocusInit();
		}
		return;
	}

	if (pressed & PAD_UP)
		Ui_FocusMove(0, -1);
	if (pressed & PAD_DOWN)
		Ui_FocusMove(0, 1);
	if (pressed & PAD_LEFT)
		Ui_FocusMove(-1, 0);
	if (pressed & PAD_RIGHT)
		Ui_FocusMove(1, 0);

	if (pressed & PAD_START)
	{
		Close(self);
		return;
	}
	if (pressed & PAD_CROSS)
	{
		Entity* focused = Ui_Focused();
		if (btn_count > 0 && focused == btns[0])
			Close(self);					/* REPRENDRE */
		else if (btn_count > 1 && focused == btns[1])
		{
			Close(self);					/* QUITTER : scene suivante */
			g_scene_switch_request = 1;
		}
	}
}
