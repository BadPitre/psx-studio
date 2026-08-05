/*
 * PSX Studio - PMD mesh format loader (Phase 1)
 * Format specification: docs/PMD-FORMAT.md
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#ifndef PSXSTUDIO_PMD_H
#define PSXSTUDIO_PMD_H

#include <stdint.h>
#include <psxgpu.h>
#include <psxgte.h>

#define PMD_VERSION			1
#define PMD_FLAG_TEXTURED	(1 << 0)

/* Kind indices used by counts/offsets, in file order. */
enum {
	PMD_KIND_F3 = 0,
	PMD_KIND_G3,
	PMD_KIND_FT3,
	PMD_KIND_GT3,
	PMD_KIND_COUNT
};

typedef struct {
	char		magic[4];		/* "PMD1" */
	uint16_t	version;
	uint16_t	flags;
	int32_t		scale;			/* 4.12: source units per PMD unit (editor metadata) */
	uint16_t	vertex_count;
	uint16_t	normal_count;
	uint16_t	prim_counts[PMD_KIND_COUNT];
	uint32_t	verts_offset;
	uint32_t	normals_offset;
	uint32_t	prim_offsets[PMD_KIND_COUNT];
} PmdHeader;

/* Primitive records: vertex/normal indices followed by a pre-encoded GPU
 * packet template (tag length set, screen coords and lit colors filled at
 * draw time, tpage/clut patched once at load time). */
typedef struct {
	uint16_t	vidx[3];
	uint16_t	nidx;
	POLY_F3		packet;
} PmdF3;

typedef struct {
	uint16_t	vidx[3];
	uint16_t	nidx[3];
	POLY_G3		packet;
} PmdG3;

typedef struct {
	uint16_t	vidx[3];
	uint16_t	nidx;
	POLY_FT3	packet;
} PmdFT3;

typedef struct {
	uint16_t	vidx[3];
	uint16_t	nidx[3];
	POLY_GT3	packet;
} PmdGT3;

typedef struct {
	const PmdHeader*	header;
	const SVECTOR*		verts;
	const SVECTOR*		normals;
	PmdF3*				f3;
	PmdG3*				g3;
	PmdFT3*				ft3;
	PmdGT3*				gt3;
} PmdModel;

/* Parse an in-memory PMD file and patch texture page/CLUT ids into the
 * textured packet templates. `data` must be writable and 4-byte aligned.
 * Returns 0 on success, negative on error. */
int Pmd_Load(PmdModel* model, void* data, uint16_t tpage, uint16_t clut);

/* Transform, light, depth-sort and enqueue every primitive of the model.
 * The GTE rotation/translation/light/color matrices must be set by the
 * caller. Returns the advanced packet pointer. */
uint8_t* Pmd_Draw(const PmdModel* model, uint32_t* ot, int ot_length,
	uint8_t* packet, uint8_t* packet_limit);

int Pmd_TriangleCount(const PmdModel* model);

#endif
