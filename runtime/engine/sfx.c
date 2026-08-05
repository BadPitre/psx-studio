/*
 * PSX Studio - lecture de SFX VAG sur le SPU.
 * Upload d'apres l'exemple vagsample de PSn00bSDK (MPL, Lameguy64/spicyjpeg).
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include <psxspu.h>
#include <hwregs_c.h>

#include "sfx.h"

/* Les 4 premiers Ko de SPU RAM sont reserves (+ un bloc factice a 0x1000). */
#define SPU_ALLOC_START 0x1010

typedef struct {
	uint32_t	magic;			/* "VAGp" */
	uint32_t	version;
	uint32_t	interleave;
	uint32_t	size;			/* big-endian, octets */
	uint32_t	sample_rate;	/* big-endian, Hz */
	uint16_t	reserved[5];
	uint16_t	channels;
	char		name[16];
} VagHeader;

static int next_addr = SPU_ALLOC_START;
static int next_channel = 0;

void Sfx_Init(void)
{
	SpuInit();
	next_addr = SPU_ALLOC_START;
	next_channel = 0;
}

SfxSample Sfx_UploadVag(const void* data)
{
	SfxSample sample = {0};
	const VagHeader* header = (const VagHeader*)data;

	if (header->magic != 0x70474156)	/* "VAGp" little-endian */
		return sample;

	int size = (int)__builtin_bswap32(header->size);
	size = (size + 63) & ~63;

	SpuSetTransferMode(SPU_TRANSFER_BY_DMA);
	SpuSetTransferStartAddr(next_addr);
	SpuWrite((const uint32_t*)(header + 1), size);
	SpuIsTransferCompleted(SPU_TRANSFER_WAIT);

	sample.addr = next_addr;
	sample.sample_rate = (int)__builtin_bswap32(header->sample_rate);
	sample.valid = 1;
	next_addr += size;
	return sample;
}

void Sfx_Play(const SfxSample* sample)
{
	if (!sample->valid)
		return;

	int ch = next_channel;
	next_channel = (next_channel + 1) % 8;

	SpuSetKey(0, 1 << ch);
	SPU_CH_FREQ(ch) = getSPUSampleRate(sample->sample_rate);
	SPU_CH_ADDR(ch) = getSPUAddr(sample->addr);
	SPU_CH_VOL_L(ch) = 0x3fff;
	SPU_CH_VOL_R(ch) = 0x3fff;
	SPU_CH_ADSR1(ch) = 0x00ff;
	SPU_CH_ADSR2(ch) = 0x0000;
	SpuSetKey(1, 1 << ch);
}
