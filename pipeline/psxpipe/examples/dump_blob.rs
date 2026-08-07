//! Désassemble un script PSX Script : le bytecode en header C (pour le
//! banc d'essai hôte de la VM) sur la sortie standard, le listing des
//! instructions sur la sortie d'erreur.
//!
//! Usage: cargo run --example dump_blob -- <script.psxs> [nom] > blob.h

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("usage: dump_blob <script.psxs> [nom]");
    let name = a.next().unwrap_or_else(|| "blob".into());
    let src = std::fs::read_to_string(&path).expect("lecture du script");
    let c = match psxpipe::psxs::compile(&src) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    };

    println!("static const unsigned char {name}[] = {{");
    for chunk in c.bytecode.chunks(12) {
        let line: Vec<String> = chunk.iter().map(|b| format!("0x{b:02X}")).collect();
        println!("\t{},", line.join(", "));
    }
    println!("}};");
    println!("static const unsigned int {name}_len = {};", c.bytecode.len());

    for (i, f) in c.fields.iter().enumerate() {
        eprintln!("champ public {i} : {} ({:?}, défaut {})", f.name, f.kind, f.default);
    }
    let u16at = |o: usize| u16::from_le_bytes([c.bytecode[o], c.bytecode[o + 1]]) as usize;
    let (consts, code_len) = (u16at(6), u16at(8));
    eprintln!("start = {}, frame = {}", u16at(10), u16at(12));
    for i in 0..code_len {
        let o = 20 + consts * 4 + i * 4;
        let w = u32::from_le_bytes([
            c.bytecode[o],
            c.bytecode[o + 1],
            c.bytecode[o + 2],
            c.bytecode[o + 3],
        ]);
        eprintln!(
            "{i:4}  op={:<3} a={:<3} b={:<3} c={:<3} imm={}",
            w >> 24,
            (w >> 16) & 0xFF,
            (w >> 8) & 0xFF,
            w & 0xFF,
            w & 0xFFFF
        );
    }
}
