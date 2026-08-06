/*
 * PSX Studio - API moteur pour les scripts de gameplay (Phase 5)
 *
 * Un script C expose deux fonctions et s'enregistre dans le registre du
 * jeu (g_scripts). L'editeur attache un script a une entite par son nom ;
 * le runtime resout le nom (hash FNV-1a 32, minuscules) au chargement.
 *
 *   void MonScript_Start(Entity* self);   // apres le chargement de scene
 *   void MonScript_Update(Entity* self);  // chaque frame, avant le rendu
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#ifndef PSXSTUDIO_ENGINE_H
#define PSXSTUDIO_ENGINE_H

#include <stdint.h>
#include <psxgte.h>
#include <psxpad.h>

/* ------------------------------------------------------------- entites -- */

typedef struct Entity
{
	/* Transform locale, mutable par les scripts. Le monde est recalcule
	 * chaque frame (les parents viennent toujours avant les enfants). */
	VECTOR		pos;		/* unites monde (+Y vers le bas) */
	SVECTOR		rot;		/* 4096 = un tour complet */
	SVECTOR		scale;		/* 4.12 (4096 = 1.0) */

	int16_t		model;		/* indice de modele, -1 = aucun */
	int16_t		parent;		/* indice d'entite, -1 = racine */
	uint16_t	script;		/* indice table scripts + 1, 0 = aucun */
	uint16_t	flags;		/* composants : ENTITY_FLAG_LIGHT / _CAMERA */
	uint16_t	cam_fov;	/* FOV vertical camera en degres, 0 = defaut */
	uint16_t	cam_draw;	/* distance d'affichage camera, 0 = illimitee */
	uint16_t	light_radius;	/* rayon lumiere ponctuelle (mutable :
							 * le faire osciller = vacillement) */
	uint8_t		visible;	/* 0 = ni rendu ni collision */
	uint8_t		solid;		/* participe aux collisions AABB */

	/* Rempli par le moteur a chaque frame. */
	MATRIX		world;		/* rotation*echelle + translation monde */
	MATRIX		light_rot;	/* rotation seule, pour l'eclairage */
} Entity;

/* ------------------------------------------------------------- scripts -- */

typedef struct
{
	const char*	name;					/* nom cote editeur/scene.json */
	void		(*on_start)(Entity*);	/* peut etre NULL */
	void		(*on_update)(Entity*);	/* peut etre NULL */
} ScriptDef;

/* Fournis par le jeu (scripts/registry.c). */
extern const ScriptDef	g_scripts[];
extern const int		g_script_count;

uint32_t Script_Hash(const char* name);

/* --------------------------------------------------------------- scene -- */

int		Scene_EntityCount(void);
Entity*	Scene_GetEntity(int index);
/* Premiere entite dont le script porte ce nom (NULL si absente). */
Entity*	Scene_FindByScript(const char* name);

/* --------------------------------------------------------------- input -- */
/* Masques PAD_* de psxpad.h (PAD_UP, PAD_CROSS, ...). */

uint16_t Input_Held(void);
uint16_t Input_Pressed(void);

/* -------------------------------------------------------------- camera -- */
/* La camera est pilotee par les scripts (position monde + angles). */

void Camera_Set(int32_t x, int32_t y, int32_t z, int yaw, int pitch);

/* Applique la premiere entite camera de la scene courante comme vue
 * initiale (position monde + rotation). Retourne 1 si appliquee — les
 * scripts peuvent ensuite reprendre la main a chaque frame. */
int Scene_ApplyCamera(void);

/* Streaming : un script le met a 1 pour demander la bascule vers la
 * scene prechargee (portail). La boucle de jeu consomme la demande
 * quand le prechargement est pret. */
extern int g_scene_switch_request;

/* ------------------------------------------------------------- dialogue -- */
/* Boite de dialogue en bas d'ecran (jusqu'a 3 lignes, separees par \n).
 * Le texte doit rester valide tant que la boite est ouverte. */

void	Dialog_Show(const char* text);
int		Dialog_IsOpen(void);
void	Dialog_Close(void);
/* Texte courant (NULL si fermee) — pour un rendu custom (canvas UI). */
const char* Dialog_Text(void);
/* Un canvas UI de la scene rend le dialogue (script "dialogue" : passer 1
 * dans son Start) : le fallback Dialog_Draw s'efface. Remis a 0 a chaque
 * chargement de scene. -1 = lire l'etat sans le changer. */
int		Dialog_UiCanvas(int present);

/* ------------------------------------------------------------------- UI -- */
/* Les widgets UI (SceneFormat v1.3) SONT des entites de la scene
 * courante : on les retrouve comme les autres (index, script attache).
 * Setters silencieux si l'entite n'a pas le composant vise. */

#define UI_COMP_CANVAS	(1 << 0)
#define UI_COMP_IMAGE	(1 << 1)
#define UI_COMP_TEXT	(1 << 2)
#define UI_COMP_BUTTON	(1 << 3)
#define UI_COMP_LAYOUT	(1 << 4)
#define UI_COMP_ACTIVE	(1 << 5)

/* Masque UI_COMP_* de l'entite (0 = pas un widget). */
uint8_t	Ui_Components(const Entity* e);
/* Type d'image : 0 simple, 1 sliced, 2 tiled, 3 filled, -1 sans image. */
int		Ui_ImageType(const Entity* e);
/* Remplissage d'une image Filled, 0..4096 (jauges). */
void	Ui_SetFill(const Entity* e, int amount_412);
/* Montre/cache un widget (un canvas cache masque tout son sous-arbre). */
void	Ui_SetActive(const Entity* e, int active);
void	Ui_SetTint(const Entity* e, uint8_t r, uint8_t g, uint8_t b);
/* Remplace la chaine d'un widget texte. Le pointeur doit rester valide
 * (buffer du script) ; NULL = retour a la chaine de la scene. */
void	Ui_SetText(const Entity* e, const char* text);

/* Focus D-pad : navigation geometrique entre les boutons visibles
 * (UI_COMP_BUTTON). Le moteur surligne le bouton focalise au dessin ;
 * le jeu decide quoi faire de X/O. */
void	Ui_FocusInit(void);				/* premier bouton visible */
void	Ui_FocusClear(void);
/* dx/dy dans -1/0/+1 (axes ecran, +y vers le bas). 1 si le focus a bouge. */
int		Ui_FocusMove(int dx, int dy);
Entity*	Ui_Focused(void);				/* NULL si aucun */

/* ------------------------------------------------------------- physique -- */
/* Deplace l'entite en glissant contre les AABB (axes monde) des autres
 * entites solides. Y est ignore (deplacement au sol). */

void Physics_MoveAndSlide(Entity* e, int32_t dx, int32_t dz);
/* Distance au carre (X/Z) entre deux entites, en unites monde. */
int32_t Entity_Dist2XZ(const Entity* a, const Entity* b);

#endif
