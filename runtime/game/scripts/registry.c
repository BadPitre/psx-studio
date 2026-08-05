/*
 * Registre des scripts du jeu : fait le lien entre les noms utilises dans
 * l'editeur (scene.json "script": "...") et les fonctions C compilees.
 * Le runtime resout par hash du nom, l'ordre n'a pas d'importance.
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include "engine.h"

void Player_Start(Entity* self);
void Player_Update(Entity* self);
void Npc_Start(Entity* self);
void Npc_Update(Entity* self);
void Torche_Start(Entity* self);
void Torche_Update(Entity* self);

const ScriptDef g_scripts[] = {
	{ "player", Player_Start, Player_Update },
	{ "npc",    Npc_Start,    Npc_Update },
	{ "torche", Torche_Start, Torche_Update },
};

const int g_script_count = sizeof(g_scripts) / sizeof(g_scripts[0]);
