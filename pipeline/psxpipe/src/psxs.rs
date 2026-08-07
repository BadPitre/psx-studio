//! PSX Script : le langage de gameplay sans C (docs/PSX-SCRIPT.md).
//!
//! Un `.psxs` est compilé ici en bytecode « PSB1 » embarqué dans le
//! `.psc` (comme les assets) et interprété par la petite VM du runtime
//! (`engine/vm.c`). Choix orientés console : **entiers uniquement**
//! (unités monde, angles 4096 = un tour), **registres à emplacements
//! fixes** décidés à la compilation (zéro allocation, zéro GC), les
//! opérations lourdes restent des appels natifs du moteur.
//!
//! Le langage (mots-clés anglais, volontairement minimal) :
//!
//! ```text
//! # spinner : tourne, et salue le joueur proche
//! var speed = 12
//!
//! on start
//!     speed = 20
//! end
//!
//! every frame
//!     rotate_y(self, speed)
//!     if distance(self, find("player")) < 120 and pressed(CROSS) then
//!         dialog("BONJOUR !")
//!     end
//! end
//! ```

use std::collections::HashMap;

/* ------------------------------------------------------------ opcodes -- */
/* Instruction 32 bits : op(8) | a(8) | b(8) | c(8). Les sauts et
 * immédiats 16 bits utilisent b<<8|c. Miroir C : engine/vm.c. */

pub const OP_NOP: u8 = 0;
pub const OP_RET: u8 = 1;
pub const OP_LOADI: u8 = 2; // r, imm16 signé
pub const OP_LOADK: u8 = 3; // r, index constante 32 bits
pub const OP_MOV: u8 = 4;
pub const OP_ADD: u8 = 5;
pub const OP_SUB: u8 = 6;
pub const OP_MUL: u8 = 7;
pub const OP_DIV: u8 = 8;
pub const OP_MOD: u8 = 9;
pub const OP_NEG: u8 = 10;
pub const OP_LT: u8 = 11;
pub const OP_LE: u8 = 12;
pub const OP_EQ: u8 = 13;
pub const OP_NE: u8 = 14;
pub const OP_AND: u8 = 15;
pub const OP_OR: u8 = 16;
pub const OP_NOT: u8 = 17;
pub const OP_JMP: u8 = 18; // addr16 absolue (index d'instruction)
pub const OP_JZ: u8 = 19; // r, addr16
pub const OP_SELF: u8 = 21;
pub const OP_GETPOS: u8 = 22; // r, e, axe
pub const OP_SETPOS: u8 = 23; // e, axe, s
pub const OP_GETROTY: u8 = 24;
pub const OP_SETROTY: u8 = 25;
pub const OP_ADDROTY: u8 = 26;
pub const OP_MOVE: u8 = 27; // e, sx, sz (collisions)
pub const OP_HELD: u8 = 28; // r, masque16
pub const OP_PRESSED: u8 = 29; // r, masque16
pub const OP_DIST: u8 = 30;
pub const OP_FIND: u8 = 31; // r, index constante (hash de script)
pub const OP_DIALOG: u8 = 32; // index constante (offset de chaîne)
pub const OP_DLGOPEN: u8 = 33;
pub const OP_DLGCLOSE: u8 = 34;
pub const OP_SHOW: u8 = 35; // e, s
pub const OP_SWITCH: u8 = 36;
pub const OP_RAND: u8 = 37; // r, s -> 0..s-1
/* Fonctions utilisateur : appels a frames (pile statique de la VM). */
pub const OP_ENTER: u8 = 38; // marqueur de tete de fonction, a = nb params
pub const OP_CALL: u8 = 39; // abase, addr16 — args dans abase.., resultat dans abase
pub const OP_RETV: u8 = 40; // r
pub const OP_RET0: u8 = 41;
pub const OP_GLOAD: u8 = 42; // r local <- champ global b
pub const OP_GSTORE: u8 = 43; // champ global a <- r local b
/// Champ `public` : si l'instance porte une surcharge (valeur réglée
/// dans l'inspecteur), l'écrit dans le registre — sinon laisse la valeur
/// par défaut du script. a = registre, b = index du champ public.
pub const OP_INITPUB: u8 = 44;
/// Caméra : 5 registres consécutifs (x, y, z, yaw, pitch) à partir de a.
pub const OP_CAMERA: u8 = 45;
/// Trigonométrie 4.12 (4096 = 1.0), angle en unités projet (4096 = tour).
pub const OP_SIN: u8 = 46;
pub const OP_COS: u8 = 47;
/// Caméra : l'entité du registre a devient la vue (elle garde son FOV et
/// sa distance de rendu réglés dans l'inspecteur).
pub const OP_CAMENT: u8 = 48;
/// Tangage (rotation X) : lire / écrire, positif = regard vers le haut.
pub const OP_GETROTX: u8 = 49;
pub const OP_SETROTX: u8 = 50;
/// Stick analogique : b = axe (0 = X gauche, 1 = Y gauche, 2 = X droit,
/// 3 = Y droit), résultat -128..127 (0 sur une manette numérique).
pub const OP_AXIS: u8 = 51;
/// 1 si la manette est en mode analogique (sticks lisibles).
pub const OP_ANALOG: u8 = 52;

pub const REG_COUNT: usize = 16;
const NO_PC: u16 = 0xFFFF;

/// Noms réservés du moteur (une `function` ne peut pas les redéfinir).
const BUILTINS: [&str; 30] = [
    "pos_x", "pos_y", "pos_z", "set_x", "set_y", "set_z", "rot_y", "set_rot_y",
    "rotate_y", "rot_x", "set_rot_x", "move", "held", "pressed", "distance",
    "find", "dialog", "dialog_open", "close_dialog", "show", "switch_scene",
    "random", "camera", "sin", "cos",
    "lstick_x", "lstick_y", "rstick_x", "rstick_y", "analog",
];

/* Boutons : masques matériels PS1 (psxpad.h), stables à jamais. */
const BUTTONS: [(&str, u16); 14] = [
    ("SELECT", 0x0001),
    ("START", 0x0008),
    ("L2", 0x0100),
    ("R2", 0x0200),
    ("L1", 0x0400),
    ("R1", 0x0800),
    ("UP", 0x0010),
    ("RIGHT", 0x0020),
    ("DOWN", 0x0040),
    ("LEFT", 0x0080),
    ("TRIANGLE", 0x1000),
    ("CIRCLE", 0x2000),
    ("CROSS", 0x4000),
    ("SQUARE", 0x8000),
];

/* -------------------------------------------------------------- lexer -- */

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Int(i32),
    Ident(String),
    Str(String),
    Sym(&'static str),
    NewLine,
}

struct Lexer {
    toks: Vec<(Tok, u32)>,
}

fn lex(src: &str) -> Result<Lexer, String> {
    let mut toks = Vec::new();
    for (li, raw) in src.lines().enumerate() {
        let line = li as u32 + 1;
        let mut it = raw.char_indices().peekable();
        let mut pushed = false;
        while let Some(&(i, c)) = it.peek() {
            match c {
                '#' => break, // commentaire jusqu'à la fin de ligne
                ' ' | '\t' | '\r' => {
                    it.next();
                }
                '"' => {
                    it.next();
                    let mut s = String::new();
                    loop {
                        match it.next() {
                            Some((_, '"')) => break,
                            Some((_, '\n')) | None => {
                                return Err(format!("ligne {line}: chaîne non fermée"))
                            }
                            Some((_, ch)) => s.push(ch),
                        }
                    }
                    toks.push((Tok::Str(s), line));
                    pushed = true;
                }
                '0'..='9' => {
                    let mut end = i;
                    while let Some(&(j, d)) = it.peek() {
                        if d.is_ascii_digit() {
                            end = j;
                            it.next();
                        } else {
                            break;
                        }
                    }
                    let text = &raw[i..=end];
                    let v: i32 = text
                        .parse()
                        .map_err(|_| format!("ligne {line}: nombre invalide '{text}'"))?;
                    toks.push((Tok::Int(v), line));
                    pushed = true;
                }
                c if c.is_alphabetic() || c == '_' => {
                    let mut end = i;
                    while let Some(&(j, d)) = it.peek() {
                        if d.is_alphanumeric() || d == '_' {
                            end = j;
                            it.next();
                        } else {
                            break;
                        }
                    }
                    toks.push((Tok::Ident(raw[i..=end].to_string()), line));
                    pushed = true;
                }
                _ => {
                    it.next();
                    let two = if let Some(&(_, n)) = it.peek() {
                        match (c, n) {
                            ('<', '=') => Some("<="),
                            ('>', '=') => Some(">="),
                            ('=', '=') => Some("=="),
                            ('!', '=') => Some("!="),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(sym) = two {
                        it.next();
                        toks.push((Tok::Sym(sym), line));
                    } else {
                        let sym = match c {
                            '+' => "+",
                            '-' => "-",
                            '*' => "*",
                            '/' => "/",
                            '%' => "%",
                            '(' => "(",
                            ')' => ")",
                            ',' => ",",
                            ':' => ":",
                            '<' => "<",
                            '>' => ">",
                            '=' => "=",
                            other => {
                                return Err(format!(
                                    "ligne {line}: caractère inattendu '{other}'"
                                ))
                            }
                        };
                        toks.push((Tok::Sym(sym), line));
                    }
                    pushed = true;
                }
            }
        }
        if pushed {
            toks.push((Tok::NewLine, line));
        }
    }
    Ok(Lexer { toks })
}

/* ------------------------------------------------------------- parser -- */

#[derive(Debug)]
enum Expr {
    Int(i32),
    Var(String, u32),
    Bin(&'static str, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Call(String, Vec<Arg>, u32),
    Me,
    Button(u16),
    Nil,
}

#[derive(Debug)]
enum Arg {
    Expr(Expr),
    Str(String),
}

#[derive(Debug)]
enum Stmt {
    Assign(String, Expr, u32),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    Call(String, Vec<Arg>, u32),
    Return(Option<Expr>, u32),
}

/// Une `function nom(params)` : les locales (`var` en tête de corps) et
/// les paramètres vivent dans la frame d'appel, pas dans les champs.
struct FuncDef {
    name: String,
    params: Vec<String>,
    locals: Vec<(String, Option<Expr>, u32)>,
    body: Vec<Stmt>,
    line: u32,
}

struct Parser {
    toks: Vec<(Tok, u32)>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }
    fn line(&self) -> u32 {
        self.toks
            .get(self.pos.min(self.toks.len().saturating_sub(1)))
            .map(|(_, l)| *l)
            .unwrap_or(0)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|(t, _)| t.clone());
        self.pos += 1;
        t
    }
    fn eat_newlines(&mut self) {
        while matches!(self.peek(), Some(Tok::NewLine)) {
            self.pos += 1;
        }
    }
    fn expect_sym(&mut self, s: &str) -> Result<(), String> {
        match self.next() {
            Some(Tok::Sym(t)) if t == s => Ok(()),
            _ => Err(format!("ligne {}: « {s} » attendu", self.line())),
        }
    }
    fn expect_kw(&mut self, kw: &str) -> Result<(), String> {
        match self.next() {
            Some(Tok::Ident(t)) if t == kw => Ok(()),
            _ => Err(format!("ligne {}: « {kw} » attendu", self.line())),
        }
    }
    fn end_of_stmt(&mut self) -> Result<(), String> {
        match self.next() {
            Some(Tok::NewLine) | None => Ok(()),
            _ => Err(format!(
                "ligne {}: fin de ligne attendue (une instruction par ligne)",
                self.line()
            )),
        }
    }

    /// `fin` termine le bloc ; `sinon` le termine aussi quand `allow_else`.
    fn parse_block(&mut self, allow_else: bool) -> Result<(Vec<Stmt>, bool), String> {
        let mut out = Vec::new();
        loop {
            self.eat_newlines();
            match self.peek() {
                None => {
                    return Err(format!(
                        "ligne {}: « end » manquant avant la fin du fichier",
                        self.line()
                    ))
                }
                Some(Tok::Ident(k)) if k == "end" => {
                    self.pos += 1;
                    return Ok((out, false));
                }
                Some(Tok::Ident(k)) if allow_else && k == "else" => {
                    self.pos += 1;
                    return Ok((out, true));
                }
                _ => out.push(self.parse_stmt()?),
            }
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        let line = self.line();
        match self.next() {
            Some(Tok::Ident(name)) => match name.as_str() {
                "return" => {
                    if matches!(self.peek(), Some(Tok::NewLine) | None) {
                        self.end_of_stmt()?;
                        return Ok(Stmt::Return(None, line));
                    }
                    let e = self.parse_expr()?;
                    self.end_of_stmt()?;
                    Ok(Stmt::Return(Some(e), line))
                }
                "var" => Err(format!(
                    "ligne {line}: « var » se déclare en tête de fichier (champs) \
                     ou en tête de function (locales)"
                )),
                "if" => {
                    let cond = self.parse_expr()?;
                    self.expect_kw("then")?;
                    self.end_of_stmt()?;
                    let (then, has_else) = self.parse_block(true)?;
                    let els = if has_else {
                        self.end_of_stmt()?;
                        self.parse_block(false)?.0
                    } else {
                        Vec::new()
                    };
                    Ok(Stmt::If(cond, then, els))
                }
                "while" => {
                    let cond = self.parse_expr()?;
                    self.expect_kw("do")?;
                    self.end_of_stmt()?;
                    let (body, _) = self.parse_block(false)?;
                    Ok(Stmt::While(cond, body))
                }
                _ => match self.peek() {
                    Some(Tok::Sym("=")) => {
                        self.pos += 1;
                        let e = self.parse_expr()?;
                        self.end_of_stmt()?;
                        Ok(Stmt::Assign(name, e, line))
                    }
                    Some(Tok::Sym("(")) => {
                        self.pos += 1;
                        let args = self.parse_args()?;
                        self.end_of_stmt()?;
                        Ok(Stmt::Call(name, args, line))
                    }
                    _ => Err(format!(
                        "ligne {line}: « = » ou « ( » attendu après « {name} »"
                    )),
                },
            },
            _ => Err(format!("ligne {line}: instruction attendue")),
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Arg>, String> {
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Tok::Sym(")"))) {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            if let Some(Tok::Str(s)) = self.peek() {
                args.push(Arg::Str(s.clone()));
                self.pos += 1;
            } else {
                args.push(Arg::Expr(self.parse_expr()?));
            }
            match self.next() {
                Some(Tok::Sym(",")) => {}
                Some(Tok::Sym(")")) => return Ok(args),
                _ => return Err(format!("ligne {}: « , » ou « ) » attendu", self.line())),
            }
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_and()?;
        while matches!(self.peek(), Some(Tok::Ident(k)) if k == "or") {
            self.pos += 1;
            e = Expr::Bin("or", Box::new(e), Box::new(self.parse_and()?));
        }
        Ok(e)
    }
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_cmp()?;
        while matches!(self.peek(), Some(Tok::Ident(k)) if k == "and") {
            self.pos += 1;
            e = Expr::Bin("and", Box::new(e), Box::new(self.parse_cmp()?));
        }
        Ok(e)
    }
    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let e = self.parse_add()?;
        for op in ["<=", ">=", "==", "!=", "<", ">"] {
            if matches!(self.peek(), Some(Tok::Sym(s)) if *s == op) {
                self.pos += 1;
                let rhs = self.parse_add()?;
                return Ok(Expr::Bin(op, Box::new(e), Box::new(rhs)));
            }
        }
        Ok(e)
    }
    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_mul()?;
        loop {
            match self.peek() {
                Some(Tok::Sym("+")) => {
                    self.pos += 1;
                    e = Expr::Bin("+", Box::new(e), Box::new(self.parse_mul()?));
                }
                Some(Tok::Sym("-")) => {
                    self.pos += 1;
                    e = Expr::Bin("-", Box::new(e), Box::new(self.parse_mul()?));
                }
                _ => return Ok(e),
            }
        }
    }
    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::Sym("*")) => {
                    self.pos += 1;
                    e = Expr::Bin("*", Box::new(e), Box::new(self.parse_unary()?));
                }
                Some(Tok::Sym("/")) => {
                    self.pos += 1;
                    e = Expr::Bin("/", Box::new(e), Box::new(self.parse_unary()?));
                }
                Some(Tok::Sym("%")) => {
                    self.pos += 1;
                    e = Expr::Bin("%", Box::new(e), Box::new(self.parse_unary()?));
                }
                _ => return Ok(e),
            }
        }
    }
    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Tok::Sym("-")) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.parse_unary()?)))
            }
            Some(Tok::Ident(k)) if k == "not" => {
                self.pos += 1;
                Ok(Expr::Not(Box::new(self.parse_unary()?)))
            }
            _ => self.parse_primary(),
        }
    }
    fn parse_primary(&mut self) -> Result<Expr, String> {
        let line = self.line();
        match self.next() {
            Some(Tok::Int(v)) => Ok(Expr::Int(v)),
            Some(Tok::Sym("(")) => {
                let e = self.parse_expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                if name == "self" {
                    return Ok(Expr::Me);
                }
                if name == "nil" {
                    return Ok(Expr::Nil);
                }
                if let Some((_, mask)) = BUTTONS.iter().find(|(n, _)| *n == name) {
                    return Ok(Expr::Button(*mask));
                }
                if matches!(self.peek(), Some(Tok::Sym("("))) {
                    self.pos += 1;
                    let args = self.parse_args()?;
                    Ok(Expr::Call(name, args, line))
                } else {
                    Ok(Expr::Var(name, line))
                }
            }
            _ => Err(format!("ligne {line}: expression attendue")),
        }
    }
}

/* ------------------------------------------------------------ codegen -- */

/// Type d'un champ `public` (widget de l'inspecteur).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PubType {
    /// Entier (unités monde, angles, compteurs).
    Int,
    /// Case à cocher (0/1).
    Bool,
    /// Référence à une entité de la scène (GameObject, caméra, lumière…) :
    /// la valeur stockée est l'index d'entité résolu au build.
    Entity,
}

impl PubType {
    fn parse(name: &str) -> Option<PubType> {
        match name {
            "int" | "number" => Some(PubType::Int),
            "bool" => Some(PubType::Bool),
            "entity" | "object" => Some(PubType::Entity),
            _ => None,
        }
    }
}

/// Un champ exposé à l'inspecteur (`public var vitesse = 8`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PubField {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: PubType,
    /// Valeur par défaut littérale (si l'init est une constante).
    pub default: i32,
}

#[derive(Debug)]
pub struct Compiled {
    pub bytecode: Vec<u8>,
    pub reg_used: usize,
    /// Champs `public`, dans l'ordre de déclaration (index = celui du
    /// bytecode et des surcharges de scène).
    pub fields: Vec<PubField>,
}

struct Gen {
    vars: HashMap<String, u8>,
    consts: Vec<i32>,
    strings: Vec<u8>,
    code: Vec<u32>,
    temp_base: u8,
    temp: u8,
    /* Fonctions utilisateur : Some(map) = on compile un corps de
     * function (params + locales -> slots de frame ; les champs `var`
     * passent alors par GLOAD/GSTORE). */
    locals: Option<HashMap<String, u8>>,
    func_arity: HashMap<String, u8>,
    func_addr: HashMap<String, u16>,
    patches: Vec<(usize, String, u32)>,
}

fn insn(op: u8, a: u8, b: u8, c: u8) -> u32 {
    (op as u32) << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32
}
fn insn16(op: u8, a: u8, imm: u16) -> u32 {
    (op as u32) << 24 | (a as u32) << 16 | imm as u32
}

impl Gen {
    fn konst(&mut self, v: i32) -> Result<u8, String> {
        if let Some(i) = self.consts.iter().position(|&k| k == v) {
            return Ok(i as u8);
        }
        if self.consts.len() >= 256 {
            return Err("trop de constantes (max 256)".into());
        }
        self.consts.push(v);
        Ok((self.consts.len() - 1) as u8)
    }

    fn string(&mut self, s: &str) -> Result<u8, String> {
        let off = self.strings.len() as i32;
        // Le charset des polices .fnt est en majuscules ASCII.
        for ch in s.chars() {
            let b = match ch {
                'à' | 'â' | 'ä' => b'A',
                'é' | 'è' | 'ê' | 'ë' => b'E',
                'î' | 'ï' => b'I',
                'ô' | 'ö' => b'O',
                'ù' | 'û' | 'ü' => b'U',
                'ç' => b'C',
                c if (c as u32) < 128 => (c as u8).to_ascii_uppercase(),
                _ => b'?',
            };
            self.strings.push(b);
        }
        self.strings.push(0);
        if self.strings.len() > u16::MAX as usize {
            return Err("table de chaînes pleine (64 Ko)".into());
        }
        self.konst(off)
    }

    fn alloc_temp(&mut self, line: u32) -> Result<u8, String> {
        let r = self.temp_base + self.temp;
        if (r as usize) >= REG_COUNT {
            return Err(format!(
                "ligne {line}: expression trop complexe ou trop de variables \
                 (16 registres, {} pris par les var)",
                self.temp_base
            ));
        }
        self.temp += 1;
        Ok(r)
    }

    fn emit_expr(&mut self, e: &Expr, dst: u8) -> Result<(), String> {
        match e {
            Expr::Int(v) => {
                if *v >= i16::MIN as i32 && *v <= i16::MAX as i32 {
                    self.code.push(insn16(OP_LOADI, dst, *v as u16));
                } else {
                    let k = self.konst(*v)?;
                    self.code.push(insn(OP_LOADK, dst, k, 0));
                }
            }
            Expr::Nil => self.code.push(insn16(OP_LOADI, dst, (-1i16) as u16)),
            Expr::Me => self.code.push(insn(OP_SELF, dst, 0, 0)),
            Expr::Button(_) => {
                return Err(
                    "un bouton (CROSS...) ne s'utilise que dans held()/pressed()"
                        .into(),
                )
            }
            Expr::Var(name, line) => {
                if let Some(&r) = self.locals.as_ref().and_then(|l| l.get(name)) {
                    if r != dst {
                        self.code.push(insn(OP_MOV, dst, r, 0));
                    }
                } else if let Some(&g) = self.vars.get(name) {
                    if self.locals.is_some() {
                        // Champ lu depuis une function : registre global.
                        self.code.push(insn(OP_GLOAD, dst, g, 0));
                    } else if g != dst {
                        self.code.push(insn(OP_MOV, dst, g, 0));
                    }
                } else {
                    return Err(format!(
                        "ligne {line}: variable inconnue '{name}' (déclare-la avec « var »)"
                    ));
                }
            }
            Expr::Neg(inner) => {
                self.emit_expr(inner, dst)?;
                self.code.push(insn(OP_NEG, dst, dst, 0));
            }
            Expr::Not(inner) => {
                self.emit_expr(inner, dst)?;
                self.code.push(insn(OP_NOT, dst, dst, 0));
            }
            Expr::Bin(op, lhs, rhs) => {
                let saved = self.temp;
                self.emit_expr(lhs, dst)?;
                let rt = self.alloc_temp(0)?;
                self.emit_expr(rhs, rt)?;
                let (opc, a, b) = match *op {
                    "+" => (OP_ADD, dst, rt),
                    "-" => (OP_SUB, dst, rt),
                    "*" => (OP_MUL, dst, rt),
                    "/" => (OP_DIV, dst, rt),
                    "%" => (OP_MOD, dst, rt),
                    "<" => (OP_LT, dst, rt),
                    "<=" => (OP_LE, dst, rt),
                    ">" => (OP_LT, rt, dst),
                    ">=" => (OP_LE, rt, dst),
                    "==" => (OP_EQ, dst, rt),
                    "!=" => (OP_NE, dst, rt),
                    "and" => (OP_AND, dst, rt),
                    "or" => (OP_OR, dst, rt),
                    other => return Err(format!("opérateur interne inconnu {other}")),
                };
                self.code.push(insn(opc, dst, a, b));
                self.temp = saved;
            }
            Expr::Call(name, args, line) => self.emit_call(name, args, Some(dst), *line)?,
        }
        Ok(())
    }

    /// Évalue une expression-argument dans un temporaire.
    fn arg_reg(&mut self, args: &[Arg], i: usize, name: &str, line: u32) -> Result<u8, String> {
        match args.get(i) {
            Some(Arg::Expr(e)) => {
                let r = self.alloc_temp(line)?;
                self.emit_expr(e, r)?;
                Ok(r)
            }
            _ => Err(format!(
                "ligne {line}: {name}() attend un nombre/une entité en argument {}",
                i + 1
            )),
        }
    }

    fn arity(name: &str, args: &[Arg], n: usize, line: u32) -> Result<(), String> {
        if args.len() != n {
            return Err(format!(
                "ligne {line}: {name}() attend {n} argument{}",
                if n > 1 { "s" } else { "" }
            ));
        }
        Ok(())
    }

    fn emit_call(
        &mut self,
        name: &str,
        args: &[Arg],
        dst: Option<u8>,
        line: u32,
    ) -> Result<(), String> {
        let saved = self.temp;
        let need_dst = |dst: Option<u8>| -> Result<u8, String> {
            dst.ok_or(format!(
                "ligne {line}: {name}() renvoie une valeur — utilise-la (x = ..., si ...)"
            ))
        };
        match name {
            "pos_x" | "pos_y" | "pos_z" => {
                Self::arity(name, args, 1, line)?;
                let d = need_dst(dst)?;
                let e = self.arg_reg(args, 0, name, line)?;
                let axis = match name {
                    "pos_x" => 0,
                    "pos_y" => 1,
                    _ => 2,
                };
                self.code.push(insn(OP_GETPOS, d, e, axis));
            }
            "set_x" | "set_y" | "set_z" => {
                Self::arity(name, args, 2, line)?;
                let e = self.arg_reg(args, 0, name, line)?;
                let v = self.arg_reg(args, 1, name, line)?;
                let axis = match name {
                    "set_x" => 0,
                    "set_y" => 1,
                    _ => 2,
                };
                self.code.push(insn(OP_SETPOS, e, axis, v));
            }
            "rot_y" => {
                Self::arity(name, args, 1, line)?;
                let d = need_dst(dst)?;
                let e = self.arg_reg(args, 0, name, line)?;
                self.code.push(insn(OP_GETROTY, d, e, 0));
            }
            "set_rot_y" | "rotate_y" => {
                Self::arity(name, args, 2, line)?;
                let e = self.arg_reg(args, 0, name, line)?;
                let v = self.arg_reg(args, 1, name, line)?;
                let opc = if name == "rotate_y" { OP_ADDROTY } else { OP_SETROTY };
                self.code.push(insn(opc, e, v, 0));
            }
            "rot_x" => {
                Self::arity(name, args, 1, line)?;
                let d = need_dst(dst)?;
                let e = self.arg_reg(args, 0, name, line)?;
                self.code.push(insn(OP_GETROTX, d, e, 0));
            }
            "set_rot_x" => {
                Self::arity(name, args, 2, line)?;
                let e = self.arg_reg(args, 0, name, line)?;
                let v = self.arg_reg(args, 1, name, line)?;
                self.code.push(insn(OP_SETROTX, e, v, 0));
            }
            "move" => {
                Self::arity(name, args, 3, line)?;
                let e = self.arg_reg(args, 0, name, line)?;
                let dx = self.arg_reg(args, 1, name, line)?;
                let dz = self.arg_reg(args, 2, name, line)?;
                self.code.push(insn(OP_MOVE, e, dx, dz));
            }
            "held" | "pressed" => {
                Self::arity(name, args, 1, line)?;
                let d = need_dst(dst)?;
                let mask = match args.first() {
                    Some(Arg::Expr(Expr::Button(m))) => *m,
                    _ => {
                        return Err(format!(
                            "ligne {line}: {name}() attend un bouton \
                             (CROSS, CIRCLE, SQUARE, TRIANGLE, UP, DOWN, LEFT, RIGHT, START, SELECT)"
                        ))
                    }
                };
                let opc = if name == "held" { OP_HELD } else { OP_PRESSED };
                self.code.push(insn16(opc, d, mask));
            }
            /* Sticks analogiques : -128..127, 0 au repos (et 0 sur une
             * manette numérique — un script marche dans les deux cas). */
            "lstick_x" | "lstick_y" | "rstick_x" | "rstick_y" => {
                Self::arity(name, args, 0, line)?;
                let d = need_dst(dst)?;
                let axis = match name {
                    "lstick_x" => 0,
                    "lstick_y" => 1,
                    "rstick_x" => 2,
                    _ => 3,
                };
                self.code.push(insn(OP_AXIS, d, axis, 0));
            }
            "analog" => {
                Self::arity(name, args, 0, line)?;
                let d = need_dst(dst)?;
                self.code.push(insn(OP_ANALOG, d, 0, 0));
            }
            "distance" => {
                Self::arity(name, args, 2, line)?;
                let d = need_dst(dst)?;
                let a = self.arg_reg(args, 0, name, line)?;
                let b = self.arg_reg(args, 1, name, line)?;
                self.code.push(insn(OP_DIST, d, a, b));
            }
            "find" => {
                Self::arity(name, args, 1, line)?;
                let d = need_dst(dst)?;
                let s = match args.first() {
                    Some(Arg::Str(s)) => s.clone(),
                    _ => {
                        return Err(format!(
                            "ligne {line}: find() attend un nom de script entre guillemets"
                        ))
                    }
                };
                let k = self.konst(crate::scene::script_hash(&s) as i32)?;
                self.code.push(insn(OP_FIND, d, k, 0));
            }
            "dialog" => {
                Self::arity(name, args, 1, line)?;
                let s = match args.first() {
                    Some(Arg::Str(s)) => s.clone(),
                    _ => {
                        return Err(format!(
                            "ligne {line}: dialog() attend un texte entre guillemets \
                             (3 lignes max, séparées par /)"
                        ))
                    }
                };
                let k = self.string(&s.replace(" / ", "\n").replace('/', "\n"))?;
                self.code.push(insn(OP_DIALOG, k, 0, 0));
            }
            "dialog_open" => {
                Self::arity(name, args, 0, line)?;
                let d = need_dst(dst)?;
                self.code.push(insn(OP_DLGOPEN, d, 0, 0));
            }
            "close_dialog" => {
                Self::arity(name, args, 0, line)?;
                self.code.push(insn(OP_DLGCLOSE, 0, 0, 0));
            }
            "show" => {
                Self::arity(name, args, 2, line)?;
                let e = self.arg_reg(args, 0, name, line)?;
                let v = self.arg_reg(args, 1, name, line)?;
                self.code.push(insn(OP_SHOW, e, v, 0));
            }
            "switch_scene" => {
                Self::arity(name, args, 0, line)?;
                self.code.push(insn(OP_SWITCH, 0, 0, 0));
            }
            /* Une entité de la scène devient la vue : elle garde le FOV et
             * la distance de rendu réglés dans l'inspecteur, et le script
             * n'a qu'à la placer (set_x/set_rot_y/set_rot_x). */
            "camera" if args.len() == 1 => {
                let e = self.arg_reg(args, 0, name, line)?;
                self.code.push(insn(OP_CAMENT, e, 0, 0));
            }
            "camera" => {
                if args.len() != 5 {
                    return Err(format!(
                        "ligne {line}: camera() attend une entité caméra, \
                         ou 5 nombres (x, y, z, yaw, pitch)"
                    ));
                }
                /* 5 registres CONSECUTIFS (x, y, z, yaw, pitch), comme
                 * une frame d'appel : la VM les lit d'un bloc. */
                let abase = self.temp_base + self.temp;
                for k in 0..5 {
                    let r = self.alloc_temp(line)?;
                    debug_assert_eq!(r, abase + k as u8);
                    match &args[k] {
                        Arg::Expr(e) => self.emit_expr(e, r)?,
                        Arg::Str(_) => {
                            return Err(format!(
                                "ligne {line}: camera() attend des nombres"
                            ))
                        }
                    }
                }
                self.code.push(insn(OP_CAMERA, abase, 0, 0));
            }
            "sin" | "cos" => {
                Self::arity(name, args, 1, line)?;
                let d = need_dst(dst)?;
                let a = self.arg_reg(args, 0, name, line)?;
                self.code
                    .push(insn(if name == "sin" { OP_SIN } else { OP_COS }, d, a, 0));
            }
            "random" => {
                Self::arity(name, args, 1, line)?;
                let d = need_dst(dst)?;
                let n = self.arg_reg(args, 0, name, line)?;
                self.code.push(insn(OP_RAND, d, n, 0));
            }
            other => {
                let Some(&np) = self.func_arity.get(other) else {
                    return Err(format!(
                        "ligne {line}: fonction inconnue '{other}' (ni du moteur, \
                         ni définie par « function » — voir docs/PSX-SCRIPT.md)"
                    ));
                };
                if args.len() != np as usize {
                    return Err(format!(
                        "ligne {line}: {other}() attend {np} argument{} ({} donné{})",
                        if np > 1 { "s" } else { "" },
                        args.len(),
                        if args.len() > 1 { "s" } else { "" },
                    ));
                }
                /* Convention d'appel : arguments évalués dans des
                 * temporaires CONSECUTIFS (abase..), le résultat revient
                 * dans abase. L'adresse est patchée après l'émission des
                 * fonctions (références avant définition permises). */
                let abase = self.temp_base + self.temp;
                for (k, a) in args.iter().enumerate() {
                    let r = self.alloc_temp(line)?;
                    debug_assert_eq!(r, abase + k as u8);
                    match a {
                        Arg::Expr(e) => self.emit_expr(e, r)?,
                        Arg::Str(_) => {
                            return Err(format!(
                                "ligne {line}: {other}() attend des nombres/entités, \
                                 pas une chaîne"
                            ))
                        }
                    }
                }
                if args.is_empty() {
                    self.alloc_temp(line)?; // slot du résultat
                }
                self.patches.push((self.code.len(), other.to_string(), line));
                self.code.push(insn16(OP_CALL, abase, 0));
                if let Some(d) = dst {
                    if d != abase {
                        self.code.push(insn(OP_MOV, d, abase, 0));
                    }
                }
            }
        }
        self.temp = saved;
        Ok(())
    }

    fn emit_stmts(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for s in stmts {
            match s {
                Stmt::Assign(name, e, line) => {
                    if let Some(&r) = self.locals.as_ref().and_then(|l| l.get(name)) {
                        self.emit_expr(e, r)?;
                    } else if let Some(&g) = self.vars.get(name) {
                        if self.locals.is_some() {
                            // Champ écrit depuis une function.
                            let saved = self.temp;
                            let t = self.alloc_temp(*line)?;
                            self.emit_expr(e, t)?;
                            self.code.push(insn(OP_GSTORE, g, t, 0));
                            self.temp = saved;
                        } else {
                            self.emit_expr(e, g)?;
                        }
                    } else {
                        return Err(format!(
                            "ligne {line}: variable inconnue '{name}' (déclare-la avec \
                             « var »)"
                        ));
                    }
                }
                Stmt::Return(value, line) => {
                    if self.locals.is_none() {
                        return Err(format!(
                            "ligne {line}: « return » ne s'utilise que dans une function"
                        ));
                    }
                    match value {
                        Some(e) => {
                            let saved = self.temp;
                            let t = self.alloc_temp(*line)?;
                            self.emit_expr(e, t)?;
                            self.code.push(insn(OP_RETV, t, 0, 0));
                            self.temp = saved;
                        }
                        None => self.code.push(insn(OP_RET0, 0, 0, 0)),
                    }
                }
                Stmt::Call(name, args, line) => self.emit_call(name, args, None, *line)?,
                Stmt::If(cond, then, els) => {
                    let saved = self.temp;
                    let c = self.alloc_temp(0)?;
                    self.emit_expr(cond, c)?;
                    self.temp = saved;
                    let jz_at = self.code.len();
                    self.code.push(0); // patché
                    self.emit_stmts(then)?;
                    if els.is_empty() {
                        let end = self.code.len() as u16;
                        self.code[jz_at] = insn16(OP_JZ, c, end);
                    } else {
                        let jmp_at = self.code.len();
                        self.code.push(0);
                        let else_pc = self.code.len() as u16;
                        self.code[jz_at] = insn16(OP_JZ, c, else_pc);
                        self.emit_stmts(els)?;
                        let end = self.code.len() as u16;
                        self.code[jmp_at] = insn16(OP_JMP, 0, end);
                    }
                }
                Stmt::While(cond, body) => {
                    let top = self.code.len() as u16;
                    let saved = self.temp;
                    let c = self.alloc_temp(0)?;
                    self.emit_expr(cond, c)?;
                    self.temp = saved;
                    let jz_at = self.code.len();
                    self.code.push(0);
                    self.emit_stmts(body)?;
                    self.code.push(insn16(OP_JMP, 0, top));
                    let end = self.code.len() as u16;
                    self.code[jz_at] = insn16(OP_JZ, c, end);
                }
            }
        }
        Ok(())
    }
}

/// Compile un source PSX Script en blob bytecode « PSB1 ».
pub fn compile(src: &str) -> Result<Compiled, String> {
    let lexer = lex(src)?;
    let mut p = Parser {
        toks: lexer.toks,
        pos: 0,
    };

    // Déclarations `var` + blocs + fonctions.
    let mut vars: Vec<(String, Option<Expr>, u32)> = Vec::new();
    /* Champs `public` : exposés dans l'inspecteur (nom, type, défaut). */
    let mut pubs: Vec<PubField> = Vec::new();
    let mut pub_regs: Vec<u8> = Vec::new();
    let mut start_block: Option<Vec<Stmt>> = None;
    let mut update_block: Option<Vec<Stmt>> = None;
    let mut funcs: Vec<FuncDef> = Vec::new();

    fn parse_var_decl(p: &mut Parser, line: u32) -> Result<(String, Option<Expr>, u32), String> {
        let name = match p.next() {
            Some(Tok::Ident(n)) => n,
            _ => return Err(format!("ligne {line}: nom de variable attendu")),
        };
        let init = if matches!(p.peek(), Some(Tok::Sym("="))) {
            p.pos += 1;
            Some(p.parse_expr()?)
        } else {
            None
        };
        p.end_of_stmt()?;
        Ok((name, init, line))
    }

    loop {
        p.eat_newlines();
        let line = p.line();
        match p.next() {
            None => break,
            Some(Tok::Ident(k)) if k == "var" || k == "public" => {
                let is_public = k == "public";
                if is_public {
                    match p.next() {
                        Some(Tok::Ident(v)) if v == "var" => {}
                        _ => {
                            return Err(format!(
                                "ligne {line}: « public » se met devant « var »                                  (public var vitesse = 8)"
                            ))
                        }
                    }
                }
                let name = match p.next() {
                    Some(Tok::Ident(n)) => n,
                    _ => return Err(format!("ligne {line}: nom de variable attendu")),
                };
                // Annotation de type facultative : `: int|bool|entity`.
                let mut kind = PubType::Int;
                if matches!(p.peek(), Some(Tok::Sym(":"))) {
                    p.pos += 1;
                    let tname = match p.next() {
                        Some(Tok::Ident(t)) => t,
                        _ => return Err(format!("ligne {line}: type attendu après « : »")),
                    };
                    kind = PubType::parse(&tname).ok_or(format!(
                        "ligne {line}: type '{tname}' inconnu (int, bool, entity)"
                    ))?;
                    if !is_public {
                        return Err(format!(
                            "ligne {line}: le type ne sert qu'aux champs « public »"
                        ));
                    }
                }
                let init = if matches!(p.peek(), Some(Tok::Sym("="))) {
                    p.pos += 1;
                    Some(p.parse_expr()?)
                } else if matches!(kind, PubType::Entity) {
                    // Un champ `entity` non réglé vaut `nil` (-1), pas
                    // l'entité 0 : `if target != nil then` doit être faux.
                    Some(Expr::Int(-1))
                } else {
                    None
                };
                p.end_of_stmt()?;
                if vars.iter().any(|(n, _, _)| *n == name) {
                    return Err(format!("ligne {line}: variable '{name}' déjà déclarée"));
                }
                if is_public {
                    // La valeur par défaut montrée dans l'inspecteur : le
                    // littéral s'il y en a un (sinon 0 / aucune entité).
                    let default = match (&init, kind) {
                        (Some(Expr::Int(v)), _) => *v,
                        (Some(Expr::Neg(inner)), _) => match **inner {
                            Expr::Int(v) => -v,
                            _ => 0,
                        },
                        (_, PubType::Entity) => -1,
                        _ => 0,
                    };
                    pub_regs.push(vars.len() as u8);
                    pubs.push(PubField {
                        name: name.clone(),
                        kind,
                        default,
                    });
                }
                vars.push((name, init, line));
            }
            Some(Tok::Ident(k)) if k == "function" => {
                let name = match p.next() {
                    Some(Tok::Ident(n)) => n,
                    _ => return Err(format!("ligne {line}: nom de function attendu")),
                };
                if BUILTINS.contains(&name.as_str()) {
                    return Err(format!(
                        "ligne {line}: '{name}' est une fonction du moteur (choisis un autre nom)"
                    ));
                }
                if funcs.iter().any(|f| f.name == name) {
                    return Err(format!("ligne {line}: function '{name}' déjà définie"));
                }
                p.expect_sym("(")?;
                let mut params = Vec::new();
                if matches!(p.peek(), Some(Tok::Sym(")"))) {
                    p.pos += 1;
                } else {
                    loop {
                        match p.next() {
                            Some(Tok::Ident(n)) => {
                                if params.contains(&n) {
                                    return Err(format!(
                                        "ligne {line}: paramètre '{n}' en double"
                                    ));
                                }
                                params.push(n);
                            }
                            _ => {
                                return Err(format!("ligne {line}: nom de paramètre attendu"))
                            }
                        }
                        match p.next() {
                            Some(Tok::Sym(",")) => {}
                            Some(Tok::Sym(")")) => break,
                            _ => {
                                return Err(format!(
                                    "ligne {}: « , » ou « ) » attendu",
                                    p.line()
                                ))
                            }
                        }
                    }
                }
                p.end_of_stmt()?;
                // Locales : les `var` en tête de corps, avant les instructions.
                let mut locals = Vec::new();
                loop {
                    p.eat_newlines();
                    let lline = p.line();
                    if matches!(p.peek(), Some(Tok::Ident(k)) if k == "var") {
                        p.pos += 1;
                        let decl = parse_var_decl(&mut p, lline)?;
                        if params.contains(&decl.0)
                            || locals.iter().any(|(n, _, _): &(String, _, _)| *n == decl.0)
                        {
                            return Err(format!(
                                "ligne {lline}: '{}' déjà déclarée dans cette function",
                                decl.0
                            ));
                        }
                        locals.push(decl);
                    } else {
                        break;
                    }
                }
                let body = p.parse_block(false)?.0;
                funcs.push(FuncDef {
                    name,
                    params,
                    locals,
                    body,
                    line,
                });
            }
            Some(Tok::Ident(k)) if k == "on" => {
                p.expect_kw("start")?;
                p.end_of_stmt()?;
                if start_block.is_some() {
                    return Err(format!("ligne {line}: « on start » en double"));
                }
                start_block = Some(p.parse_block(false)?.0);
            }
            Some(Tok::Ident(k)) if k == "every" => {
                p.expect_kw("frame")?;
                p.end_of_stmt()?;
                if update_block.is_some() {
                    return Err(format!("ligne {line}: « every frame » en double"));
                }
                update_block = Some(p.parse_block(false)?.0);
            }
            Some(_) => {
                return Err(format!(
                    "ligne {line}: attendu « var », « function », « on start » \
                     ou « every frame »"
                ))
            }
        }
    }
    if start_block.is_none() && update_block.is_none() {
        return Err("script vide : ajoute un bloc « every frame ... end »".into());
    }
    if vars.len() > 12 {
        return Err(format!(
            "trop de variables ({}, max 12 — 4 registres restent pour les calculs)",
            vars.len()
        ));
    }

    let mut g = Gen {
        vars: vars
            .iter()
            .enumerate()
            .map(|(i, (n, _, _))| (n.clone(), i as u8))
            .collect(),
        consts: Vec::new(),
        strings: Vec::new(),
        code: Vec::new(),
        temp_base: vars.len() as u8,
        temp: 0,
        locals: None,
        func_arity: funcs
            .iter()
            .map(|f| (f.name.clone(), f.params.len() as u8))
            .collect(),
        func_addr: HashMap::new(),
        patches: Vec::new(),
    };

    // Bloc demarre : initialisations `var x = ...` puis le bloc utilisateur.
    let start_pc = {
        let pc = g.code.len() as u16;
        for (name, init, _) in &vars {
            let r = g.vars[name];
            match init {
                Some(e) => g.emit_expr(e, r)?,
                None => g.code.push(insn16(OP_LOADI, r, 0)),
            }
        }
        // Surcharges de l'inspecteur : écrasent les défauts, avant que
        // le code utilisateur de `on start` ne tourne (comme Unity).
        for (i, reg) in pub_regs.iter().enumerate() {
            g.code.push(insn(OP_INITPUB, *reg, i as u8, 0));
        }
        if let Some(stmts) = &start_block {
            g.emit_stmts(stmts)?;
        }
        g.code.push(insn(OP_RET, 0, 0, 0));
        pc
    };
    let update_pc = match &update_block {
        Some(stmts) => {
            let pc = g.code.len() as u16;
            g.emit_stmts(stmts)?;
            g.code.push(insn(OP_RET, 0, 0, 0));
            pc
        }
        None => NO_PC,
    };

    /* Corps des fonctions, après les blocs : chaque function démarre
     * par OP_ENTER (nb de params, lu par la VM à l'appel), ses params +
     * locales occupent les premiers slots de la frame, les temporaires
     * suivent. Fin de corps sans return -> renvoie 0. */
    for f in &funcs {
        let nslots = f.params.len() + f.locals.len();
        if nslots > 12 {
            return Err(format!(
                "ligne {}: function '{}' : trop de paramètres + locales ({nslots}, \
                 max 12 — 4 registres restent pour les calculs)",
                f.line, f.name
            ));
        }
        g.func_addr.insert(f.name.clone(), g.code.len() as u16);
        g.code.push(insn(OP_ENTER, f.params.len() as u8, 0, 0));
        let mut locals: HashMap<String, u8> = HashMap::new();
        for (i, pname) in f.params.iter().enumerate() {
            locals.insert(pname.clone(), i as u8);
        }
        for (i, (n, _, _)) in f.locals.iter().enumerate() {
            locals.insert(n.clone(), (f.params.len() + i) as u8);
        }
        g.locals = Some(locals);
        let (saved_tb, saved_t) = (g.temp_base, g.temp);
        g.temp_base = nslots as u8;
        g.temp = 0;
        // La frame arrive zéroée de la VM : n'émettre que les inits.
        for (i, (_, init, _)) in f.locals.iter().enumerate() {
            if let Some(e) = init {
                g.emit_expr(e, (f.params.len() + i) as u8)?;
            }
        }
        g.emit_stmts(&f.body)?;
        g.code.push(insn(OP_RET0, 0, 0, 0));
        g.locals = None;
        g.temp_base = saved_tb;
        g.temp = saved_t;
    }

    /* Patch des appels (les références avant définition sont permises). */
    for (idx, name, line) in &g.patches {
        let addr = *g.func_addr.get(name).ok_or(format!(
            "ligne {line}: function '{name}' appelée mais jamais définie"
        ))?;
        let abase = (g.code[*idx] >> 16) as u8;
        g.code[*idx] = insn16(OP_CALL, abase, addr);
    }

    if g.code.len() > u16::MAX as usize {
        return Err("script trop long (65535 instructions max)".into());
    }

    /* Blob PSB1. */
    if pubs.len() > 255 {
        return Err("trop de champs publics (255 max)".into());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"PSB2");
    out.extend_from_slice(&(g.temp_base as u16 + 4).to_le_bytes()); // registres info
    out.extend_from_slice(&(g.consts.len() as u16).to_le_bytes());
    out.extend_from_slice(&(g.code.len() as u16).to_le_bytes());
    out.extend_from_slice(&start_pc.to_le_bytes());
    out.extend_from_slice(&update_pc.to_le_bytes());
    out.extend_from_slice(&(g.strings.len() as u16).to_le_bytes());
    // v2 : registre de chaque champ public (la VM y écrit la surcharge).
    out.extend_from_slice(&(pubs.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // pad (en-tête sur 20 o)
    for k in &g.consts {
        out.extend_from_slice(&k.to_le_bytes());
    }
    for i in &g.code {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out.extend_from_slice(&g.strings);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    for reg in &pub_regs {
        out.push(*reg);
    }
    while out.len() % 4 != 0 {
        out.push(0);
    }
    Ok(Compiled {
        bytecode: out,
        reg_used: g.temp_base as usize,
        fields: pubs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(bc: &[u8]) -> Vec<u32> {
        let consts = u16::from_le_bytes([bc[6], bc[7]]) as usize;
        let code_len = u16::from_le_bytes([bc[8], bc[9]]) as usize;
        let code_off = 20 + consts * 4;
        (0..code_len)
            .map(|i| {
                let o = code_off + i * 4;
                u32::from_le_bytes([bc[o], bc[o + 1], bc[o + 2], bc[o + 3]])
            })
            .collect()
    }

    fn ops(bc: &[u8]) -> Vec<u8> {
        let consts = u16::from_le_bytes([bc[6], bc[7]]) as usize;
        let code_len = u16::from_le_bytes([bc[8], bc[9]]) as usize;
        let code_off = 20 + consts * 4;
        (0..code_len)
            .map(|i| bc[code_off + i * 4 + 3]) // little-endian : op = octet haut
            .collect()
    }

    #[test]
    fn tourne_compile() {
        let c = compile("every frame\n    rotate_y(self, 12)\nend\n").unwrap();
        assert_eq!(&c.bytecode[0..4], b"PSB2");
        let o = ops(&c.bytecode);
        // start implicite : RET ; frame : SELF, LOADI, ADDROTY, RET.
        assert_eq!(o, vec![OP_RET, OP_SELF, OP_LOADI, OP_ADDROTY, OP_RET]);
    }

    #[test]
    fn si_sinon_saute_correctement() {
        let src = "var x\nevery frame\n    if x < 3 then\n        x = x + 1\n    else\n        x = 0\n    end\nend\n";
        let c = compile(src).unwrap();
        let o = ops(&c.bytecode);
        assert!(o.contains(&OP_JZ) && o.contains(&OP_JMP) && o.contains(&OP_LT));
    }

    #[test]
    fn erreurs_claires() {
        let e = compile("every frame\n    turn(self, 2)\nend\n").unwrap_err();
        assert!(e.contains("ligne 2") && e.contains("fonction inconnue"), "{e}");
        let e = compile("every frame\n    x = 1\nend\n").unwrap_err();
        assert!(e.contains("variable inconnue"), "{e}");
        let e = compile("every frame\n    if 1 then\nend\n").unwrap_err();
        assert!(e.contains("end"), "{e}");
        let e = compile("every frame\n    held(1)\nend\n").unwrap_err();
        assert!(e.contains("bouton") || e.contains("renvoie"), "{e}");
    }

    #[test]
    fn fonctions_utilisateur() {
        let src = "var total = 0\n\nfunction add(a, b)\n    return a + b\nend\n\nfunction fact(n)\n    if n <= 1 then\n        return 1\n    end\n    return n * fact(n - 1)\nend\n\nfunction bump()\n    total = total + 1\nend\n\non start\n    total = add(2, 3)\nend\n\nevery frame\n    bump()\nend\n";
        let c = compile(src).unwrap();
        let o = ops(&c.bytecode);
        assert!(o.contains(&OP_CALL) && o.contains(&OP_ENTER) && o.contains(&OP_RETV));
        assert!(o.contains(&OP_GLOAD) && o.contains(&OP_GSTORE)); // total depuis bump()
    }

    #[test]
    fn fonctions_locales_et_erreurs() {
        // Locale avec init, parametre utilise.
        compile("function twice(x)\n    var y = x * 2\n    return y\nend\nevery frame\n    show(self, twice(1))\nend\n").unwrap();
        // Arite fausse.
        let e = compile("function f(a)\n    return a\nend\nevery frame\n    show(self, f(1, 2))\nend\n").unwrap_err();
        assert!(e.contains("attend 1 argument"), "{e}");
        // Nom du moteur interdit.
        let e = compile("function move(a)\n    return a\nend\nevery frame\nend\n").unwrap_err();
        assert!(e.contains("moteur"), "{e}");
        // return hors function.
        let e = compile("every frame\n    return 1\nend\n").unwrap_err();
        assert!(e.contains("function"), "{e}");
        // var au milieu d'un bloc.
        let e = compile("every frame\n    var x = 1\nend\n").unwrap_err();
        assert!(e.contains("tête"), "{e}");
        // Fonction inconnue reste une erreur claire.
        let e = compile("every frame\n    show(self, mystere())\nend\n").unwrap_err();
        assert!(e.contains("mystere"), "{e}");
    }

    #[test]
    fn champs_publics() {
        let src = "public var speed = 24\npublic var target : entity\npublic var actif : bool = 1\nvar prive = 5\n\nevery frame\n    rotate_y(self, speed)\nend\n";
        let c = compile(src).unwrap();
        assert_eq!(c.fields.len(), 3);
        assert_eq!(c.fields[0].name, "speed");
        assert_eq!((c.fields[0].kind, c.fields[0].default), (PubType::Int, 24));
        assert_eq!((c.fields[1].kind, c.fields[1].default), (PubType::Entity, -1));
        assert_eq!((c.fields[2].kind, c.fields[2].default), (PubType::Bool, 1));
        // Un OP_INITPUB par champ, dans le bloc start.
        assert_eq!(ops(&c.bytecode).iter().filter(|o| **o == OP_INITPUB).count(), 3);
        // Un champ `entity` non réglé démarre à nil (-1), pas à l'entité 0.
        let src = "public var target : entity\nvar touche = 0\n\nevery frame\n    if target != nil then\n        touche = 1\n    end\nend\n";
        let c = compile(src).unwrap();
        let init = words(&c.bytecode)[0];
        assert_eq!(init >> 24, OP_LOADI as u32);
        assert_eq!((init & 0xFFFF) as i16, -1);

        // Erreurs claires.
        let e = compile("public speed = 1\nevery frame\nend\n").unwrap_err();
        assert!(e.contains("public"), "{e}");
        let e = compile("public var x : couleur\nevery frame\nend\n").unwrap_err();
        assert!(e.contains("couleur"), "{e}");
        let e = compile("var x : entity\nevery frame\nend\n").unwrap_err();
        assert!(e.contains("public"), "{e}");
    }

    #[test]
    fn dialogue_et_chaines() {
        let c = compile("every frame\n    dialog(\"salut / ça va\")\nend\n").unwrap();
        let s = String::from_utf8_lossy(&c.bytecode);
        assert!(s.contains("SALUT\nCA VA"), "chaîne translittérée attendue");
    }
}
