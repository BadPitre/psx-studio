/*
 * PSX Studio - lecture de SFX VAG sur le SPU.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#ifndef PSXSTUDIO_SFX_H
#define PSXSTUDIO_SFX_H

typedef struct {
	int	addr;
	int	sample_rate;
	int	valid;
} SfxSample;

void		Sfx_Init(void);
/* Upload un VAG en SPU RAM (le buffer source peut etre reutilise). */
SfxSample	Sfx_UploadVag(const void* data);
void		Sfx_Play(const SfxSample* sample);

#endif
