use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn cc_cli_emits_assembly_and_memory_image() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ex3-cc-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("answer.c");
    let asm = dir.join("answer.s");
    let mem = dir.join("answer.mem");
    fs::write(&input, "int main(void) { return 6 * 7; }\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ex3"))
        .arg("cc")
        .arg(&input)
        .arg("-S")
        .arg("-o")
        .arg(&asm)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fs::read_to_string(&asm).unwrap().contains("CALL main"));

    let output = Command::new(env!("CARGO_BIN_EXE_ex3"))
        .arg("cc")
        .arg(&input)
        .arg("-o")
        .arg(&mem)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(mem.exists());
    assert!(mem.with_extension("prb").exists());

    let output = Command::new(env!("CARGO_BIN_EXE_ex3"))
        .arg("run")
        .arg(&mem)
        .arg("--max-steps")
        .arg("10000")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("AC=0000002a"));
    fs::remove_dir_all(dir).unwrap();
}
