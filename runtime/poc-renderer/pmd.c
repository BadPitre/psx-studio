/*
 * PSX Studio - PMD mesh format loader and renderer (Phase 1)
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <psxgpu.h>
#include <psxgte.h>
#include <inline_c.h>

#include "pmd.h"

/* Record sizes are part of the format: catch struct padding surprises at
 * compile time (see docs/PMD-FORMAT.md). */
_Static_assert(sizeof(PmdHeader) == 48, "PMD header must be 48 bytes");
_Static_assert(sizeof(PmdF3) == 8 + sizeof(POLY_F3), "PmdF3 padding");
_Static_assert(sizeof(PmdG3) == 12 + sizeof(POLY_G3), "PmdG3 padding");
_Static_assert(sizeof(PmdFT3) == 8 + sizeof(POLY_FT3), "PmdFT3 padding");
_Static_assert(sizeof(PmdGT3) == 12 + sizeof(POLY_GT3), "PmdGT3 padding");

int Pmd_Load(PmdModel* model, void* data, uint16_t tpage, uint16_t clut)
{
	PmdHeader* header = (PmdHeader*)data;
	uint8_t* base = (uint8_t*)data;

	if (memcmp(header->magic, "PMD1", 4) != 0)
		return -1;
	if (header->version != PMD_VERSION)
		return -2;

	model->header  = header;
	model->verts   = (const SVECTOR*)(base + header->verts_offset);
	model->normals = (const SVECTOR*)(base + header->normals_offset);
	model->f3      = (PmdF3*)(base + header->prim_offsets[PMD_KIND_F3]);
	model->g3      = (PmdG3*)(base + header->prim_offsets[PMD_KIND_G3]);
	model->ft3     = (PmdFT3*)(base + header->prim_offsets[PMD_KIND_FT3]);
	model->gt3     = (PmdGT3*)(base + header->prim_offsets[PMD_KIND_GT3]);

	/* One-time fixup: point every textured packet at the uploaded TIM. */
	for (int i = 0; i < header->prim_counts[PMD_KIND_FT3]; i++)
	{
		model->ft3[i].packet.tpage = tpage;
		model->ft3[i].packet.clut  = clut;
	}
	for (int i = 0; i < header->prim_counts[PMD_KIND_GT3]; i++)
	{
		model->gt3[i].packet.tpage = tpage;
		model->gt3[i].packet.clut  = clut;
	}

	return 0;
}

int Pmd_TriangleCount(const PmdModel* model)
{
	int total = 0;
	for (int i = 0; i < PMD_KIND_COUNT; i++)
		total += model->header->prim_counts[i];
	return total;
}

/* Copy a packet template (tag included) into the primitive buffer. */
static inline void* CopyPacket(uint8_t** packet, const void* template_,
	size_t size)
{
	uint32_t* dst = (uint32_t*)*packet;
	const uint32_t* src = (const uint32_t*)template_;
	for (size_t i = 0; i < size / 4; i++)
		dst[i] = src[i];
	*packet += size;
	return dst;
}

/* Transform and project one triangle. Returns the OT index, or -1 if the
 * triangle is backfacing or out of range. Screen coordinates are left in
 * the GTE SXY registers. */
static inline int TransformTri(const SVECTOR* v0, const SVECTOR* v1,
	const SVECTOR* v2, int ot_length)
{
	int nclip, otz;

	gte_ldv3(v0, v1, v2);
	gte_rtpt();

	/* Winding test (backface culling). */
	gte_nclip();
	gte_stopz(&nclip);
	if (nclip <= 0)
		return -1;

	/* Average Z, pre-divided by 4 by the GTE (ZSF3). */
	gte_avsz3();
	gte_stotz(&otz);
	otz >>= 2;
	if (otz <= 0 || otz >= ot_length)
		return -1;

	return otz;
}

uint8_t* Pmd_Draw(const PmdModel* model, uint32_t* ot, int ot_length,
	uint8_t* packet, uint8_t* packet_limit)
{
	const PmdHeader* header = model->header;
	const SVECTOR* verts = model->verts;
	const SVECTOR* normals = model->normals;

	/* Flat untextured triangles. */
	for (int i = 0; i < header->prim_counts[PMD_KIND_F3]; i++)
	{
		const PmdF3* prim = &model->f3[i];
		int otz = TransformTri(&verts[prim->vidx[0]], &verts[prim->vidx[1]],
			&verts[prim->vidx[2]], ot_length);
		if (otz < 0)
			continue;

		POLY_F3* poly = (POLY_F3*)CopyPacket(&packet, &prim->packet, sizeof(POLY_F3));
		gte_stsxy0(&poly->x0);
		gte_stsxy1(&poly->x1);
		gte_stsxy2(&poly->x2);

		/* One normal: light the base color, write it back. */
		gte_ldrgb(&poly->r0);
		gte_ldv0(&normals[prim->nidx]);
		gte_nccs();
		gte_strgb(&poly->r0);

		addPrim(&ot[otz], poly);
	}

	/* Gouraud untextured triangles. */
	for (int i = 0; i < header->prim_counts[PMD_KIND_G3]; i++)
	{
		const PmdG3* prim = &model->g3[i];
		int otz = TransformTri(&verts[prim->vidx[0]], &verts[prim->vidx[1]],
			&verts[prim->vidx[2]], ot_length);
		if (otz < 0)
			continue;

		POLY_G3* poly = (POLY_G3*)CopyPacket(&packet, &prim->packet, sizeof(POLY_G3));
		gte_stsxy0(&poly->x0);
		gte_stsxy1(&poly->x1);
		gte_stsxy2(&poly->x2);

		gte_ldrgb(&poly->r0);
		gte_ldv3(&normals[prim->nidx[0]], &normals[prim->nidx[1]],
			&normals[prim->nidx[2]]);
		gte_ncct();
		gte_strgb3(&poly->r0, &poly->r1, &poly->r2);

		addPrim(&ot[otz], poly);
	}

	/* Flat textured triangles. */
	for (int i = 0; i < header->prim_counts[PMD_KIND_FT3]; i++)
	{
		const PmdFT3* prim = &model->ft3[i];
		int otz = TransformTri(&verts[prim->vidx[0]], &verts[prim->vidx[1]],
			&verts[prim->vidx[2]], ot_length);
		if (otz < 0)
			continue;

		POLY_FT3* poly = (POLY_FT3*)CopyPacket(&packet, &prim->packet, sizeof(POLY_FT3));
		gte_stsxy0(&poly->x0);
		gte_stsxy1(&poly->x1);
		gte_stsxy2(&poly->x2);

		gte_ldrgb(&poly->r0);
		gte_ldv0(&normals[prim->nidx]);
		gte_nccs();
		gte_strgb(&poly->r0);

		addPrim(&ot[otz], poly);
	}

	/* Gouraud textured triangles. */
	for (int i = 0; i < header->prim_counts[PMD_KIND_GT3]; i++)
	{
		const PmdGT3* prim = &model->gt3[i];
		int otz = TransformTri(&verts[prim->vidx[0]], &verts[prim->vidx[1]],
			&verts[prim->vidx[2]], ot_length);
		if (otz < 0)
			continue;

		POLY_GT3* poly = (POLY_GT3*)CopyPacket(&packet, &prim->packet, sizeof(POLY_GT3));
		gte_stsxy0(&poly->x0);
		gte_stsxy1(&poly->x1);
		gte_stsxy2(&poly->x2);

		gte_ldrgb(&poly->r0);
		gte_ldv3(&normals[prim->nidx[0]], &normals[prim->nidx[1]],
			&normals[prim->nidx[2]]);
		gte_ncct();
		gte_strgb3(&poly->r0, &poly->r1, &poly->r2);

		addPrim(&ot[otz], poly);
	}

	assert(packet <= packet_limit);
	return packet;
}
