/*
 * PSX Studio - cote moteur de l'API gameplay (utilise par la boucle
 * principale du jeu, pas par les scripts — ceux-ci incluent engine.h).
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#ifndef PSXSTUDIO_GAMEAPI_H
#define PSXSTUDIO_GAMEAPI_H

#include <stdint.h>
#include <psxgte.h>

void Input_Init(void);
void Input_Update(void);

/* Matrice monde->camera issue du dernier Camera_Set. */
void Camera_GetViewMatrix(MATRIX* view);

/* Dessine la boite de dialogue si ouverte (tile + texte). */
uint8_t* Dialog_Draw(uint32_t* ot, uint8_t* packet);

#endif
