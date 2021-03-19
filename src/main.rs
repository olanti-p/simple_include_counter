use std::env::args;
use std::fs::File;
use std::io::Read;

struct FileInfo {
    name: String,
    data: String,
    includes: Vec<String>,
}

fn load_files(path: &str) -> Vec<FileInfo> {
    let dir = std::fs::read_dir(&path).unwrap();

    let mut ret = Vec::<FileInfo>::new();

    for file in dir {
        let file = file.unwrap();

        let name: String = file.file_name().to_string_lossy().to_string();
        if !name.ends_with(".h") && !name.ends_with(".cpp") {
            continue;
        }

        let mut data = String::new();
        File::open(&file.path())
            .unwrap()
            .read_to_string(&mut data)
            .unwrap();

        ret.push(FileInfo {
            name,
            data,
            includes: vec![],
        });
    }

    ret
}

fn custom_sort(data: &mut [FileInfo]) {
    data.sort_by(|a, b| a.name.cmp(&b.name));
}

fn parse_all_includes(data: &mut [FileInfo]) {
    for d in data {
        d.includes = parse_file_includes(&d.data);
    }
}

fn debug_print(data: &[FileInfo]) {
    println!("File / Size / # Includes");
    for it in data {
        println!("{: <32}  {: >6} {: >3}", it.name, it.data.len(), it.includes.len());
    }
}

fn parse_file_includes(data: &str) -> Vec<String> {

    vec!["haha.h".to_string(), "hoho.h".to_string()]
}

fn main() {
    if args().len() != 2 {
        println!("Expected dir path");
        return;
    }

    let mut data = load_files(&args().nth(1).unwrap());

    custom_sort(&mut data);
    parse_all_includes(&mut data);
    debug_print(&data);
}
