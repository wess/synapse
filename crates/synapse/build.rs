use std::{env, fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../assets/scene/characters");
    let mut code = String::from("pub const SPRITES: &[&[u8]] = &[\n");
    for sheet in 2..=5 {
        let path = root.join(format!("chara{sheet}.png"));
        println!("cargo:rerun-if-changed={}", path.display());
        if path.exists() {
            code.push_str(&format!(
                "include_bytes!({:?}),\n",
                path.canonicalize().unwrap()
            ));
        }
    }
    code.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("scene.rs"),
        code,
    )
    .unwrap();
}
