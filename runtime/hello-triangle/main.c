/*
 * PSX Studio - Phase 0 deliverable
 * Rotating Gouraud-shaded 3D triangle using the GTE.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 *
 * Portions adapted from the PSn00bSDK template and GTE example
 * (C) Lameguy64, spicyjpeg - MPL licensed.
 */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <psxgpu.h>
#include <psxgte.h>
#include <inline_c.h>

/* Ordering table length: Z granularity for depth sorting. */
#define OT_LENGTH		256

/* Primitive/packet buffer size. */
#define BUFFER_LENGTH	8192

#define SCREEN_XRES		320
#define SCREEN_YRES		240
#define CENTER_X		(SCREEN_XRES >> 1)
#define CENTER_Y		(SCREEN_YRES >> 1)

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
	/* Two framebuffers stacked vertically in VRAM. */
	SetDefDispEnv(&ctx->buffers[0].disp_env, 0, 0, SCREEN_XRES, SCREEN_YRES);
	SetDefDrawEnv(&ctx->buffers[0].draw_env, 0, SCREEN_YRES, SCREEN_XRES, SCREEN_YRES);
	SetDefDispEnv(&ctx->buffers[1].disp_env, 0, SCREEN_YRES, SCREEN_XRES, SCREEN_YRES);
	SetDefDrawEnv(&ctx->buffers[1].draw_env, 0, 0, SCREEN_XRES, SCREEN_YRES);

	for (int i = 0; i < 2; i++)
	{
		/* Dark blue background, auto-clear and dithering enabled. */
		setRGB0(&ctx->buffers[i].draw_env, 16, 16, 48);
		ctx->buffers[i].draw_env.isbg = 1;
		ctx->buffers[i].draw_env.dtd = 1;
	}

	ctx->active_buffer = 0;
	ctx->next_packet = ctx->buffers[0].buffer;
	ClearOTagR(ctx->buffers[0].ot, OT_LENGTH);
	PutDrawEnv(&ctx->buffers[0].draw_env);

	/* Turn on video output. */
	SetDispMask(1);
}

static void FlipBuffers(RenderContext* ctx)
{
	DrawSync(0);
	VSync(0);

	RenderBuffer* draw_buffer = &ctx->buffers[ctx->active_buffer];
	RenderBuffer* disp_buffer = &ctx->buffers[ctx->active_buffer ^ 1];

	PutDispEnv(&disp_buffer->disp_env);

	/* OT was cleared with ClearOTagR, so it is drawn from the end. */
	DrawOTagEnv(&draw_buffer->ot[OT_LENGTH - 1], &draw_buffer->draw_env);

	ctx->active_buffer ^= 1;
	ctx->next_packet = disp_buffer->buffer;
	ClearOTagR(disp_buffer->ot, OT_LENGTH);
}

static void* NewPrimitive(RenderContext* ctx, int z, size_t size)
{
	RenderBuffer* buffer = &ctx->buffers[ctx->active_buffer];
	uint8_t* prim = ctx->next_packet;

	addPrim(&buffer->ot[z], prim);
	ctx->next_packet += size;

	assert(ctx->next_packet <= &buffer->buffer[BUFFER_LENGTH]);

	return (void*)prim;
}

/* Geometry --------------------------------------------------------------- */

/* One triangle in model space (16-bit fixed coordinates). */
static SVECTOR TriangleVerts[3] =
{
	{    0, -96, 0, 0 },
	{  -96,  96, 0, 0 },
	{   96,  96, 0, 0 }
};

/* Main ------------------------------------------------------------------- */

int main(int argc, const char** argv)
{
	RenderContext ctx;

	SVECTOR rotation = { 0, 0, 0, 0 };
	VECTOR position = { 0, 0, 400, 0 };
	MATRIX transform;

	/* Init GPU, debug font and rendering context. */
	ResetGraph(0);
	FntLoad(960, 0);
	SetupContext(&ctx);

	/* Init GTE: projection offset (screen center) and projection plane
	 * distance (half the screen width gives a natural FOV). */
	InitGeom();
	gte_SetGeomOffset(CENTER_X, CENTER_Y);
	gte_SetGeomScreen(CENTER_X);

	for (;;)
	{
		/* Build the world matrix from Euler angles + translation, then
		 * load it into the GTE registers. */
		RotMatrix(&rotation, &transform);
		TransMatrix(&transform, &position);
		gte_SetRotMatrix(&transform);
		gte_SetTransMatrix(&transform);

		/* Spin. 4096 = one full turn in PS1 fixed-point angle units. */
		rotation.vy += 16;
		rotation.vx += 6;

		/* Transform and project the 3 vertices in one GTE call. */
		gte_ldv3(&TriangleVerts[0], &TriangleVerts[1], &TriangleVerts[2]);
		gte_rtpt();

		/* Average Z of the triangle, used as OT index. */
		int otz;
		gte_avsz3();
		gte_stotz(&otz);
		otz >>= 2;

		if (otz > 0 && otz < OT_LENGTH)
		{
			/* Gouraud-shaded triangle: one color per vertex.
			 * No backface culling on purpose so the triangle stays
			 * visible from both sides while it spins. */
			POLY_G3* poly = (POLY_G3*)NewPrimitive(&ctx, otz, sizeof(POLY_G3));

			setPolyG3(poly);
			gte_stsxy0(&poly->x0);
			gte_stsxy1(&poly->x1);
			gte_stsxy2(&poly->x2);
			setRGB0(poly, 255, 64, 64);
			setRGB1(poly, 64, 255, 64);
			setRGB2(poly, 64, 64, 255);
		}

		/* Debug text on top (Z = 0 is drawn last with a reversed OT). */
		ctx.next_packet = (uint8_t*)FntSort(
			&ctx.buffers[ctx.active_buffer].ot[0],
			ctx.next_packet, 8, 16, "PSX STUDIO - PHASE 0");

		FlipBuffers(&ctx);
	}

	return 0;
}
