/*
 * Script "torche" : vacillement d'une lumiere ponctuelle en faisant
 * osciller son rayon (le moteur recalcule l'attenuation par objet a
 * chaque frame, et le halo suit).
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include <psxgte.h>
#include "engine.h"

/* Rayon d'origine par entite (le .psc le fournit, on oscille autour). */
#define TORCHE_MAX 8
static struct {
	const Entity*	ent;
	uint16_t		base;
} bases[TORCHE_MAX];
static int base_count;
static int frame;

static uint16_t BaseFor(const Entity* self)
{
	for (int i = 0; i < base_count; i++)
		if (bases[i].ent == self)
			return bases[i].base;
	if (base_count < TORCHE_MAX)
	{
		bases[base_count].ent = self;
		bases[base_count].base = self->light_radius;
		return bases[base_count++].base;
	}
	return self->light_radius;
}

void Torche_Start(Entity* self)
{
	BaseFor(self);
}

void Torche_Update(Entity* self)
{
	frame++;
	int base = BaseFor(self);
	/* Deux sinus desaccordes = tremblement organique, +/- ~10 %. */
	int wobble = (isin((frame * 150) & 4095) >> 6)
		+ (isin((frame * 47 + 900) & 4095) >> 7);
	int radius = base + (base * wobble >> 12);
	if (radius < 64)
		radius = 64;
	self->light_radius = (uint16_t)radius;
}
