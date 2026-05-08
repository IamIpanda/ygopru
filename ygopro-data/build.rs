use std::fs;
use std::io;
use std::io::Write;
use std::error::Error;
use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;

use darling::FromAttributes;

#[derive(FromAttributes, Debug)]
#[darling(attributes(message), allow_unknown_fields)]
struct MessageParameters {
    flag: u8
}

fn scan_dir(path: OsString) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() { scan_dir(entry.path().into_os_string()).ok(); }
        else { scan_file(entry.path().into_os_string()).ok(); }
    }
    Ok(())
}

fn scan_file(path: OsString) -> Result<(), Box<dyn Error>> {
    let mut file = fs::File::open(path.clone())?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    let file = syn::parse_file(&buf)?;
    let structs = file.items.into_iter()
    .filter_map(|item| match item {
        syn::Item::Struct(struct_item) => Some(struct_item),
        _ => None
    })
    .filter(|struct_item| struct_item.attrs.iter().find(|attr| {
        attr.path.is_ident("derive") && attr.tokens.to_string().contains("Message")
    }).is_some())
    .collect::<Vec<_>>();
    println!("Scan {:?} for {} structs.", path, structs.len());
    generate_content(PathBuf::from(path).file_name().expect("cannot get file stem").to_os_string(), structs);
    Ok(())
}

fn generate_content(name: OsString, structs: Vec<syn::ItemStruct>) {
    if structs.len() == 0 { return }
    let module_name = std::path::Path::new(&name).file_stem().unwrap_or(name.as_os_str()).to_string_lossy();
    let structs_ref = &structs;
    let format = move |prefix: &str| {
        structs_ref
        .iter()
        .filter_map(|item_struct| {
            let parameter = MessageParameters::from_attributes(&item_struct.attrs).ok()?;
            Some(format!("{}{} = {}u8", prefix, item_struct.ident.to_string(), parameter.flag))
        })
        .collect::<Vec<_>>()
        .join(",\n            ")
    };
    let macro_calls_simple = format("");
    let macro_calls_long = format(&format!("ygopro::message::{}::", module_name));

    let content = format!("
macro_rules! every_message {{
    ($macro_name:path) => {{
        $macro_name!(
            {}
        );
    }};
}}

#[macro_export]
macro_rules! every_{}_message {{
    ($macro_name:path) => {{
        $macro_name!(
            {}
        );
    }};
}}
    ", macro_calls_simple, module_name, macro_calls_long);
    
    write_file(name, content)
}

fn write_file(name: OsString, content: String) {
    let path = std::env::var_os("OUT_DIR").expect("cannot find output directory");
    let file_path = PathBuf::from(path).join(name.clone());
    let mut file = fs::File::create(file_path).expect(&format!("open {} fail", name.clone().to_str().unwrap_or("unknown name")));
    write!(file, "{}", content).ok();
}

fn main() -> io::Result<()>{
    let path = std::env::var_os("CARGO_MANIFEST_DIR").expect("cannot find cargo minifest directory");
    scan_dir(PathBuf::from(path).join("src").into_os_string()).ok();
    Ok(())
}
