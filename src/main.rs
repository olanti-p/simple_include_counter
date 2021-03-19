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
    stab: bool, // stab for a missing file

    includes: Vec<IncludeInfo>,
    included_by: Vec<usize>,

    lines: usize,  // source file lines
    clines: usize, // code lines

    combined_clines: Option<usize>,   // code lines with includes
    contributes_total: Option<usize>, // contributes lines in total
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
            stab: false,
            lines: 0,
            clines: 0,
            includes: vec![],
            included_by: vec![],
            combined_clines: None,
            contributes_total: None,
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
    Contrib,
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
        SortMode::Contrib => {
            data.sort_by(|a, b| match (a.contributes_total, b.contributes_total) {
                (None, None) => Ordering::Equal,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(al), Some(bl)) => al.cmp(&bl).reverse(),
            })
        }
    }
    // Then sort system stuff to the bottom
    data.sort_by(|a, b| match (a.stab, b.stab) {
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

    // Step 2. Add stabs for missing included
    let mut to_add = HashSet::<String>::new();
    for d in data.iter() {
        for inc in &d.includes {
            if data.iter().any(|x| x.name == inc.name) {
                // already be on the list
                continue;
            }
            to_add.insert(inc.name.clone());
        }
    }
    data.extend(to_add.into_iter().map(|name| FileInfo {
        name,
        data: "".to_string(),
        stab: true,
        includes: vec![],
        included_by: vec![],
        lines: 1,
        clines: 1,
        combined_clines: Some(1),
        contributes_total: None,
    }));

    // Step 3. Index includes for quick access
    let hashed: HashMap<String, usize> = data
        .iter()
        .enumerate()
        .map(|(idx, x)| (x.name.clone(), idx))
        .collect();
    for d in data.iter_mut() {
        for i in &mut d.includes {
            i.idx = Some(*hashed.get(&i.name).unwrap());
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
    // Link includers
    for idx in 0..data.len() {
        for ii in &data[idx].includes {
            data[ii.idx.unwrap()].included_by.push(idx);
        }
    }

    // Calculate contribution
    for d in data.iter_mut() {
        if let Some(clines) = d.combined_clines {
            d.contributes_total = Some(d.included_by.len() * clines);
        }
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
    println!(
          "File                                 Size  L.Text  L.Code   In  Out     Contrib    Combined  Heaviest headers that include this one"
    );
    for it in data {
        let name = if it.stab {
            format!("<{}>", it.name)
        } else {
            it.name.clone()
        };
        let combined_loc = if let Some(n) = it.combined_clines {
            fmt_bignum(n)
        } else {
            "?".to_string()
        };
        let contributes_loc = if let Some(n) = it.contributes_total {
            fmt_bignum(n)
        } else {
            "?".to_string()
        };
        print!(
            "{: <34}{: >7}  {: >6}  {: >6}  {: >3}  {: >3} {: >11} {: >11}",
            name,
            fmt_bignum(it.data.len()),
            fmt_bignum(it.lines),
            fmt_bignum(it.clines),
            it.includes.len(),
            it.included_by.len(),
            contributes_loc,
            combined_loc
        );

        let mut incl_by = it.included_by.clone();
        incl_by.sort_by(|a, b| {
            let a = data[*a].contributes_total.unwrap_or(0);
            let b = data[*b].contributes_total.unwrap_or(0);
            a.cmp(&b).reverse()
        });
        for inc in incl_by {
            if data[inc].stab {
                print!("  <{}>", data[inc].name);
            } else {
                print!("  {}", data[inc].name);
            }
        }
        println!();
    }
    println!(
        "Total files: {} (+{})",
        data.iter().filter(|x| !x.stab).count(),
        data.iter().filter(|x| x.stab).count()
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
        "cont" => SortMode::Contrib,
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
