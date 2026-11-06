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

fn lua_directory() -> &'static str {
    if env::var_os("CARGO_FEATURE_LUA55").is_some() { LUA_55_DIRECTORY } else { LUA_54_DIRECTORY }
}

fn dir_or(variable: &str, default: PathBuf) -> PathBuf {
    env::var_os(variable).map(PathBuf::from).unwrap_or(default)
}

enum Core {
    Official,
    Koishi,
    Custom(PathBuf),
}

impl Core {
    fn source_dir(&self, root: &Path) -> PathBuf {
        match self {
            Core::Official => dir_or("YGOCORE_OFFICIAL_DIR", root.join("core").join("official")),
            Core::Koishi => dir_or("YGOCORE_KOISHI_DIR", root.join("core").join("koishi")),
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

    fn prefix(&self) -> &'static str {
        match self {
            Core::Official => unreachable!(),
            Core::Koishi => "koishi_",
            Core::Custom(_) => "custom_",
        }
    }
}

struct Selection {
    cores: Vec<Core>,
}

impl Selection {
    fn from_enabled_features() -> Self {
        let official = env::var_os("CARGO_FEATURE_OFFICIAL").is_some();
        let koishi = env::var_os("CARGO_FEATURE_KOISHI").is_some();
        let custom = env::var_os("CARGO_FEATURE_CUSTOM").is_some();
        let mut cores = Vec::new();
        if official {
            cores.push(Core::Official);
        }
        if koishi {
            cores.push(Core::Koishi);
        }
        if custom {
            if env::var_os("YGOPRO_OCGCORE_DIR").is_some() {
                println!("cargo:warning=`YGOPRO_OCGCORE_DIR` is renamed to `YGOCORE_CUSTOM_DIR`, and is ignored when the latter is set");
            }
            let ocgcore_dir = env::var_os("YGOCORE_CUSTOM_DIR").or_else(|| env::var_os("YGOPRO_OCGCORE_DIR")).map(PathBuf::from).unwrap_or_else(|| {
                panic!("feature `custom` requires env YGOCORE_CUSTOM_DIR to point to an ocgcore source tree")
            });
            cores.push(Core::Custom(ocgcore_dir));
            if let Some(wrapper) = env::var_os("YGOCORE_CUSTOM_WRAPPER").map(PathBuf::from) {
                println!("cargo:rustc-cfg=custom_wrapper");
                println!("cargo:rustc-env=YGOCORE_CUSTOM_WRAPPER={}", wrapper.display());
                println!("cargo:rerun-if-changed={}", wrapper.display());
            }
        }
        assert!(!cores.is_empty(), "no core selected: enable one of `official`, `koishi` or `custom`");
        println!("cargo:rustc-check-cfg=cfg(mix_core)");
        println!("cargo:rustc-check-cfg=cfg(custom_wrapper)");
        if cores.len() > 1 {
            println!("cargo:rustc-cfg=mix_core");
            // It's not needed for this cargo. This metadata is only for downstream crates to know that this crate is built with mix_core enabled.
            println!("cargo::metadata=mix_core=true");
        }
        Self { cores }
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
    println!("cargo:rerun-if-env-changed=YGOCORE_OFFICIAL_DIR");
    println!("cargo:rerun-if-env-changed=YGOCORE_KOISHI_DIR");
    println!("cargo:rerun-if-env-changed=YGOCORE_CUSTOM_DIR");
    println!("cargo:rerun-if-env-changed=YGOPRO_OCGCORE_DIR");
    println!("cargo:rerun-if-env-changed=YGOCORE_CUSTOM_WRAPPER");

    if !lua_dir.exists() {
        panic!("lua not found at {}. Please run: git submodule update --init", lua_dir.display());
    }

    ensure_source_tree(&primary_dir);
    compile_support(&root, &primary_dir);
    compile_lua(&root, &lua_dir);

    let mix_core = selection.cores.len() > 1;

    for core in &selection.cores {
        let ocgcore_dir = core.source_dir(&root);
        println!("cargo:rerun-if-changed={}", ocgcore_dir.display());
        ensure_source_tree(&ocgcore_dir);
        let archive = compile_core(&root, &lua_dir, core, &ocgcore_dir);
        if mix_core && !matches!(core, Core::Official) {
            rename_symbols(&archive, core.prefix());
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

fn rename_symbols(archive: &Path, prefix: &str) {
    let symbols = defined_symbols(archive, prefix);
    assert!(!symbols.is_empty(), "no defined symbol in {}", archive.display());
    let list_path = archive.with_extension("redefine");
    let mut list = String::new();
    for symbol in &symbols {
        list.push_str(symbol);
        list.push(' ');
        list.push_str(&prefixed(symbol, prefix));
        list.push('\n');
    }
    fs::write(&list_path, &list).unwrap();
    let status = Command::new(rust_objcopy()).arg(format!("--redefine-syms={}", list_path.display())).arg(archive).status().expect("failed to run rust-objcopy");
    assert!(status.success(), "rust-objcopy failed on {}", archive.display());
    let leaked: Vec<_> = defined_symbols(archive, prefix).into_iter().filter(|symbol| !is_prefixed(symbol, prefix)).collect();
    assert!(leaked.is_empty(), "{} symbols left unprefixed in {}: {:?}", leaked.len(), archive.display(), &leaked[..leaked.len().min(10)]);
}

fn is_prefixed(symbol: &str, prefix: &str) -> bool {
    match symbol.strip_prefix('_') {
        Some(rest) if target_os() == "macos" => rest.starts_with(prefix),
        _ => symbol.starts_with(prefix),
    }
}

fn prefixed(symbol: &str, prefix: &str) -> String {
    match symbol.strip_prefix('_') {
        Some(rest) if target_os() == "macos" => format!("_{}{}", prefix, rest),
        _ => format!("{}{}", prefix, symbol),
    }
}

fn defined_symbols(archive: &Path, prefix: &str) -> BTreeSet<String> {
    match target_os().as_str() {
        "windows" => coff_symbols(archive, prefix),
        _ => nm_symbols(archive, prefix),
    }
}

fn nm_symbols(archive: &Path, prefix: &str) -> BTreeSet<String> {
    let output = Command::new("nm").args(["-P", "--defined-only", "--extern-only"]).arg(archive).output().expect("failed to run nm");
    assert!(output.status.success(), "nm failed on {}", archive.display());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.ends_with(':'))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|symbol| !symbol.is_empty() && !is_prefixed(symbol, prefix))
        .map(str::to_owned)
        .collect()
}

fn coff_symbols(archive: &Path, prefix: &str) -> BTreeSet<String> {
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
        if !symbol.is_empty() && !is_prefixed(symbol, prefix) {
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

