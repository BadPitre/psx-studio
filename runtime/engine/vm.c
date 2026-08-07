/*
 * PSX Studio - VM PSX Script (bytecode "PSB1", docs/PSX-SCRIPT.md)
 *
 * Machine a REGISTRES (pas de pile) : 16 registres i32 par instance,
 * emplacements decides a la compilation par psxpipe — zero allocation,
 * zero GC, entiers uniquement. Une instruction = un u32 a decodage fixe
 * (op | a | b | c), la boucle est un simple switch ; chaque opcode
 * moteur appelle directement le C natif (Physics_MoveAndSlide,
 * Dialog_Show...) — la VM n'orchestre que la logique.
 *
 * Le blob vit dans l'arene de scene (charge avec le .psc, aucune copie).
 * Lie par le jeu uniquement (le player ne fait qu'afficher).
 *
 * Copyright (c) 2026 Bertrand - MIT License
 */

#include <stdint.h>
#include <string.h>

#include "engine.h"
#include "scene.h"

#define VM_REGS			16
#define VM_MAX_INSTANCES	24
/* Garde-fou : un `tantque` infini rend la main au bout de N pas au lieu
 * de geler la console (le script reprendra a la frame suivante). */
#define VM_MAX_STEPS		4096

/* En-tete PSB1 (petit-boutiste, miroir de psxpipe/src/psxs.rs). */
#define PSB_CONSTS(b)	(*(const uint16_t*)((b) + 6))
#define PSB_CODELEN(b)	(*(const uint16_t*)((b) + 8))
#define PSB_START(b)	(*(const uint16_t*)((b) + 10))
#define PSB_UPDATE(b)	(*(const uint16_t*)((b) + 12))
#define PSB_NO_PC		0xFFFF
/* En-tete PSB2 : 20 octets (les 4 derniers = nb de champs publics). */
#define PSB_HEADER		20

/* Opcodes (miroir de psxs.rs). */
enum
{
	OP_NOP = 0, OP_RET, OP_LOADI, OP_LOADK, OP_MOV,
	OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MOD, OP_NEG,
	OP_LT, OP_LE, OP_EQ, OP_NE, OP_AND, OP_OR, OP_NOT,
	OP_JMP, OP_JZ,
	OP_SELF = 21, OP_GETPOS, OP_SETPOS, OP_GETROTY, OP_SETROTY,
	OP_ADDROTY, OP_MOVE, OP_HELD, OP_PRESSED, OP_DIST, OP_FIND,
	OP_DIALOG, OP_DLGOPEN, OP_DLGCLOSE, OP_SHOW, OP_SWITCH, OP_RAND,
	/* Fonctions utilisateur (frames sur pile statique). */
	OP_ENTER, OP_CALL, OP_RETV, OP_RET0, OP_GLOAD, OP_GSTORE,
	/* Champ `public` : surcharge de l'inspecteur si l'instance en a une. */
	OP_INITPUB
};

/* Pile d'appels des fonctions utilisateur : frames statiques (zero
 * allocation), recursion bornee — un appel au-dela de la profondeur max
 * renvoie 0 au lieu de deborder. Run() n'est pas reentrant (une instance
 * a la fois), la pile peut donc etre partagee. */
#define VM_CALL_DEPTH 8

typedef struct
{
	uint16_t	ret_pc;
	uint8_t		ret_slot;
	int32_t*	regs;
} VmCall;

static int32_t	frame_pool[VM_CALL_DEPTH][VM_REGS];
static VmCall	call_stack[VM_CALL_DEPTH];

typedef struct
{
	int16_t			entity;
	int16_t			script;		/* indice table + 1 (valeurs publiques) */
	const uint8_t*	blob;
	int32_t			regs[VM_REGS];
} VmInstance;

static VmInstance	instances[VM_MAX_INSTANCES];
static int			instance_count;
static uint32_t		rng_state = 0x2A5F1C3;

static Entity* EntityAt(Scene* scene, int32_t v)
{
	if (v < 0 || v >= scene->entity_count)
		return 0;
	return &scene->entities[v];
}

/* Distance XZ approchee (alpha max + beta min, ~4 % d'erreur, zero
 * racine carree) sur les positions monde de la derniere frame. */
static int32_t DistXZ(const Entity* a, const Entity* b)
{
	int32_t dx = a->world.t[0] - b->world.t[0];
	int32_t dz = a->world.t[2] - b->world.t[2];
	if (dx < 0) dx = -dx;
	if (dz < 0) dz = -dz;
	return dx > dz ? dx + (dz * 3 >> 3) : dz + (dx * 3 >> 3);
}

static void Run(Scene* scene, VmInstance* in, uint16_t pc)
{
	const uint8_t* blob = in->blob;
	const int32_t* K = (const int32_t*)(blob + PSB_HEADER);
	const uint32_t* code =
		(const uint32_t*)(blob + PSB_HEADER + PSB_CONSTS(blob) * 4);
	const char* strings = (const char*)(code + PSB_CODELEN(blob));
	int32_t* R = in->regs;
	int depth = 0;
	int steps = VM_MAX_STEPS;

	for (;;)
	{
		if (--steps < 0)
			return;
		uint32_t i = code[pc++];
		uint8_t op = (uint8_t)(i >> 24);
		uint8_t a = (uint8_t)(i >> 16);
		uint8_t b = (uint8_t)(i >> 8);
		uint8_t c = (uint8_t)i;
		uint16_t imm = (uint16_t)i;
		Entity* e;

		switch (op)
		{
		case OP_NOP: break;
		case OP_RET: return;
		case OP_LOADI: R[a] = (int16_t)imm; break;
		case OP_LOADK: R[a] = K[b]; break;
		case OP_MOV: R[a] = R[b]; break;
		case OP_ADD: R[a] = R[b] + R[c]; break;
		case OP_SUB: R[a] = R[b] - R[c]; break;
		case OP_MUL: R[a] = R[b] * R[c]; break;
		case OP_DIV: R[a] = R[c] != 0 ? R[b] / R[c] : 0; break;
		case OP_MOD: R[a] = R[c] != 0 ? R[b] % R[c] : 0; break;
		case OP_NEG: R[a] = -R[b]; break;
		case OP_LT: R[a] = R[b] < R[c]; break;
		case OP_LE: R[a] = R[b] <= R[c]; break;
		case OP_EQ: R[a] = R[b] == R[c]; break;
		case OP_NE: R[a] = R[b] != R[c]; break;
		case OP_AND: R[a] = R[b] && R[c]; break;
		case OP_OR: R[a] = R[b] || R[c]; break;
		case OP_NOT: R[a] = !R[b]; break;
		case OP_JMP: pc = imm; break;
		case OP_JZ: if (!R[a]) pc = imm; break;
		case OP_SELF: R[a] = in->entity; break;
		case OP_GETPOS:
			e = EntityAt(scene, R[b]);
			R[a] = !e ? 0 : c == 0 ? e->pos.vx : c == 1 ? e->pos.vy : e->pos.vz;
			break;
		case OP_SETPOS:
			e = EntityAt(scene, R[a]);
			if (e)
			{
				if (b == 0) e->pos.vx = R[c];
				else if (b == 1) e->pos.vy = R[c];
				else e->pos.vz = R[c];
			}
			break;
		case OP_GETROTY:
			e = EntityAt(scene, R[b]);
			R[a] = e ? e->rot.vy : 0;
			break;
		case OP_SETROTY:
			e = EntityAt(scene, R[a]);
			if (e) e->rot.vy = (int16_t)R[b];
			break;
		case OP_ADDROTY:
			e = EntityAt(scene, R[a]);
			if (e) e->rot.vy = (int16_t)(e->rot.vy + R[b]);
			break;
		case OP_MOVE:
			e = EntityAt(scene, R[a]);
			if (e) Physics_MoveAndSlide(e, R[b], R[c]);
			break;
		case OP_HELD: R[a] = (Input_Held() & imm & 0xFFFF) != 0; break;
		case OP_PRESSED: R[a] = (Input_Pressed() & imm & 0xFFFF) != 0; break;
		case OP_DIST:
		{
			Entity* e1 = EntityAt(scene, R[b]);
			Entity* e2 = EntityAt(scene, R[c]);
			R[a] = (e1 && e2) ? DistXZ(e1, e2) : 0x7FFFFFFF;
			break;
		}
		case OP_FIND:
		{
			uint32_t hash = (uint32_t)K[b];
			R[a] = -1;
			for (int j = 0; j < scene->entity_count; j++)
			{
				const Entity* cand = &scene->entities[j];
				if (cand->script != 0 &&
					scene->script_hashes[cand->script - 1] == hash)
				{
					R[a] = j;
					break;
				}
			}
			break;
		}
		case OP_DIALOG: Dialog_Show(strings + K[a]); break;
		case OP_DLGOPEN: R[a] = Dialog_IsOpen(); break;
		case OP_DLGCLOSE: Dialog_Close(); break;
		case OP_SHOW:
			e = EntityAt(scene, R[a]);
			if (e) e->visible = R[b] != 0;
			break;
		case OP_SWITCH: g_scene_switch_request = 1; break;
		case OP_RAND:
			rng_state = rng_state * 1103515245u + 12345u;
			R[a] = R[b] > 0 ? (int32_t)((rng_state >> 16) % (uint32_t)R[b]) : 0;
			break;
		case OP_ENTER: break; /* marqueur, consomme a l'appel */
		case OP_CALL:
		{
			if (depth >= VM_CALL_DEPTH)
			{
				/* Recursion trop profonde : l'appel renvoie 0. */
				R[a] = 0;
				break;
			}
			uint8_t nparams = (uint8_t)(code[imm] >> 16);
			int32_t* frame = frame_pool[depth];
			call_stack[depth].ret_pc = pc;
			call_stack[depth].ret_slot = a;
			call_stack[depth].regs = R;
			depth++;
			memset(frame, 0, sizeof(frame_pool[0]));
			for (int k = 0; k < nparams && k < VM_REGS; k++)
				frame[k] = R[a + k];
			R = frame;
			pc = imm + 1; /* saute le OP_ENTER */
			break;
		}
		case OP_RETV:
		case OP_RET0:
		{
			int32_t val = op == OP_RETV ? R[a] : 0;
			if (depth == 0)
				return; /* return au niveau d'un bloc : fin de tick */
			depth--;
			pc = call_stack[depth].ret_pc;
			R = call_stack[depth].regs;
			R[call_stack[depth].ret_slot] = val;
			break;
		}
		case OP_GLOAD: R[a] = in->regs[b]; break;
		case OP_GSTORE: in->regs[a] = R[b]; break;
		case OP_INITPUB:
			/* Valeur reglee dans l'inspecteur pour CETTE instance : elle
			 * ecrase le defaut du script (la table est courte, lue une
			 * seule fois au demarrage). */
			for (int j = 0; j < scene->script_value_count; j++)
			{
				const PscScriptValue* v = &scene->script_values[j];
				if (v->entity == in->entity && v->script == in->script &&
					v->field == b)
				{
					R[a] = v->value;
					break;
				}
			}
			break;
		default: return; /* opcode inconnu : on coupe, pas de plantage */
		}
	}
}

/* Une instance de VM par (entite, script bytecode) : une entite peut
 * porter plusieurs scripts (table de composants v1.6), chacun avec ses
 * propres registres. */
static void AddInstance(Scene* scene, int entity, int script_index)
{
	if (instance_count >= VM_MAX_INSTANCES || script_index <= 0)
		return;
	const uint8_t* blob = scene->vm_code[script_index - 1];
	if (!blob || memcmp(blob, "PSB2", 4) != 0)
		return;
	VmInstance* in = &instances[instance_count++];
	in->entity = (int16_t)entity;
	in->script = (int16_t)script_index;
	in->blob = blob;
	memset(in->regs, 0, sizeof(in->regs));
	if (PSB_START(blob) != PSB_NO_PC)
		Run(scene, in, PSB_START(blob));
}

void Vm_StartScripts(Scene* scene)
{
	instance_count = 0;
	if (scene->script_comp_count > 0)
	{
		for (int i = 0; i < scene->script_comp_count; i++)
		{
			const PscScriptComp* c = &scene->script_comps[i];
			if (c->entity < scene->entity_count)
				AddInstance(scene, c->entity, c->script);
		}
		return;
	}
	for (int i = 0; i < scene->entity_count; i++)
		AddInstance(scene, i, scene->entities[i].script);
}

void Vm_Tick(Scene* scene, int frozen)
{
	if (frozen)
		return;
	for (int i = 0; i < instance_count; i++)
	{
		uint16_t pc = PSB_UPDATE(instances[i].blob);
		if (pc != PSB_NO_PC)
			Run(scene, &instances[i], pc);
	}
}
