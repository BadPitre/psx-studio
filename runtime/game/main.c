/*
 * PSX Studio - Phase 5 deliverable
 * Boucle de jeu generique : scene + scripts + dialogue + camera script.
 * La demo "village" : marcher (D-pad), collisions AABB, parler au PNJ (X).
 *
 * Copyright (c) 2026 Bertrand - MIT License
 *
 * Rendering skeleton shared with the previous phases, portions adapted
 * from the PSn00bSDK template and examples (MPL, Lameguy64/spicyjpeg).
 */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <psxapi.h>
#include <psxcd.h>
#include <psxgpu.h>
#include <psxgte.h>
#include <psxpad.h>
#include <inline_c.h>

#include "engine.h"
#include "gameapi.h"

/* Menu pause ouvert (script pause) : gele aussi les Character Controllers. */
extern int g_ui_pause_open;
#include "scene.h"
#include "sfx.h"

#define OT_LENGTH		1024
#define BUFFER_LENGTH	32768

#define SCREEN_XRES		320
#define SCREEN_YRES		240
#define CENTER_X		(SCREEN_XRES >> 1)
#define CENTER_Y		(SCREEN_YRES >> 1)

/* SFX partage avec les scripts (blip du dialogue). */
SfxSample g_sfx_blip;

/* Double buffering ------------------------------------------------------- */

typedef struct
{
	DISPENV		disp_env;
	DRAWENV		draw_env;
	uint32_t	ot[OT_LENGTH];
	uint8_t		buffer[BUFFER_LENGTH];
} RenderBuffer;

typedef struct
{
	RenderBuffer	buffers[2];
	uint8_t*		next_packet;
	int				active_buffer;
} RenderContext;

static void SetupContext(RenderContext* ctx)
{
	SetDefDispEnv(&ctx->buffers[0].disp_env, 0, 0, SCREEN_XRES, SCREEN_YRES);
	SetDefDrawEnv(&ctx->buffers[0].draw_env, 0, SCREEN_YRES, SCREEN_XRES, SCREEN_YRES);
	SetDefDispEnv(&ctx->buffers[1].disp_env, 0, SCREEN_YRES, SCREEN_XRES, SCREEN_YRES);
	SetDefDrawEnv(&ctx->buffers[1].draw_env, 0, 0, SCREEN_XRES, SCREEN_YRES);

	for (int i = 0; i < 2; i++)
	{
		ctx->buffers[i].draw_env.isbg = 1;
		ctx->buffers[i].draw_env.dtd = 1;
	}

	ctx->active_buffer = 0;
	ctx->next_packet = ctx->buffers[0].buffer;
	ClearOTagR(ctx->buffers[0].ot, OT_LENGTH);
	PutDrawEnv(&ctx->buffers[0].draw_env);

	SetDispMask(1);
}

static void SetBackground(RenderContext* ctx, const CVECTOR* color)
{
	for (int i = 0; i < 2; i++)
		setRGB0(&ctx->buffers[i].draw_env, color->r, color->g, color->b);
}

static void FlipBuffers(RenderContext* ctx)
{
	DrawSync(0);
	VSync(0);

	RenderBuffer* draw_buffer = &ctx->buffers[ctx->active_buffer];
	RenderBuffer* disp_buffer = &ctx->buffers[ctx->active_buffer ^ 1];

	PutDispEnv(&disp_buffer->disp_env);
	DrawOTagEnv(&draw_buffer->ot[OT_LENGTH - 1], &draw_buffer->draw_env);

	ctx->active_buffer ^= 1;
	ctx->next_packet = disp_buffer->buffer;
	ClearOTagR(disp_buffer->ot, OT_LENGTH);
}

/* Main -------------------------------------------------------------------- */

int main(int argc, const char** argv)
{
	RenderContext ctx;
	Scene scene;

	ResetGraph(0);
	FntLoad(960, 0);
	SetupContext(&ctx);
	Input_Init();

	InitGeom();
	gte_SetGeomOffset(CENTER_X, CENTER_Y);
	gte_SetGeomScreen(CENTER_X);

	CdInit();
	Sfx_Init();

	/* SFX du dialogue, uploade avant que la scene ne reutilise l'arene. */
	{
		void* vag = Scene_ReadFileToArena("\\BLIP.VAG;1", 0);
		if (vag)
			g_sfx_blip = Sfx_UploadVag(vag);
	}

	static const char* const SCENE_PATHS[] = {
		"\\SCENE0.PSC;1",
		"\\SCENE1.PSC;1",
	};
	int scene_index = 0;

	int err = Scene_LoadFromCd(&scene, SCENE_PATHS[scene_index]);
	assert(err == 0);
	SetBackground(&ctx, &scene.background);
	/* Entite camera de la scene = vue initiale ; les scripts (camera de
	 * suivi du player) reprennent la main ensuite. */
	Scene_ApplyCamera();
	Scene_StartScripts(&scene);
	Vm_StartScripts(&scene);

	/* Streaming : la scene suivante se precharge pendant qu'on joue. */
	Scene_Preload(SCENE_PATHS[scene_index ^ 1]);

	for (;;)
	{
		Input_Update();
		Scene_UpdateScripts(&scene);
		/* Character Controllers (composant sans code) : apres les
		 * scripts, geles quand le menu pause est ouvert. */
		Controller_Tick(g_ui_pause_open);
		/* Scripts PSX Script (bytecode de la scene, VM du moteur). */
		Vm_Tick(&scene, g_ui_pause_open);

		/* Bascule demandee par un script (portail) : si la scene
		 * suivante est prete, changement INSTANTANE — le parse est
		 * local, aucune lecture CD, aucun ecran de chargement. */
		if (g_scene_switch_request)
		{
			int ready = Scene_PreloadReady();
			if (ready == 1)
			{
				g_scene_switch_request = 0;
				DrawSync(0);
				err = Scene_ActivatePreloaded(&scene);
				assert(err == 0);
				SetBackground(&ctx, &scene.background);
				Scene_ApplyCamera();
				Scene_StartScripts(&scene);
				Vm_StartScripts(&scene);
				scene_index ^= 1;
				Scene_Preload(SCENE_PATHS[scene_index ^ 1]);
			}
			else if (ready < 0)
			{
				g_scene_switch_request = 0;
			}
		}

		Scene_UpdateWorld(&scene);
		/* Camera reclamee par un script PSX Script : maintenant que les
		 * matrices monde sont a jour. */
		Camera_ApplyRequest();

		MATRIX view;
		Camera_GetViewMatrix(&view);

		RenderBuffer* draw_buffer = &ctx.buffers[ctx.active_buffer];
		ctx.next_packet = Scene_Draw(&scene, &view, draw_buffer->ot, OT_LENGTH,
			ctx.next_packet, &draw_buffer->buffer[BUFFER_LENGTH]);

		ctx.next_packet = Dialog_Draw(draw_buffer->ot, ctx.next_packet);
		ctx.next_packet = (uint8_t*)FntSort(&draw_buffer->ot[0],
			ctx.next_packet, 8, 8, "PSX STUDIO - PHASE 5");

		FlipBuffers(&ctx);
	}

	return 0;
}
