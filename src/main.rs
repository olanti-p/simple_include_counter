#![allow(dead_code)]

use num_format::{Buffer, CustomFormat, Grouping, ToFormattedStr};
use std::env::args;
use std::fs::File;
use std::io::Read;

struct FileInfo {
    name: String,
    data: String,
    includes: Vec<String>,
    lines: usize,  // source file lines
    clines: usize, // code lines
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
            clines: 0,
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
    CLines,
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
        SortMode::CLines => data.sort_by(|a, b| a.clines.cmp(&b.clines).reverse()),
    }
}

fn process_data(data: &mut [FileInfo]) {
    for d in data {
        d.lines = count_file_lines(&d.data);
        let (includes, clines) = parse_file_data(&d.data);
        d.includes = includes;
        d.clines = clines;
    }
}

fn fmt_bignum<T: ToFormattedStr>(n: T) -> String {
    let format = CustomFormat::builder()
        .grouping(Grouping::Standard)
        .minus_sign("-")
        .separator("'")
        .build()
        .unwrap();

    let mut buf = Buffer::new();
    buf.write_formatted(&n, &format);
    buf.to_string()
}

fn debug_print(data: &[FileInfo]) {
    println!("File / Size / Lines / Code lines / # Includes");
    for it in data {
        println!(
            "{: <32}  {: >7} {: >6} {: >6} {: >3}",
            it.name,
            fmt_bignum(it.data.len()),
            fmt_bignum(it.lines),
            fmt_bignum(it.clines),
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

fn skip_whitespace(s: &str) -> Option<&str> {
    let c = s.chars().nth(0).unwrap();

    if c.is_whitespace() || c.is_control() {
        // skip whitespace & newlines
        Some(&s[1..])
    } else {
        None
    }
}

fn skip_comment(s: &str) -> Option<&str> {
    Some(if s.starts_with("//") {
        // comment!
        skip_to_end_of_line(&s[1..])
    } else if s.starts_with("/*") {
        // comment!
        if let Some(idx) = s.find("*/") {
            &s[idx + 2..]
        } else {
            ""
        }
    } else {
        return None;
    })
}

fn skip_to_end_of_line(s: &str) -> &str {
    if let Some(idx) = s.find("\n") {
        &s[idx + 1..]
    } else {
        ""
    }
}

fn extract_include_name<'a>(s: &'a str, closing: &str) -> Option<&'a str> {
    if let Some(idx) = s.find(closing) {
        Some(&s[..idx])
    } else {
        None
    }
}

fn try_extract_include(s: &str) -> Option<&str> {
    let mut s = s;

    if !s.starts_with("#") {
        return None;
    }
    s = &s[1..];

    while let Some(ss) = skip_whitespace(s) {
        s = ss;
    }

    if !s.starts_with("include") {
        return None;
    }
    s = &s[7..];

    while let Some(ss) = skip_whitespace(s) {
        s = ss;
    }

    if s.starts_with("<") {
        // system include
        extract_include_name(&s[1..], ">")
    } else if s.starts_with("\"") {
        // local include
        extract_include_name(&s[1..], "\"")
    } else {
        // should never happen
        panic!("Shit happened")
    }
}

fn parse_file_data(data: &str) -> (Vec<String>, usize) {
    let mut ret = Vec::<String>::new();
    let mut clines = 0usize;

    let mut s = data;
    loop {
        if s.is_empty() {
            break;
        }

        if let Some(ss) = skip_whitespace(s) {
            s = ss;
            continue;
        }

        if let Some(ss) = skip_comment(s) {
            s = ss;
            continue;
        }

        clines += 1;

        if let Some(inc) = try_extract_include(s) {
            ret.push(inc.to_string());
        }
        s = skip_to_end_of_line(s);
    }

    (ret, clines)
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
        "clin" => SortMode::CLines,
        x => {
            println!("Unknown sort method {}", x);
            return;
        }
    };

    let mut data = load_files(&args().nth(1).unwrap());
    process_data(&mut data);
    custom_sort(&mut data, sort_mode);
    debug_print(&data);
    println!("Done.");
}
