//! Intégration du compilateur SSL sfall, sans shell et sans publier un INT invalide.
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct CompileReport {
    pub bytes: Option<Vec<u8>>,
    pub output: String,
}

pub fn find_compiler(root: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("FO_SSL_COMPILER") {
        candidates.push(PathBuf::from(path));
    }
    for base in [
        Some(root.to_path_buf()),
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
    ]
    .into_iter()
    .flatten()
    {
        for rel in [
            "tools/sslc/compile.exe",
            "tools/sslc-sfall5/compile.exe",
            "compile.exe",
        ] {
            candidates.push(base.join(rel));
        }
        for parent in base.ancestors().take(4) {
            candidates.push(parent.join("FO2glscripts/tools/sslc-sfall5/compile.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub fn compile(
    executable: &Path,
    source: &Path,
    text: &str,
    includes: &[PathBuf],
) -> Result<CompileReport, String> {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("fomapper_ssl_{}_{}", std::process::id(), id));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let input = dir.join(
        source
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("script.ssl")),
    );
    let output = dir.join("script.int");
    let result = (|| {
        // Les sources historiques et le compilateur utilisent Latin-1.
        let bytes: Vec<u8> = text
            .chars()
            .map(|c| {
                u8::try_from(c as u32)
                    .map_err(|_| format!("Caractère non pris en charge dans SSL : {c}"))
            })
            .collect::<Result<_, _>>()?;
        std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
        let mut command = Command::new(executable);
        command.current_dir(&dir);
        command.args(["-q", "-l", "-p"]);
        if let Some(parent) = source.parent().filter(|p| p.is_dir()) {
            command.arg(format!("-I{}", parent.display()));
        }
        for path in includes.iter().filter(|p| p.is_dir()) {
            command.arg(format!("-I{}", path.display()));
        }
        command
            .arg(input.file_name().unwrap())
            .arg("-o")
            .arg(output.file_name().unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let compiled = command
            .output()
            .map_err(|e| format!("{} : {e}", executable.display()))?;
        let decode = |bytes: &[u8]| bytes.iter().map(|&b| b as char).collect::<String>();
        let log = format!("{}{}", decode(&compiled.stdout), decode(&compiled.stderr)).replace(
            input.file_name().unwrap().to_string_lossy().as_ref(),
            &source.display().to_string(),
        );
        if !compiled.status.success() || !output.is_file() {
            return Ok(CompileReport {
                bytes: None,
                output: log,
            });
        }
        let bytes = std::fs::read(&output).map_err(|e| e.to_string())?;
        crate::Program::parse(&bytes).map_err(|e| format!("INT produit invalide : {e}\n{log}"))?;
        Ok(CompileReport {
            bytes: Some(bytes),
            output: log,
        })
    })();
    for path in [&input, &output] {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_dir(&dir);
    result
}

/// Ligne d'un diagnostic `fichier.ssl:42: ...` ou `fichier.ssl(42): ...`.
pub fn diagnostic_line(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let end = lower.find(".ssl")? + 4;
    let rest = text[end..].trim_start_matches([':', '(', ' ', '>']);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub fn register_script(game: &fo_core::GameData, name: &str, locals: u32) -> Result<usize, String> {
    if name.is_empty()
        || name.len() > 13
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("Nom de script invalide".into());
    }
    let file = format!("{name}.int");
    let old = game.list("scripts/scripts.lst");
    if let Some(index) = old.iter().position(|n| n.eq_ignore_ascii_case(&file)) {
        return Ok(index);
    }
    let mut bytes = game
        .read("scripts/scripts.lst")
        .map_err(|e| e.to_string())?;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.extend_from_slice(b"\r\n");
    }
    bytes.extend_from_slice(format!("{file} # local_vars={locals}\r\n").as_bytes());
    game.save_data_file("scripts/scripts.lst", &bytes)
        .map_err(|e| e.to_string())?;
    game.forget_lists();
    Ok(old.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_keep_the_source_line() {
        assert_eq!(diagnostic_line("C:\\src\\file.ssl:42: error"), Some(42));
        assert_eq!(diagnostic_line("file.ssl(12): bad token"), Some(12));
        assert_eq!(
            diagnostic_line("[Error] <file.ssl>:1:25: expecting ';'."),
            Some(1)
        );
        assert_eq!(diagnostic_line("success"), None);
    }
    #[test]
    fn real_compiler_accepts_valid_source_and_reports_errors() {
        let Some(exe) = find_compiler(Path::new("D:/Git/Fallout/FalloutMapper")) else {
            return;
        };
        let source = std::env::temp_dir().join("mappertest.ssl");
        let good = compile(
            &exe,
            &source,
            "procedure start;\nprocedure start begin\nend\n",
            &[],
        )
        .unwrap();
        assert!(good.bytes.is_some(), "{}", good.output);
        let bad = compile(&exe, &source, "procedure start begin !!! end", &[]).unwrap();
        assert!(bad.bytes.is_none());
        assert!(!bad.output.trim().is_empty());
        assert!(
            bad.output
                .lines()
                .any(|line| diagnostic_line(line).is_some())
        );
    }
    #[test]
    fn registering_a_script_appends_without_changing_existing_indexes() {
        let root = std::env::temp_dir().join(format!("mapper_lst_{}", std::process::id()));
        let dir = root.join("data/scripts");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("scripts.lst"), b"old.int # local_vars=4\r\n").unwrap();
        let game = fo_core::GameData::open(&root).unwrap();
        assert_eq!(game.list("scripts/scripts.lst").len(), 1);
        assert_eq!(register_script(&game, "newone", 3).unwrap(), 1);
        assert_eq!(register_script(&game, "NEWONE", 3).unwrap(), 1);
        let text = std::fs::read_to_string(dir.join("scripts.lst")).unwrap();
        assert_eq!(
            text,
            "old.int # local_vars=4\r\nnewone.int # local_vars=3\r\n"
        );
        assert_eq!(game.list("scripts/scripts.lst").len(), 2);
        assert_eq!(
            std::fs::read(dir.join("scripts.lst.bak")).unwrap(),
            b"old.int # local_vars=4\r\n"
        );
        for name in ["scripts.lst", "scripts.lst.bak"] {
            std::fs::remove_file(dir.join(name)).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
        std::fs::remove_dir(root.join("data")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn compiler_resolves_includes_from_a_source_directory_with_spaces() {
        let Some(exe) = find_compiler(Path::new("D:/Git/Fallout/FalloutMapper")) else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("mapper ssl includes {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("defs.h"), b"#define BODY begin end\n").unwrap();
        let report = compile(
            &exe,
            &dir.join("test.ssl"),
            "#include \"defs.h\"\nprocedure start;\nprocedure start BODY\n",
            &[],
        )
        .unwrap();
        std::fs::remove_file(dir.join("defs.h")).unwrap();
        std::fs::remove_dir(dir).unwrap();
        assert!(report.bytes.is_some(), "{}", report.output);
    }
}
