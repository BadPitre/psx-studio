/*
 * PSX Studio - UI runtime (SceneFormat v1.3, docs/UI-SYSTEM.md)
 *
 * Philosophie uGUI : les widgets sont des entites de la scene ; chaque
 * enregistrement UI porte un RectTransform (ancres/pivot en 4.12 du rect
 * parent) et des composants (canvas, image, text...). Les rects sont
 * resolus en une passe descendante (les parents viennent toujours avant
 * dans le fichier), puis dessines en primitives 2D inserees en OT[0] —
 * au-dessus de la 3D.
 *
 * Jalon 1 : canvas, image (aplat / sprite Simple / Filled), texte.
 * Les types Sliced/Tiled et les Layout Groups arrivent au jalon 2.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include <string.h>
#include <psxgpu.h>

#include "engine.h"
#include "scene.h"

#define SCREEN_W	320
#define SCREEN_H	240

typedef struct
{
	int16_t	x, y, w, h;
} UiRect;

/* Rects resolus + visibilite de la frame courante. */
static UiRect	resolved[SCENE_MAX_UI];
static uint8_t	visible[SCENE_MAX_UI];
/* Curseur d'empilement par Layout Group (offset sur l'axe principal). */
static int16_t	layout_cursor[SCENE_MAX_UI];
/* Etat mutable par widget (setters) : amount/actif/teinte vivent dans
 * les enregistrements de l'arene, directement (memoire ordinaire). */

static const PscUiRec* find_rec(const Scene* scene, int entity, int* index_out)
{
	for (int i = 0; i < scene->ui_count; i++)
	{
		if (scene->ui[i].entity == entity)
		{
			if (index_out)
				*index_out = i;
			return &scene->ui[i];
		}
	}
	return 0;
}

const PscUiRec* Ui_Get(const Scene* scene, const Entity* e)
{
	return find_rec(scene, (int)(e - scene->entities), 0);
}

/* Les setters ecrivent dans l'arene (les enregistrements sont mutables
 * au meme titre que les transforms d'entites). */
void Ui_SetFill(const Scene* scene, const Entity* e, int amount_412)
{
	PscUiRec* rec = (PscUiRec*)Ui_Get(scene, e);
	if (!rec)
		return;
	if (amount_412 < 0)
		amount_412 = 0;
	if (amount_412 > 4096)
		amount_412 = 4096;
	rec->data = (uint16_t)amount_412;
}

void Ui_SetActive(const Scene* scene, const Entity* e, int active)
{
	PscUiRec* rec = (PscUiRec*)Ui_Get(scene, e);
	if (!rec)
		return;
	if (active)
		rec->components |= UI_COMP_ACTIVE;
	else
		rec->components &= ~UI_COMP_ACTIVE;
}

void Ui_SetTint(const Scene* scene, const Entity* e, uint8_t r, uint8_t g,
	uint8_t b)
{
	PscUiRec* rec = (PscUiRec*)Ui_Get(scene, e);
	if (!rec)
		return;
	rec->color[0] = r;
	rec->color[1] = g;
	rec->color[2] = b;
}

/* Resolution d'un axe du RectTransform (semantique Unity) : ancres 4.12
 * du rect parent ; posees (min == max) -> pos = offset du pivot, taille
 * propre ; etirees -> pos/size = marges. */
static void resolve_axis(int p_start, int p_len, int a_min, int a_max,
	int pivot, int pos, int size, int16_t* out_start, int16_t* out_len)
{
	int lo = p_start + ((p_len * a_min) >> 12);
	int hi = p_start + ((p_len * a_max) >> 12);

	if (a_min == a_max)
	{
		*out_len = (int16_t)size;
		*out_start = (int16_t)(lo + pos - ((size * pivot) >> 12));
	}
	else
	{
		*out_start = (int16_t)(lo + pos);
		*out_len = (int16_t)(hi - size - *out_start);
	}
}

static int text_width(const UiFont* font, const char* s)
{
	int w = 0;
	for (; *s; s++)
	{
		int g = (uint8_t)*s - font->first;
		w += (g >= 0 && g < font->count) ? font->advances[g] : font->cell_w / 2;
	}
	return w;
}

uint8_t* Ui_Draw(const Scene* scene, uint32_t* ot, uint8_t* packet,
	uint8_t* packet_limit)
{
	int n = scene->ui_count;
	if (n <= 0)
		return packet;

	/* Passe 1 : rects + visibilite (parents d'abord, ordre du fichier).
	 * Un parent Layout Group ECRASE le rect de ses enfants : empilement
	 * sur son axe (padding + spacing), alignement sur l'axe croise —
	 * comme un Vertical/Horizontal Layout Group Unity. */
	for (int i = 0; i < n; i++)
	{
		const PscUiRec* rec = &scene->ui[i];
		UiRect parent = { 0, 0, SCREEN_W, SCREEN_H };
		uint8_t parent_visible = 1;
		const PscUiRec* parent_rec = 0;
		int pi = -1;
		int16_t parent_entity = scene->entities[rec->entity].parent;

		layout_cursor[i] = 0;
		if (parent_entity >= 0 && find_rec(scene, parent_entity, &pi) && pi < i)
		{
			parent = resolved[pi];
			parent_visible = visible[pi];
			parent_rec = &scene->ui[pi];
		}
		visible[i] = parent_visible && (rec->components & UI_COMP_ACTIVE);

		if (parent_rec && (parent_rec->components & UI_COMP_LAYOUT))
		{
			/* Zone de contenu = parent moins padding (uv du layout). */
			int cx = parent.x + parent_rec->uv[0];
			int cy = parent.y + parent_rec->uv[1];
			int cw = parent.w - parent_rec->uv[0] - parent_rec->uv[2];
			int ch = parent.h - parent_rec->uv[1] - parent_rec->uv[3];
			int horizontal = parent_rec->flags & (1 << 4);
			int w = (parent_rec->flags & (1 << 5)) && !horizontal
				? cw : rec->size[0];
			int h = (parent_rec->flags & (1 << 6)) && horizontal
				? ch : rec->size[1];
			int align = parent_rec->border[0];

			if (horizontal)
			{
				resolved[i].x = (int16_t)(cx + layout_cursor[pi]);
				/* Alignement vertical : ligne de la grille 3x3. */
				resolved[i].y = (int16_t)(cy + ((align / 3) * (ch - h)) / 2);
				layout_cursor[pi] += w + (int16_t)parent_rec->extra;
			}
			else
			{
				resolved[i].y = (int16_t)(cy + layout_cursor[pi]);
				/* Alignement horizontal : colonne de la grille 3x3. */
				resolved[i].x = (int16_t)(cx + ((align % 3) * (cw - w)) / 2);
				layout_cursor[pi] += h + (int16_t)parent_rec->extra;
			}
			resolved[i].w = (int16_t)w;
			resolved[i].h = (int16_t)h;
			continue;
		}

		resolve_axis(parent.x, parent.w, rec->anchor_min[0], rec->anchor_max[0],
			rec->pivot[0], rec->pos[0], rec->size[0],
			&resolved[i].x, &resolved[i].w);
		resolve_axis(parent.y, parent.h, rec->anchor_min[1], rec->anchor_max[1],
			rec->pivot[1], rec->pos[1], rec->size[1],
			&resolved[i].y, &resolved[i].h);
	}

	/* Passe 2 : primitives. addPrim insere en tete de liste (dessine en
	 * premier) : on parcourt a l'ENVERS pour que l'ordre du fichier reste
	 * l'ordre de dessin (le dernier au-dessus), et le DR_TPAGE d'un
	 * widget texte est ajoute APRES ses SPRT pour etre execute avant. */
	for (int i = n - 1; i >= 0; i--)
	{
		const PscUiRec* rec = &scene->ui[i];
		UiRect r = resolved[i];

		if (!visible[i] || r.w <= 0 || r.h <= 0)
			continue;

		if (rec->components & UI_COMP_IMAGE)
		{
			int w = r.w;
			int h = r.h;
			/* Image Filled : le rect est tronque par amount (4.12). */
			if ((rec->flags & 0x3) == 3)
			{
				if (rec->flags & (1 << 2))
					h = (r.h * rec->data) >> 12;
				else
					w = (r.w * rec->data) >> 12;
			}
			if (w <= 0 || h <= 0)
				continue;

			if (rec->asset == 0xFF)
			{
				/* Aplat colore (Panel). */
				if (packet + sizeof(TILE) > packet_limit)
					break;
				TILE* tile = (TILE*)packet;
				setTile(tile);
				setXY0(tile, r.x, r.y);
				setWH(tile, w, h);
				setRGB0(tile, rec->color[0], rec->color[1], rec->color[2]);
				if (rec->flags & (1 << 3))
					setSemiTrans(tile, 1);
				addPrim(&ot[0], tile);
				packet += sizeof(TILE);
			}
			else if ((rec->flags & 0x3) == 1)
			{
				/* Sliced (9-slice) : coins intacts, bords/centre etires.
				 * POLY_FT4 partout (il porte son tpage, uv affines). */
				int sw = rec->uv[2] ? rec->uv[2] : 255;
				int sh = rec->uv[3] ? rec->uv[3] : 255;
				int bl = rec->border[0], bt = rec->border[1];
				int br = rec->border[2], bb = rec->border[3];
				int xs[4] = { r.x, r.x + bl, r.x + w - br, r.x + w };
				int ys[4] = { r.y, r.y + bt, r.y + h - bb, r.y + h };
				int us[4] = { rec->uv[0], rec->uv[0] + bl,
					rec->uv[0] + sw - br, rec->uv[0] + sw };
				int vs[4] = { rec->uv[1], rec->uv[1] + bt,
					rec->uv[1] + sh - bb, rec->uv[1] + sh };
				for (int cy = 0; cy < 3; cy++)
				for (int cx = 0; cx < 3; cx++)
				{
					int pw = xs[cx + 1] - xs[cx];
					int ph = ys[cy + 1] - ys[cy];
					if (pw <= 0 || ph <= 0)
						continue;
					if (packet + sizeof(POLY_FT4) > packet_limit)
						break;
					POLY_FT4* q = (POLY_FT4*)packet;
					setPolyFT4(q);
					setXYWH(q, xs[cx], ys[cy], pw, ph);
					setUVWH(q, us[cx], vs[cy],
						us[cx + 1] - us[cx] - 1, vs[cy + 1] - vs[cy] - 1);
					q->tpage = scene->tex_tpage[rec->asset];
					q->clut = scene->tex_clut[rec->asset];
					setRGB0(q, rec->color[0], rec->color[1], rec->color[2]);
					if (rec->flags & (1 << 3))
						setSemiTrans(q, 1);
					addPrim(&ot[0], q);
					packet += sizeof(POLY_FT4);
				}
			}
			else if ((rec->flags & 0x3) == 2)
			{
				/* Tiled : le sprite repete en grille de SPRT (tres
				 * economes), bords tronques. */
				int sw = rec->uv[2] ? rec->uv[2] : 16;
				int sh = rec->uv[3] ? rec->uv[3] : 16;
				for (int ty = 0; ty < h; ty += sh)
				for (int tx = 0; tx < w; tx += sw)
				{
					if (packet + sizeof(SPRT) > packet_limit)
						break;
					SPRT* spr = (SPRT*)packet;
					setSprt(spr);
					setXY0(spr, r.x + tx, r.y + ty);
					setWH(spr, tx + sw > w ? w - tx : sw,
						ty + sh > h ? h - ty : sh);
					setUV0(spr, rec->uv[0], rec->uv[1]);
					spr->clut = scene->tex_clut[rec->asset];
					setRGB0(spr, rec->color[0], rec->color[1], rec->color[2]);
					if (rec->flags & (1 << 3))
						setSemiTrans(spr, 1);
					addPrim(&ot[0], spr);
					packet += sizeof(SPRT);
				}
				if (packet + sizeof(DR_TPAGE) <= packet_limit)
				{
					DR_TPAGE* tp = (DR_TPAGE*)packet;
					setDrawTPage(tp, 0, 1, scene->tex_tpage[rec->asset]);
					addPrim(&ot[0], tp);
					packet += sizeof(DR_TPAGE);
				}
			}
			else
			{
				/* Sprite Simple/Filled : un SPRT. */
				if (packet + sizeof(SPRT) + sizeof(DR_TPAGE) > packet_limit)
					break;
				SPRT* spr = (SPRT*)packet;
				setSprt(spr);
				setXY0(spr, r.x, r.y);
				setWH(spr, w, h);
				setUV0(spr, rec->uv[0], rec->uv[1]);
				spr->clut = scene->tex_clut[rec->asset];
				setRGB0(spr, rec->color[0], rec->color[1], rec->color[2]);
				if (rec->flags & (1 << 3))
					setSemiTrans(spr, 1);
				addPrim(&ot[0], spr);
				packet += sizeof(SPRT);

				DR_TPAGE* tp = (DR_TPAGE*)packet;
				setDrawTPage(tp, 0, 1, scene->tex_tpage[rec->asset]);
				addPrim(&ot[0], tp);
				packet += sizeof(DR_TPAGE);
			}
		}

		if ((rec->components & UI_COMP_TEXT) && rec->asset < scene->font_count)
		{
			const UiFont* font = &scene->fonts[rec->asset];
			const char* s = scene->ui_strings + rec->data;
			int x = r.x;
			int y = r.y;

			if (rec->extra == 1)
				x = r.x + (r.w - text_width(font, s)) / 2;
			else if (rec->extra == 2)
				x = r.x + r.w - text_width(font, s);

			for (; *s; s++)
			{
				int g = (uint8_t)*s - font->first;
				if (g < 0 || g >= font->count)
				{
					x += font->cell_w / 2;
					continue;
				}
				if (packet + sizeof(SPRT) + sizeof(DR_TPAGE) > packet_limit)
					break;
				SPRT* spr = (SPRT*)packet;
				setSprt(spr);
				setXY0(spr, x, y);
				setWH(spr, font->cell_w, font->cell_h);
				setUV0(spr,
					(uint8_t)(font->u0 + (g % 16) * font->cell_w),
					(uint8_t)(font->v0 + (g / 16) * font->cell_h));
				spr->clut = font->clut;
				setRGB0(spr, rec->color[0], rec->color[1], rec->color[2]);
				addPrim(&ot[0], spr);
				packet += sizeof(SPRT);
				x += font->advances[g];
			}

			if (packet + sizeof(DR_TPAGE) <= packet_limit)
			{
				DR_TPAGE* tp = (DR_TPAGE*)packet;
				setDrawTPage(tp, 0, 1, font->tpage);
				addPrim(&ot[0], tp);
				packet += sizeof(DR_TPAGE);
			}
		}
	}

	return packet;
}
