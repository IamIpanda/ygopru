use glob::glob;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

const LUA_54_DIRECTORY: &str = "5.4.8";
const LUA_55_DIRECTORY: &str = "5.5.1";
const LUA_LIBRARY: &str = "ygopro-core-lua";
const SUPPORT_LIBRARY: &str = "ygopro-core-support";
const KOISHI_PREFIX: &str = "koishi_";

fn lua_directory() -> &'static str {
    if env::var_os("CARGO_FEATURE_LUA55").is_some() { LUA_55_DIRECTORY } else { LUA_54_DIRECTORY }
}

enum Core {
    Official,
    Koishi,
    Custom(PathBuf),
}

impl Core {
    fn source_dir(&self, root: &Path) -> PathBuf {
        match self {
            Core::Official => root.join("core").join("official"),
            Core::Koishi => root.join("core").join("koishi"),
            Core::Custom(ocgcore_dir) => ocgcore_dir.clone(),
        }
    }

    fn library_name(&self) -> &'static str {
        match self {
            Core::Official => "ygopro-core-official",
            Core::Koishi => "ygopro-core-koishi",
            Core::Custom(_) => "ygopro-core-custom",
        }
    }
}

struct Selection {
    cores: Vec<Core>,
    dual: bool,
}

impl Selection {
    fn from_enabled_features() -> Self {
        let dual = env::var_os("CARGO_FEATURE_DUAL").is_some();
        let official = env::var_os("CARGO_FEATURE_OFFICIAL").is_some();
        let koishi = env::var_os("CARGO_FEATURE_KOISHI").is_some();
        let custom = env::var_os("CARGO_FEATURE_CUSTOM").is_some();
        if dual {
            if official || koishi || custom {
                println!("cargo:warning=feature `dual` takes precedence over `official`, `koishi` and `custom`");
            }
            return Self { cores: vec![Core::Official, Core::Koishi], dual: true };
        }
        if [official, koishi, custom].iter().filter(|enabled| **enabled).count() > 1 {
            println!("cargo:warning=enabling more than one of `official`, `koishi`, `custom`; using one of them by priority");
        }
        if custom {
            let ocgcore_dir = PathBuf::from(env::var("YGOPRO_OCGCORE_DIR").unwrap_or_else(|_| {
                panic!("feature `custom` requires env YGOPRO_OCGCORE_DIR to point to an ocgcore source tree")
            }));
            return Self { cores: vec![Core::Custom(ocgcore_dir)], dual: false };
        }
        if koishi {
            return Self { cores: vec![Core::Koishi], dual: false };
        }
        Self { cores: vec![Core::Official], dual: false }
    }
}

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let lua_dir = root.join("lua").join(lua_directory());
    let selection = Selection::from_enabled_features();
    let primary_dir = selection.cores[0].source_dir(&root);

    println!("cargo:rerun-if-changed={}", lua_dir.display());
    println!("cargo:rerun-if-changed=src/lua.cpp");
    println!("cargo:rerun-if-changed=src/lua_extern_c.h");
    println!("cargo:rerun-if-changed=src/random.cpp");
    println!("cargo:rerun-if-env-changed=YGOPRO_OCGCORE_DIR");

    if !lua_dir.exists() {
        panic!("lua not found at {}. Please run: git submodule update --init", lua_dir.display());
    }

    ensure_source_tree(&primary_dir);
    compile_support(&root, &primary_dir);
    compile_lua(&root, &lua_dir);

    for core in &selection.cores {
        let ocgcore_dir = core.source_dir(&root);
        println!("cargo:rerun-if-changed={}", ocgcore_dir.display());
        ensure_source_tree(&ocgcore_dir);
        let archive = compile_core(&root, &lua_dir, core, &ocgcore_dir);
        if selection.dual && matches!(core, Core::Koishi) {
            rename_symbols(&archive);
        }
    }
}

fn ensure_source_tree(dir: &Path) {
    if !dir.exists() {
        panic!("ocgcore not found at {}. Please run: git submodule update --init", dir.display());
    }
}

fn target_env() -> String {
    env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default()
}

fn target_os() -> String {
    env::var("CARGO_CFG_TARGET_OS").unwrap_or_default()
}

fn configure(build: &mut cc::Build, root: &Path, lua_dir: &Path) {
    build.cpp(true);
    build.warnings(false);
    build.flag_if_supported("-std=c++14");
    build.include(lua_dir);
    build.define("LUA_CORE", None);
    force_include(build, &root.join("src").join("lua_extern_c.h"));
    if target_env() == "msvc" {
        build.flag("/TP");
        build.flag("/utf-8");
    } else {
        build.flag("-Wno-deprecated-declarations");
    }
}

fn force_include(build: &mut cc::Build, header: &Path) {
    if target_env() == "msvc" {
        build.flag(format!("/FI{}", header.display()));
    } else {
        build.flag("-include").flag(header);
    }
}

fn compile_lua(root: &Path, lua_dir: &Path) {
    let mut build = cc::Build::new();
    configure(&mut build, root, lua_dir);
    for entry in glob(lua_dir.join("*.c").to_str().unwrap()).unwrap() {
        let path = entry.unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();
        if filename != "lua.c" && filename != "luac.c" && filename != "onelua.c" {
            build.file(&path);
        }
    }
    build.compile(LUA_LIBRARY);
}

fn compile_core(root: &Path, lua_dir: &Path, core: &Core, ocgcore_dir: &Path) -> PathBuf {
    let mut build = cc::Build::new();
    configure(&mut build, root, lua_dir);
    build.include(ocgcore_dir);
    for entry in glob(ocgcore_dir.join("*.cpp").to_str().unwrap()).unwrap() {
        let path = entry.unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();
        if filename == "emscripten_shutdown.cpp" {
            continue;
        }
        build.file(&path);
    }
    build.file(root.join("src").join("lua.cpp"));
    build.compile(core.library_name());
    archive_path(core.library_name())
}

fn archive_path(library: &str) -> PathBuf {
    let name = if target_os() == "windows" && target_env() == "msvc" { format!("{}.lib", library) } else { format!("lib{}.a", library) };
    PathBuf::from(env::var("OUT_DIR").unwrap()).join(name)
}

fn compile_support(root: &Path, ocgcore_dir: &Path) {
    let mut build = cc::Build::new();
    build.cpp(true);
    build.warnings(false);
    build.flag_if_supported("-std=c++14");
    build.include(ocgcore_dir);
    if target_env() == "msvc" {
        build.flag("/TP");
        build.flag("/utf-8");
    } else {
        build.flag("-Wno-deprecated-declarations");
    }
    build.file(root.join("src").join("random.cpp"));
    build.compile(SUPPORT_LIBRARY);
}

fn rename_symbols(archive: &Path) {
    let symbols = defined_symbols(archive);
    assert!(!symbols.is_empty(), "no defined symbol in {}", archive.display());
    let list_path = archive.with_extension("redefine");
    let mut list = String::new();
    for symbol in &symbols {
        list.push_str(symbol);
        list.push(' ');
        list.push_str(&prefixed(symbol));
        list.push('\n');
    }
    fs::write(&list_path, &list).unwrap();
    let status = Command::new(rust_objcopy()).arg(format!("--redefine-syms={}", list_path.display())).arg(archive).status().expect("failed to run rust-objcopy");
    assert!(status.success(), "rust-objcopy failed on {}", archive.display());
    let leaked: Vec<_> = defined_symbols(archive).into_iter().filter(|symbol| !is_prefixed(symbol)).collect();
    assert!(leaked.is_empty(), "{} symbols left unprefixed in {}: {:?}", leaked.len(), archive.display(), &leaked[..leaked.len().min(10)]);
}

fn is_prefixed(symbol: &str) -> bool {
    match symbol.strip_prefix('_') {
        Some(rest) if target_os() == "macos" => rest.starts_with(KOISHI_PREFIX),
        _ => symbol.starts_with(KOISHI_PREFIX),
    }
}

fn prefixed(symbol: &str) -> String {
    match symbol.strip_prefix('_') {
        Some(rest) if target_os() == "macos" => format!("_{}{}", KOISHI_PREFIX, rest),
        _ => format!("{}{}", KOISHI_PREFIX, symbol),
    }
}

fn defined_symbols(archive: &Path) -> BTreeSet<String> {
    match target_os().as_str() {
        "windows" => coff_symbols(archive),
        _ => nm_symbols(archive),
    }
}

fn nm_symbols(archive: &Path) -> BTreeSet<String> {
    let output = Command::new("nm").args(["-P", "--defined-only", "--extern-only"]).arg(archive).output().expect("failed to run nm");
    assert!(output.status.success(), "nm failed on {}", archive.display());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.ends_with(':'))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|symbol| !symbol.is_empty() && !is_prefixed(symbol))
        .map(str::to_owned)
        .collect()
}

fn coff_symbols(archive: &Path) -> BTreeSet<String> {
    let output = Command::new(dumpbin()).arg("/symbols").arg(archive).output().expect("failed to run dumpbin");
    assert!(output.status.success(), "dumpbin failed on {}", archive.display());
    let mut symbols = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 || fields[2] == "UNDEF" || !fields.contains(&"External") {
            continue;
        }
        let Some(index) = line.find('|') else { continue };
        let symbol = line[index + 1..].split_whitespace().next().unwrap_or("");
        if !symbol.is_empty() && !is_prefixed(symbol) {
            symbols.insert(symbol.to_owned());
        }
    }
    symbols
}

fn dumpbin() -> PathBuf {
    let compiler = cc::Build::new().cpp(true).get_compiler().path().to_path_buf();
    if let Some(dir) = compiler.parent() {
        let candidate = dir.join("dumpbin.exe");
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("dumpbin.exe")
}

fn rust_objcopy() -> PathBuf {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| String::from("rustc"));
    let output = Command::new(rustc).args(["--print", "sysroot"]).output().expect("failed to run rustc");
    let sysroot = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let name = if target_os() == "windows" { "rust-objcopy.exe" } else { "rust-objcopy" };
    sysroot.join("lib").join("rustlib").join(env::var("HOST").unwrap()).join("bin").join(name)
}

