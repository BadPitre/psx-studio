/*
 * PSX Studio - Phase 2 deliverable
 * Generic scene player: loads SceneFormat v1 (.psc) files from the CD,
 * renders the entity hierarchy, free camera, scene switching.
 *
 * Controls:
 *   D-pad up/down     move forward/back (along the view direction)
 *   D-pad left/right  turn (yaw)
 *   L1 / R1           strafe left/right
 *   Triangle / Cross  look up/down
 *   Square            play the demo SFX (BLIP.VAG via SPU)
 *   Select            toggle CD-DA music (track 2, if the ISO has one)
 *   Circle            load the next scene
 *   Start             reset the camera
 *
 * Copyright (c) 2026 Bertrand - MIT License
 *
 * Rendering skeleton shared with Phase 0/1, portions adapted from the
 * PSn00bSDK template and examples (C) Lameguy64, spicyjpeg - MPL licensed.
 * Camera math follows the fpscam example.
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
#include <psxspu.h>
#include <hwregs_c.h>
#include <inline_c.h>

#include "pmd.h"
#include "scene.h"

/* Le player (Phase 2-3) n'embarque aucun script de gameplay : registre
 * vide, les entites scriptees d'une scene sont simplement statiques. */
const ScriptDef g_scripts[1] = {{0, 0, 0}};
const int g_script_count = 0;

#define OT_LENGTH		1024
#define BUFFER_LENGTH	32768

#define SCREEN_XRES		320
#define SCREEN_YRES		240
#define CENTER_X		(SCREEN_XRES >> 1)
#define CENTER_Y		(SCREEN_YRES >> 1)

static const char* const SCENE_PATHS[] = {
	"\\SCENE0.PSC;1",
	"\\SCENE1.PSC;1",
};
#define SCENE_COUNT ((int)(sizeof(SCENE_PATHS) / sizeof(SCENE_PATHS[0])))

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

/* Pad input --------------------------------------------------------------- */

static uint8_t pad_buff[2][34];

static void SetupPads(void)
{
	InitPAD(pad_buff[0], 34, pad_buff[1], 34);
	StartPAD();
	ChangeClearPAD(0);
}

static uint16_t PadHeld(void)
{
	const PADTYPE* pad = (const PADTYPE*)pad_buff[0];

	if (pad->stat != 0)
		return 0;
	if (pad->type != 0x4 && pad->type != 0x5 && pad->type != 0x7)
		return 0;
	return (uint16_t)~pad->btn;
}

/* Free camera (fpscam-style) ---------------------------------------------- */

#define CAM_TURN_SPEED	24
#define CAM_MOVE_SPEED	6
#define CAM_PITCH_MIN	-1024
#define CAM_PITCH_MAX	1024

typedef struct
{
	VECTOR	pos;		/* world units */
	int		yaw;		/* 4096 = full turn */
	int		pitch;
} FreeCamera;

static void ResetCamera(FreeCamera* cam)
{
	cam->pos.vx = 0;
	cam->pos.vy = -140;		/* above the ground (Y is down) */
	cam->pos.vz = -420;
	cam->yaw = 0;
	cam->pitch = 170;		/* look slightly down at the scene */
}

static void UpdateCamera(FreeCamera* cam, uint16_t held)
{
	if (held & PAD_LEFT)
		cam->yaw -= CAM_TURN_SPEED;
	if (held & PAD_RIGHT)
		cam->yaw += CAM_TURN_SPEED;
	if (held & PAD_TRIANGLE)
		cam->pitch -= CAM_TURN_SPEED;
	if (held & PAD_CROSS)
		cam->pitch += CAM_TURN_SPEED;

	cam->yaw &= 4095;
	if (cam->pitch < CAM_PITCH_MIN) cam->pitch = CAM_PITCH_MIN;
	if (cam->pitch > CAM_PITCH_MAX) cam->pitch = CAM_PITCH_MAX;

	/* Movement along the view direction (fpscam sign conventions). */
	int sy = isin(cam->yaw), cy = icos(cam->yaw);
	int sp = isin(cam->pitch), cp = icos(cam->pitch);

	if (held & PAD_UP)
	{
		cam->pos.vx -= (((sy * cp) >> 12) * CAM_MOVE_SPEED) >> 12;
		cam->pos.vy += (sp * CAM_MOVE_SPEED) >> 12;
		cam->pos.vz += (((cy * cp) >> 12) * CAM_MOVE_SPEED) >> 12;
	}
	if (held & PAD_DOWN)
	{
		cam->pos.vx += (((sy * cp) >> 12) * CAM_MOVE_SPEED) >> 12;
		cam->pos.vy -= (sp * CAM_MOVE_SPEED) >> 12;
		cam->pos.vz -= (((cy * cp) >> 12) * CAM_MOVE_SPEED) >> 12;
	}
	if (held & PAD_L1)
	{
		cam->pos.vx -= (cy * CAM_MOVE_SPEED) >> 12;
		cam->pos.vz -= (sy * CAM_MOVE_SPEED) >> 12;
	}
	if (held & PAD_R1)
	{
		cam->pos.vx += (cy * CAM_MOVE_SPEED) >> 12;
		cam->pos.vz += (sy * CAM_MOVE_SPEED) >> 12;
	}
}

/* Audio ------------------------------------------------------------------- */

/* First free SPU RAM address (the SPU reserves 4 KB + a dummy block). */
#define SPU_ALLOC_START 0x1010

typedef struct {
	uint32_t	magic;			/* "VAGp" */
	uint32_t	version;
	uint32_t	interleave;
	uint32_t	size;			/* big-endian, bytes */
	uint32_t	sample_rate;	/* big-endian, Hz */
	uint16_t	reserved[5];
	uint16_t	channels;
	char		name[16];
} VagHeader;

typedef struct {
	int	addr;
	int	sample_rate;
	int	valid;
} SfxSample;

/* Upload a VAG read from CD to SPU RAM. The source buffer can be reused
 * afterwards (the SPU keeps its own copy). */
static SfxSample UploadVag(const void* data)
{
	SfxSample sample = {0};
	const VagHeader* header = (const VagHeader*)data;

	if (header->magic != 0x70474156)	/* "VAGp" little-endian */
		return sample;

	int size = (int)__builtin_bswap32(header->size);
	size = (size + 63) & ~63;

	SpuSetTransferMode(SPU_TRANSFER_BY_DMA);
	SpuSetTransferStartAddr(SPU_ALLOC_START);
	SpuWrite((const uint32_t*)(header + 1), size);
	SpuIsTransferCompleted(SPU_TRANSFER_WAIT);

	sample.addr = SPU_ALLOC_START;
	sample.sample_rate = (int)__builtin_bswap32(header->sample_rate);
	sample.valid = 1;
	return sample;
}

static void PlaySfx(const SfxSample* sample)
{
	if (!sample->valid)
		return;

	SpuSetKey(0, 1 << 0);
	SPU_CH_FREQ(0) = getSPUSampleRate(sample->sample_rate);
	SPU_CH_ADDR(0) = getSPUAddr(sample->addr);
	SPU_CH_VOL_L(0) = 0x3fff;
	SPU_CH_VOL_R(0) = 0x3fff;
	SPU_CH_ADSR1(0) = 0x00ff;
	SPU_CH_ADSR2(0) = 0x0000;
	SpuSetKey(1, 1 << 0);
}

/* CD-DA music: track 2 of the disc (track 1 is the data track). Data reads
 * interrupt playback, so this is re-issued after a scene switch. */
static void StartMusic(void)
{
	uint8_t mode = CdlModeDA | CdlModeRept;
	uint8_t track = 2;
	SPU_CD_VOL_L = 0x3fff;
	SPU_CD_VOL_R = 0x3fff;
	CdControl(CdlSetmode, &mode, 0);
	CdControl(CdlPlay, &track, 0);
}

static void StopMusic(void)
{
	CdControl(CdlPause, 0, 0);
}

/* World-to-camera matrix, fpscam style: rotation from the camera angles,
 * translation = rotated negated camera position. */
static void CameraMatrix(const FreeCamera* cam, MATRIX* view)
{
	SVECTOR rot = { (int16_t)cam->pitch, (int16_t)cam->yaw, 0, 0 };
	RotMatrix(&rot, view);

	VECTOR neg = { -cam->pos.vx, -cam->pos.vy, -cam->pos.vz, 0 };
	VECTOR t;
	ApplyMatrixLV(view, &neg, &t);
	TransMatrix(view, &t);
}

/* Main -------------------------------------------------------------------- */

int main(int argc, const char** argv)
{
	RenderContext ctx;
	Scene scene;
	FreeCamera cam;

	ResetGraph(0);
	FntLoad(960, 0);
	SetupContext(&ctx);
	SetupPads();

	InitGeom();
	gte_SetGeomOffset(CENTER_X, CENTER_Y);
	gte_SetGeomScreen(CENTER_X);

	CdInit();
	SpuInit();

	/* Boot-time SFX upload: read the VAG into the (still empty) arena,
	 * push it to SPU RAM, then let the scene load reuse the arena. */
	SfxSample blip = {0};
	{
		uint32_t size;
		void* vag = Scene_ReadFileToArena("\\BLIP.VAG;1", &size);
		if (vag)
			blip = UploadVag(vag);
	}

	int scene_index = 0;
	int music_on = 0;
	int err = Scene_LoadFromCd(&scene, SCENE_PATHS[scene_index]);
	assert(err == 0);
	SetBackground(&ctx, &scene.background);
	ResetCamera(&cam);

	uint16_t prev_held = 0;
	int tri_total = Scene_TriangleCount(&scene);

	for (;;)
	{
		uint16_t held = PadHeld();
		uint16_t pressed = held & ~prev_held;
		prev_held = held;

		if (pressed & PAD_CIRCLE)
		{
			/* Let the GPU finish the in-flight frame, then swap scenes.
			 * The arena is reset by the load; VRAM is simply overwritten. */
			DrawSync(0);
			int next = (scene_index + 1) % SCENE_COUNT;
			err = Scene_LoadFromCd(&scene, SCENE_PATHS[next]);
			if (err == 0)
			{
				scene_index = next;
			}
			else
			{
				/* Missing/bad scene: fall back to the current one (the
				 * arena was reset, so it must be reloaded). */
				err = Scene_LoadFromCd(&scene, SCENE_PATHS[scene_index]);
				assert(err == 0);
			}
			SetBackground(&ctx, &scene.background);
			ResetCamera(&cam);
			tri_total = Scene_TriangleCount(&scene);
			/* The CD read stopped any CD-DA playback. */
			if (music_on)
				StartMusic();
		}
		if (pressed & PAD_SQUARE)
			PlaySfx(&blip);
		if (pressed & PAD_SELECT)
		{
			music_on ^= 1;
			if (music_on)
				StartMusic();
			else
				StopMusic();
		}
		if (pressed & PAD_START)
			ResetCamera(&cam);

		UpdateCamera(&cam, held);

		MATRIX view;
		CameraMatrix(&cam, &view);

		/* Les transforms sont mutables depuis la Phase 5 : recalcul des
		 * matrices monde a chaque frame (necessaire au live tweaking). */
		Scene_UpdateWorld(&scene);

		RenderBuffer* draw_buffer = &ctx.buffers[ctx.active_buffer];
		ctx.next_packet = Scene_Draw(&scene, &view, draw_buffer->ot, OT_LENGTH,
			ctx.next_packet, &draw_buffer->buffer[BUFFER_LENGTH]);

		char stats[64];
		sprintf(stats, "SCENE %d/%d  TRIS %d  O:SUIV  []:SFX",
			scene_index + 1, SCENE_COUNT, tri_total);
		ctx.next_packet = (uint8_t*)FntSort(&draw_buffer->ot[0],
			ctx.next_packet, 8, 16, "PSX STUDIO - PHASE 2");
		ctx.next_packet = (uint8_t*)FntSort(&draw_buffer->ot[0],
			ctx.next_packet, 8, 28, stats);

		FlipBuffers(&ctx);
	}

	return 0;
}
