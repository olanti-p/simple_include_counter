#![allow(dead_code)]

use num_format::{Buffer, CustomFormat, Grouping, ToFormattedStr};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::env::args;
use std::fs::File;
use std::io::Read;

struct IncludeInfo {
    name: String,
    /// true if was included as <header>, false if as "header"
    system: bool,
}

struct FileInfo {
    name: String,
    data: String,
    stab_file: bool,                   // stab for a missing file
    source_file: bool,                 // is source file (.cpp)
    text_lines: usize,                 // source file lines
    lines: usize,                      // code lines
    parsed_includes: Vec<IncludeInfo>, // includes, as parsed

    includes: Vec<usize>,
    included_by: Vec<usize>,

    includes_indirect: Vec<usize>,
    included_by_indirect: Vec<usize>,

    lines_with_all_includes: usize, // code lines with all direct & indirect includes counted once
    lines_contributes_total: usize, // code lines contribution (with all direct & indirect includes) to all direct & indirect includers
    lines_contributes_self: usize, // code lines contribution (this file only) to all direct & indirect includers
}

impl FileInfo {
    pub fn new(name: String, data: String, stab: bool, source_file: bool) -> Self {
        Self {
            name,
            data,
            stab_file: stab,
            source_file,
            text_lines: 0,
            lines: 0,
            parsed_includes: vec![],
            includes: vec![],
            included_by: vec![],
            includes_indirect: vec![],
            included_by_indirect: vec![],
            lines_with_all_includes: 0,
            lines_contributes_total: 0,
            lines_contributes_self: 0,
        }
    }
}

impl std::fmt::Display for FileInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stab_file {
            write!(f, "<{}>", self.name)
        } else {
            write!(f, "{}", self.name)
        }
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

        let source_file = name.ends_with(".cpp");
        ret.push(FileInfo::new(name, data, false, source_file));
    }

    ret
}

enum SortMode {
    FileName,
    FileSize,
    NumIncludes,
    TextLines,
    Lines,
    LinesWithAllIncludes,
    ContribWithAllIncludes,
    ContribSelfOnly,
}

fn custom_sort(data: &[FileInfo], mode: SortMode, dir: bool) -> Vec<usize> {
    let mut ret: Vec<usize> = (0..data.len()).collect();

    // First sort by name
    ret.sort_by(|a, b| data[*a].name.cmp(&data[*b].name));

    // Then sort by whatever else

    let mut sort_func: Box<dyn Fn(&FileInfo, &FileInfo) -> Ordering> = match mode {
        SortMode::FileName => Box::new(|_, _| Ordering::Equal),
        SortMode::FileSize => Box::new(|a, b| a.data.len().cmp(&b.data.len())),
        SortMode::NumIncludes => Box::new(|a, b| a.includes.len().cmp(&b.includes.len())),
        SortMode::Lines => Box::new(|a, b| a.lines.cmp(&b.lines)),
        SortMode::TextLines => Box::new(|a, b| a.text_lines.cmp(&b.text_lines)),
        SortMode::LinesWithAllIncludes => {
            Box::new(|a, b| a.lines_with_all_includes.cmp(&b.lines_with_all_includes))
        }
        SortMode::ContribWithAllIncludes => {
            Box::new(|a, b| a.lines_contributes_total.cmp(&b.lines_contributes_total))
        }
        SortMode::ContribSelfOnly => {
            Box::new(|a, b| a.lines_contributes_self.cmp(&b.lines_contributes_self))
        }
    };
    if dir {
        sort_func = Box::new(move |a, b| sort_func(a, b).reverse());
    }
    ret.sort_by(|a, b| sort_func(&data[*a], &data[*b]));

    // Then sort external includes & sources to the bottom
    ret.sort_by(|a, b| match (data[*a].stab_file, data[*b].stab_file) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => match (data[*a].source_file, data[*b].source_file) {
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => Ordering::Equal,
        },
    });

    ret
}

/// Returns all mentioned files by their names
fn process_step_parse(data: &mut [FileInfo]) -> HashSet<String> {
    let mut ret = HashSet::<String>::new();
    for d in data.iter_mut() {
        d.text_lines = count_file_lines(&d.data);
        let (includes, clines) = parse_file_data(&d.data);
        d.parsed_includes = includes;
        d.lines = clines;
        for ii in &d.parsed_includes {
            ret.insert(ii.name.to_string());
        }
    }
    ret
}

/// Generate stubs for missing include files
fn process_step_generate_stubs(data: &mut Vec<FileInfo>, all: &HashSet<String>) {
    for name in all {
        if data.iter().any(|x| &x.name == name) {
            // Found
            continue;
        }
        data.push(FileInfo::new(name.clone(), "".to_string(), true, false));
    }
}

/// Link includers and includees
fn process_step_link_include(data: &mut [FileInfo]) {
    for idx in 0..data.len() {
        for idx2 in 0..data[idx].parsed_includes.len() {
            let that_name = &data[idx].parsed_includes[idx2].name;
            let idx_that = data
                .iter()
                .enumerate()
                .find_map(|(idx, x)| {
                    if &x.name == that_name {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .unwrap();

            data[idx_that].included_by.push(idx);
            data[idx].includes.push(idx_that);
        }
    }
}

struct CircCheck {
    idx: usize,
    included_by: Vec<usize>,
}

/// Check circular dependencies
fn process_step_check_circular(data: &[FileInfo]) -> Option<(usize, usize)> {
    let mut all: Vec<CircCheck> = data
        .iter()
        .enumerate()
        .map(|(idx, x)| CircCheck {
            idx,
            included_by: x.included_by.clone(),
        })
        .collect();

    loop {
        let mut did_something = false;
        for i in 0..all.len() {
            let idx_this = all[i].idx;
            if all[i].included_by.is_empty() {
                all.remove(i);
                for elem in &mut all {
                    elem.included_by.retain(|x| *x != idx_this);
                }
                did_something = true;
                break;
            }
        }
        if !did_something {
            break;
        }
    }

    if all.is_empty() {
        None
    } else {
        Some((all[0].idx, all[0].included_by[0]))
    }
}

fn recurse_collect_includes(data: &[FileInfo], idx: usize, ret: &mut HashSet<usize>) {
    for idx2 in &data[idx].includes {
        ret.insert(*idx2);
        recurse_collect_includes(data, *idx2, ret);
    }
}

fn recurse_collect_included_by(data: &[FileInfo], idx: usize, ret: &mut HashSet<usize>) {
    for idx2 in &data[idx].included_by {
        ret.insert(*idx2);
        recurse_collect_included_by(data, *idx2, ret);
    }
}

/// Link indirect inclusions
fn process_step_link_include_indirect(data: &mut [FileInfo]) {
    for idx in 0..data.len() {
        let mut temp = HashSet::<usize>::new();
        recurse_collect_includes(data, idx, &mut temp);
        data[idx].includes_indirect = temp.into_iter().collect();

        let mut temp = HashSet::<usize>::new();
        recurse_collect_included_by(data, idx, &mut temp);
        data[idx].included_by_indirect = temp.into_iter().collect();
    }
}

/// Calculate cost of this file with all includes
fn process_step_calc_costs(data: &mut [FileInfo]) {
    for idx in 0..data.len() {
        let sum: usize = data[idx]
            .includes_indirect
            .iter()
            .map(|x| data[*x].lines)
            .sum();
        data[idx].lines_with_all_includes = data[idx].lines + sum;
        data[idx].lines_contributes_self = data[idx].lines * data[idx].included_by_indirect.len();
        data[idx].lines_contributes_total =
            data[idx].lines_with_all_includes * data[idx].included_by_indirect.len();
    }
}

/// Returns whether it's possible to build a tree without circular dependencies
fn process_data(data: &mut Vec<FileInfo>) -> bool {
    eprintln!("Parsing files...");
    let to_add = process_step_parse(data);
    eprintln!("Generating stubs for missing includes...");
    process_step_generate_stubs(data, &to_add);
    eprintln!("Resolving include relations...");
    process_step_link_include(data);
    eprintln!("Checking circular dependencies...");
    if let Some((a, b)) = process_step_check_circular(data) {
        eprintln!("Circular dependency detected: {} <-> {}", data[a], data[b]);
        return false;
    }
    eprintln!("Resolving indirect includes...");
    process_step_link_include_indirect(data);
    eprintln!("Calculating include costs...");
    process_step_calc_costs(data);
    true
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

fn debug_print(data: &[FileInfo], sort_mode: SortMode, sort_dir: bool) {
    let sorted = custom_sort(data, sort_mode, sort_dir);

    println!(
          "File                                 Size  L.Text  L.Code   In / All  Out / All      Combined  ContribSelf    Heaviest headers that include this one"
    );

    for sorted_idx in sorted {
        let it = &data[sorted_idx];

        let name = if it.stab_file {
            format!("<{}>", it.name)
        } else {
            it.name.clone()
        };
        print!(
            "{: <34}{: >7}  {: >6}  {: >6}  {: >3} / {: >3}  {: >3} / {: >3}  {: >11} {: >11}",
            name,
            fmt_bignum(it.data.len()),
            fmt_bignum(it.text_lines),
            fmt_bignum(it.lines),
            fmt_bignum(it.includes.len()),
            fmt_bignum(it.includes_indirect.len()),
            fmt_bignum(it.included_by.len()),
            fmt_bignum(it.included_by_indirect.len()),
            fmt_bignum(it.lines_with_all_includes),
            fmt_bignum(it.lines_contributes_self),
        );

        let mut incl_by = it.included_by.clone();
        incl_by.sort_by(|a, b| {
            let a = data[*a].lines_contributes_self;
            let b = data[*b].lines_contributes_self;
            a.cmp(&b).reverse()
        });
        for inc in incl_by {
            if data[inc].stab_file {
                print!("  <{}>", data[inc].name);
            } else {
                print!("  {}", data[inc].name);
            }
        }
        println!();
    }

    println!(
        "Total files: {} sources, {} includes, {} other includes",
        data.iter().filter(|x| x.source_file).count(),
        data.iter()
            .filter(|x| !x.source_file && !x.stab_file)
            .count(),
        data.iter().filter(|x| x.stab_file).count()
    );

    let sum: usize = data.iter().map(|x| x.lines).sum();
    println!("Total code lines: {}", fmt_bignum(sum));

    let sum: usize = data
        .iter()
        .map(|x| {
            if x.source_file {
                x.lines_with_all_includes
            } else {
                0
            }
        })
        .sum();
    println!("Total compiled code lines: {}", fmt_bignum(sum));
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
    if args().len() != 4 {
        eprintln!("Expected 3 args: dir path, sort mode, sort dir");
        return;
    }

    let sort_mode = match args().nth(2).unwrap().as_str() {
        "fname" => SortMode::FileName,
        "fsize" => SortMode::FileSize,
        "num_includes" => SortMode::NumIncludes,
        "code_lines" => SortMode::Lines,
        "text_lines" => SortMode::TextLines,
        "code_lines_total" => SortMode::LinesWithAllIncludes,
        "cont_self" => SortMode::ContribSelfOnly,
        "cont_total" => SortMode::ContribWithAllIncludes,
        x => {
            eprintln!("Unknown sort method '{}'", x);
            return;
        }
    };

    let sort_dir = match args().nth(3).unwrap().as_str() {
        "norm" => false,
        "rev" => true,
        x => {
            eprintln!("Unknown sort dir '{}'", x);
            return;
        }
    };

    let mut data = load_files(&args().nth(1).unwrap());
    if !process_data(&mut data) {
        eprintln!("Failed.");
        return;
    }
    eprintln!("Writing...");
    debug_print(&data, sort_mode, sort_dir);
    eprintln!("Done.");
}
