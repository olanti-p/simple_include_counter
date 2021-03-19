#![allow(dead_code)]

use num_format::{Buffer, CustomFormat, Grouping, ToFormattedStr};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::env::args;
use std::fs::File;
use std::io::Read;

struct IncludeInfo {
    name: String,
    system: bool,
    idx: Option<usize>,
}

struct FileInfo {
    name: String,
    data: String,
    system: bool,

    includes: Vec<IncludeInfo>,
    included_by: Vec<IncludeInfo>,

    lines: usize,  // source file lines
    clines: usize, // code lines

    combined_clines: Option<usize>, // code lines with includes
}

impl std::fmt::Display for IncludeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.system {
            write!(f, "<{}>", self.name)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

impl FileInfo {
    pub fn combined_clines(&self, data: &[FileInfo]) -> Option<usize> {
        let mut ret = self.lines;
        for inc in &self.includes {
            if let Some(idx) = inc.idx {
                ret += data[idx].combined_clines?;
            }
        }
        Some(ret)
    }
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
            system: false,
            lines: 0,
            clines: 0,
            includes: vec![],
            included_by: vec![],
            combined_clines: None,
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
    IncLines,
}

fn custom_sort(data: &mut [FileInfo], mode: SortMode) {
    // First sort by name
    data.sort_by(|a, b| a.name.cmp(&b.name));
    // Then sort by whatever else
    match mode {
        SortMode::Name => {}
        SortMode::Size => data.sort_by(|a, b| a.data.len().cmp(&b.data.len()).reverse()),
        SortMode::NumIncludes => data.sort_by(|a, b| a.includes.len().cmp(&b.includes.len())),
        SortMode::Lines => data.sort_by(|a, b| a.lines.cmp(&b.lines).reverse()),
        SortMode::CLines => data.sort_by(|a, b| a.clines.cmp(&b.clines).reverse()),
        SortMode::IncLines => data.sort_by(|a, b| match (a.combined_clines, b.combined_clines) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(al), Some(bl)) => al.cmp(&bl).reverse(),
        }),
    }
    // Then sort system stuff to the bottom
    data.sort_by(|a, b| match (a.system, b.system) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => Ordering::Equal,
    });
}

/// Returns whether it's possible to build a tree without circular dependencies
fn process_data_basic(data: &mut Vec<FileInfo>) -> bool {
    // Step 1. Parse files
    for d in data.iter_mut() {
        d.lines = count_file_lines(&d.data);
        let (includes, clines) = parse_file_data(&d.data);
        d.includes = includes;
        d.clines = clines;

        d.includes.sort_by(|a, b| {
            if a.system != b.system {
                if a.system {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            } else {
                a.name.cmp(&b.name)
            }
        })
    }

    // Step 2. Add stabs for system includes
    let mut to_add = HashSet::<String>::new();
    for d in data.iter() {
        for inc in &d.includes {
            if !inc.system {
                // should already be on the list
                continue;
            }
            to_add.insert(inc.name.clone());
        }
    }
    data.extend(to_add.into_iter().map(|name| FileInfo {
        name,
        data: "".to_string(),
        system: true,
        includes: vec![],
        included_by: vec![],
        lines: 1,
        clines: 1,
        combined_clines: Some(1),
    }));

    // Step 3. Index includes for quick access
    let hashed: HashMap<String, usize> = data
        .iter()
        .enumerate()
        .map(|(idx, x)| (x.name.clone(), idx))
        .collect();
    for d in data.iter_mut() {
        for i in &mut d.includes {
            if let Some(idx) = hashed.get(&i.name).copied() {
                i.idx = Some(idx);
            }
        }
    }

    // Step 4. Calculate combined cost
    loop {
        let mut did_something = false;
        for idx in 0..data.len() {
            let info: &FileInfo = &data[idx];
            if info.combined_clines.is_some() {
                // Already checked
                continue;
            }
            let combined_clines = info.combined_clines(data);
            did_something = did_something || combined_clines.is_some();
            data[idx].combined_clines = combined_clines;
        }
        if !did_something {
            // Cannot resolve further
            break;
        }
    }

    data.iter().all(|x| x.combined_clines.is_some())
}

fn process_data_tree(data: &mut [FileInfo]) {
    //for idx in data.enumerate()
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
    println!("File / Size / Text lines / LoC / # Includes / Combined LoC");
    for it in data {
        print!(
            "{: <34}{: >7}  {: >6}  {: >6}  {: >3}  {: >9}",
            if it.system {
                format!("<{}>", it.name)
            } else {
                it.name.clone()
            },
            fmt_bignum(it.data.len()),
            fmt_bignum(it.lines),
            fmt_bignum(it.clines),
            it.includes.len(),
            if let Some(n) = it.combined_clines {
                fmt_bignum(n)
            } else {
                "?".to_string()
            }
        );

        for inc in it.includes.iter().filter(|x| !x.system).take(6) {
            print!("  {}", inc);
        }
        let num_sys = it.includes.iter().filter(|x| x.system).count();
        if it.includes.len() - num_sys > 6 {
            print!("  ...");
        }
        if num_sys > 0 {
            print!("  +{}", num_sys);
        }
        println!();
    }
    println!(
        "Total files: {} (+{})",
        data.iter().filter(|x| !x.system).count(),
        data.iter().filter(|x| x.system).count()
    );
    let sum: usize = data.iter().map(|x| x.data.len()).sum();
    println!("Total size: {}", fmt_bignum(sum));
    let sum: usize = data.iter().map(|x| x.combined_clines.unwrap_or(0)).sum();
    println!("Total size with includes: {}", fmt_bignum(sum));
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

fn try_extract_include(s: &str) -> Option<IncludeInfo> {
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

    let (name, system) = if s.starts_with("<") {
        // system include
        (extract_include_name(&s[1..], ">"), true)
    } else if s.starts_with("\"") {
        // local include
        (extract_include_name(&s[1..], "\""), false)
    } else {
        // should never happen
        panic!("Shit happened")
    };

    if let Some(name) = name {
        Some(IncludeInfo {
            name: name.to_string(),
            system,
            idx: None,
        })
    } else {
        None
    }
}

fn parse_file_data(data: &str) -> (Vec<IncludeInfo>, usize) {
    let mut ret = Vec::<IncludeInfo>::new();
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
            ret.push(inc);
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
        "ilin" => SortMode::IncLines,
        x => {
            println!("Unknown sort method {}", x);
            return;
        }
    };

    let mut data = load_files(&args().nth(1).unwrap());
    let can_build_tree = process_data_basic(&mut data);
    if can_build_tree {
        process_data_tree(&mut data);
    }
    custom_sort(&mut data, sort_mode);
    debug_print(&data);
    println!("Done.");
}
