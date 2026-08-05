/*
 * PSX Studio - Phase 1 deliverable
 * PMD model viewer: textured, lit mesh with an orbital pad camera.
 *
 * Controls:
 *   D-pad         orbit the camera (yaw/pitch)
 *   Cross         zoom in
 *   Triangle      zoom out
 *   Select        toggle auto-spin
 *   Start         reset the camera
 *
 * Copyright (c) 2026 Bertrand - MIT License
 *
 * Rendering skeleton shared with Phase 0 (double buffer + reversed OT),
 * portions adapted from the PSn00bSDK template and examples
 * (C) Lameguy64, spicyjpeg - MPL licensed.
 */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <psxapi.h>
#include <psxgpu.h>
#include <psxgte.h>
#include <psxpad.h>
#include <inline_c.h>

#include "pmd.h"

#define OT_LENGTH		1024
#define BUFFER_LENGTH	24576

#define SCREEN_XRES		320
#define SCREEN_YRES		240
#define CENTER_X		(SCREEN_XRES >> 1)
#define CENTER_Y		(SCREEN_YRES >> 1)

/* Embedded assets (see CMakeLists.txt, psn00bsdk_target_incbin). The PMD
 * blob lives in .data: Pmd_Load patches tpage/clut into it. */
extern uint32_t cube_pmd[];
extern const uint32_t checker_tim[];

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
		setRGB0(&ctx->buffers[i].draw_env, 16, 16, 48);
		ctx->buffers[i].draw_env.isbg = 1;
		ctx->buffers[i].draw_env.dtd = 1;
	}

	ctx->active_buffer = 0;
	ctx->next_packet = ctx->buffers[0].buffer;
	ClearOTagR(ctx->buffers[0].ot, OT_LENGTH);
	PutDrawEnv(&ctx->buffers[0].draw_env);

	SetDispMask(1);
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

/* Pad input --------------------------------------------------------------- */

static uint8_t pad_buff[2][34];

static void SetupPads(void)
{
	InitPAD(pad_buff[0], 34, pad_buff[1], 34);
	StartPAD();
	/* Don't make StartPAD spam VSync acknowledge. */
	ChangeClearPAD(0);
}

static uint16_t PadHeld(void)
{
	const PADTYPE* pad = (const PADTYPE*)pad_buff[0];

	if (pad->stat != 0)
		return 0;
	/* Digital pad, analog pad or dual shock. */
	if (pad->type != 0x4 && pad->type != 0x5 && pad->type != 0x7)
		return 0;
	return (uint16_t)~pad->btn;
}

/* Orbital camera ---------------------------------------------------------- */

#define CAM_YAW_SPEED	24		/* fixed angle units per frame (4096 = 360 deg) */
#define CAM_PITCH_MIN	-1024
#define CAM_PITCH_MAX	1024
#define CAM_DIST_MIN	300
#define CAM_DIST_MAX	1600
#define CAM_DIST_STEP	8
#define CAM_DIST_HOME	600

typedef struct
{
	int	yaw;
	int	pitch;
	int	dist;
	int	auto_spin;
} OrbitCamera;

static void ResetCamera(OrbitCamera* cam)
{
	cam->yaw = 0;
	cam->pitch = -256;		/* slightly above the model */
	cam->dist = CAM_DIST_HOME;
	cam->auto_spin = 1;
}

static void UpdateCamera(OrbitCamera* cam, uint16_t held, uint16_t pressed)
{
	if (held & PAD_LEFT)
		cam->yaw -= CAM_YAW_SPEED;
	if (held & PAD_RIGHT)
		cam->yaw += CAM_YAW_SPEED;
	if (held & PAD_UP)
		cam->pitch -= CAM_YAW_SPEED;
	if (held & PAD_DOWN)
		cam->pitch += CAM_YAW_SPEED;
	if (held & PAD_CROSS)
		cam->dist -= CAM_DIST_STEP;
	if (held & PAD_TRIANGLE)
		cam->dist += CAM_DIST_STEP;
	if (pressed & PAD_SELECT)
		cam->auto_spin ^= 1;
	if (pressed & PAD_START)
		ResetCamera(cam);

	if (cam->auto_spin && !(held & (PAD_LEFT | PAD_RIGHT)))
		cam->yaw += 8;

	cam->yaw &= 4095;
	if (cam->pitch < CAM_PITCH_MIN) cam->pitch = CAM_PITCH_MIN;
	if (cam->pitch > CAM_PITCH_MAX) cam->pitch = CAM_PITCH_MAX;
	if (cam->dist < CAM_DIST_MIN) cam->dist = CAM_DIST_MIN;
	if (cam->dist > CAM_DIST_MAX) cam->dist = CAM_DIST_MAX;
}

/* Main -------------------------------------------------------------------- */

int main(int argc, const char** argv)
{
	RenderContext ctx;
	PmdModel model;
	OrbitCamera cam;
	TIM_IMAGE tim;

	ResetGraph(0);
	FntLoad(960, 0);
	SetupContext(&ctx);
	SetupPads();

	InitGeom();
	gte_SetGeomOffset(CENTER_X, CENTER_Y);
	gte_SetGeomScreen(CENTER_X);

	/* Upload the texture and its palette to VRAM. The TIM was placed by
	 * the pipeline outside the framebuffer area (x >= 320). */
	GetTimInfo(checker_tim, &tim);
	LoadImage(tim.prect, tim.paddr);
	if (tim.mode & 0x8)
		LoadImage(tim.crect, tim.caddr);
	DrawSync(0);

	uint16_t tpage = getTPage(tim.mode & 0x3, 0, tim.prect->x, tim.prect->y);
	uint16_t clut = (tim.mode & 0x8) ? getClut(tim.crect->x, tim.crect->y) : 0;

	int err = Pmd_Load(&model, cube_pmd, tpage, clut);
	assert(err == 0);

	/* One white directional light. Rows of the light matrix are 4.12
	 * vectors pointing TOWARD the light source (here: up, behind-left of
	 * the camera at rest), so lit faces get a positive dot product. */
	MATRIX light_mtx = {{
		{ -2365, -2365, -2365 },
		{ 0, 0, 0 },
		{ 0, 0, 0 },
	}, { 0, 0, 0 }};

	/* Color matrix: column 0 = RGB intensity of light 0 (white). */
	MATRIX color_mtx = {{
		{ ONE, 0, 0 },
		{ ONE, 0, 0 },
		{ ONE, 0, 0 },
	}, { 0, 0, 0 }};
	gte_SetColorMatrix(&color_mtx);

	/* Ambient term (background color registers). */
	gte_SetBackColor(64, 64, 64);

	ResetCamera(&cam);

	uint16_t prev_held = 0;
	int tri_total = Pmd_TriangleCount(&model);

	for (;;)
	{
		uint16_t held = PadHeld();
		uint16_t pressed = held & ~prev_held;
		prev_held = held;
		UpdateCamera(&cam, held, pressed);

		/* Orbit = rotate the world by the camera angles, then push it
		 * away. Equivalent to a camera turning around the origin. */
		SVECTOR rotation = { (int16_t)cam.pitch, (int16_t)cam.yaw, 0, 0 };
		VECTOR position = { 0, 0, cam.dist, 0 };
		MATRIX world_mtx, composite_light;

		RotMatrix(&rotation, &world_mtx);
		TransMatrix(&world_mtx, &position);
		gte_SetRotMatrix(&world_mtx);
		gte_SetTransMatrix(&world_mtx);

		/* Bring the world-space light into model space. */
		MulMatrix0(&light_mtx, &world_mtx, &composite_light);
		gte_SetLightMatrix(&composite_light);

		RenderBuffer* draw_buffer = &ctx.buffers[ctx.active_buffer];
		ctx.next_packet = Pmd_Draw(&model, draw_buffer->ot, OT_LENGTH,
			ctx.next_packet, &draw_buffer->buffer[BUFFER_LENGTH]);

		char stats[48];
		sprintf(stats, "TRIS %d  DIST %d", tri_total, cam.dist);
		ctx.next_packet = (uint8_t*)FntSort(&draw_buffer->ot[0],
			ctx.next_packet, 8, 16, "PSX STUDIO - PHASE 1");
		ctx.next_packet = (uint8_t*)FntSort(&draw_buffer->ot[0],
			ctx.next_packet, 8, 28, stats);

		FlipBuffers(&ctx);
	}

	return 0;
}
