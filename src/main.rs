use std::env::args;
use std::fs::File;
use std::io::Read;
use num_format::{Buffer, Error, CustomFormat, Grouping, ToFormattedStr};

struct FileInfo {
    name: String,
    data: String,
    includes: Vec<String>,
    lines: usize,
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
            lines: 0,
            includes: vec![],
        });
    }

    ret
}

enum SortMode {
    Name,
    Size,
    NumIncludes,
    Lines,
}

fn custom_sort(data: &mut [FileInfo], mode: SortMode) {
    // First sort by name
    data.sort_by(|a, b| a.name.cmp(&b.name));
    // Then sort by whatever else
    match mode {
        SortMode::Name => return,
        SortMode::Size => data.sort_by(|a, b| a.data.len().cmp(&b.data.len()).reverse()),
        SortMode::NumIncludes => data.sort_by(|a, b| a.includes.len().cmp(&b.includes.len())),
        SortMode::Lines => data.sort_by(|a, b| a.lines.cmp(&b.lines).reverse()),
    }
}

fn process_data(data: &mut [FileInfo]) {
    for d in data {
        d.lines = count_file_lines(&d.data);
        d.includes = parse_file_includes(&d.data);
    }
}

fn fmt_bignum<T: ToFormattedStr>(n: T) -> String {
    let format = CustomFormat::builder()
        .grouping(Grouping::Standard)
        .minus_sign("-")
        .separator("'")
        .build().unwrap();

    let mut buf = Buffer::new();
    buf.write_formatted(&n, &format);
    buf.to_string()
}

fn debug_print(data: &[FileInfo]) {
    println!("File / Size / Lines / # Includes");
    for it in data {
        println!(
            "{: <32}  {: >7} {: >7} {: >3}",
            it.name,
            fmt_bignum(it.data.len()),
            it.lines,
            it.includes.len()
        );
    }
    println!("Total files: {}", data.len());
    let sum: usize = data.iter().map(|x| x.data.len()).sum();
    println!("Total size: {}", fmt_bignum(sum));
}

fn count_file_lines(data: &str) -> usize {
    data.lines().count()
}

fn parse_file_includes(data: &str) -> Vec<String> {

    vec!["haha.h".to_string(), "hoho.h".to_string()]
}

fn main() {
    if args().len() != 3 {
        println!("Expected dir path & sort mode");
        return;
    }

    let sort_mode = match args().nth(2).unwrap().as_str() {
        "name" => SortMode::Name,
        "incl" => SortMode::NumIncludes,
        "size" => SortMode::Size,
        "line" => SortMode::Lines,
        x => { println!("Unknown sort method {}", x); return; }
    };

    let mut data = load_files(&args().nth(1).unwrap());
    process_data(&mut data);
    custom_sort(&mut data, sort_mode);
    debug_print(&data);
}
