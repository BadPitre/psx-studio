/*
 * Script "dialogue" (attache a un canvas UI inactif) : rend la boite de
 * dialogue en widgets de la scene — fond Sliced, lignes de texte — a la
 * place du fallback Dialog_Draw historique. Les trois premiers widgets
 * texte descendants du canvas recoivent les lignes via Ui_SetText.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include <string.h>
#include "engine.h"

#define DLG_LINES		3
#define DLG_LINE_MAX	40

static Entity*		line_widgets[DLG_LINES];
static char			line_bufs[DLG_LINES][DLG_LINE_MAX];
static const char*	shown;

static int IsDescendantOf(const Entity* e, int root)
{
	int p = e->parent;
	while (p >= 0 && p != root)
		p = Scene_GetEntity(p)->parent;
	return p == root;
}

void Dialogue_Start(Entity* self)
{
	int root = (int)(self - Scene_GetEntity(0));
	int count = 0;

	memset(line_widgets, 0, sizeof(line_widgets));
	shown = 0;
	for (int i = 0; i < Scene_EntityCount() && count < DLG_LINES; i++)
	{
		Entity* e = Scene_GetEntity(i);
		if (IsDescendantOf(e, root) && (Ui_Components(e) & UI_COMP_TEXT))
			line_widgets[count++] = e;
	}
	Dialog_UiCanvas(1);
	Ui_SetActive(self, 0);
}

void Dialogue_Update(Entity* self)
{
	const char* text = Dialog_Text();

	if (!text)
	{
		Ui_SetActive(self, 0);
		shown = 0;
		return;
	}
	Ui_SetActive(self, 1);
	if (text == shown)
		return;
	shown = text;

	/* Decoupe en lignes ('\n') vers les buffers du script ; les lignes
	 * manquantes sont videes (src reste sur le NUL final). */
	const char* src = text;
	for (int row = 0; row < DLG_LINES; row++)
	{
		int len = 0;
		while (src[len] && src[len] != '\n' && len < DLG_LINE_MAX - 1)
			len++;
		memcpy(line_bufs[row], src, len);
		line_bufs[row][len] = 0;
		if (line_widgets[row])
			Ui_SetText(line_widgets[row], line_bufs[row]);
		src += len;
		if (*src == '\n')
			src++;
	}
}
