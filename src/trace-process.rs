#![feature(iter_array_chunks)]
#![feature(btree_cursors)]

use csv::{ReaderBuilder, WriterBuilder};
use itertools::Itertools;
use proc_modules::Module;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::BufRead;
use std::ops::Bound;
use std::path::Path;
use std::process::Command;

use trace_explorer::trace::Bio;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Symbol {
    Column: u32,
    Discriminator: u32,
    FileName: String,
    FunctionName: String,
    Line: u32,
    StartAddress: String,
    StartFileName: String,
    StartLine: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LlvmSymbolizerItem {
    Address: String,
    ModuleName: String,
    Symbol: Vec<Symbol>,
}

impl LlvmSymbolizerItem {
    fn address_(&self) -> u64 {
        let stripped = self.Address.strip_prefix("0x").unwrap();
        u64::from_str_radix(stripped, 16).unwrap()
    }
}

fn get_bio() {
    let file = File::open("log.csv").unwrap();
    let mut reader = ReaderBuilder::new().flexible(true).from_reader(file);

    let mut bio_list: Vec<Bio> = Vec::new();

    for result in reader.records() {
        let record = result.unwrap();
        let event_type = &record[0];

        if event_type.contains("Attaching") {
            continue;
        }

        let tid: u64 = record[1].parse().unwrap();
        let timestamp: u64 = record[2].parse().unwrap();

        if event_type == "bio_queue" {
            let bio = Bio {
                offset: record[3].parse().unwrap(),
                size: record[4].parse().unwrap(),
                is_metadata: record[5].contains("M"),
                is_flush: record[5].contains("F"),
                is_write: record[5].contains("W"),
                start: timestamp,
                end: None,
            };
            bio_list.push(bio);
            println!("start");
            println!("{:?}", bio_list.last().unwrap());
        } else if event_type == "bio_rq_complete" {
            let offset: u64 = record[3]
                .parse()
                .unwrap_or_else(|e| panic!("{:?}", &record[3]));
            let size: u64 = record[4].parse().unwrap();
            let is_flush = record[5].contains("F");

            if is_flush {
                for bio in bio_list.iter_mut().rev() {
                    if bio.is_flush {
                        bio.end = Some(timestamp);
                        println!("end");
                        println!("{:?}", bio);
                        break;
                    }
                }
            }

            for bio in bio_list.iter_mut() {
                if bio.end.is_none()
                    && bio.offset >= offset
                    && bio.offset + bio.size <= offset + size
                {
                    bio.end = Some(timestamp);
                    println!("end");
                    println!("{:?}", bio);
                }
            }
        }
    }

    dbg!(&bio_list);
}

fn process_stack_traces(stack_traces: HashMap<String, usize>) {
    let mut stack_traces: Vec<(String, usize)> = stack_traces.into_iter().collect();
    stack_traces.sort_by_key(|x| x.1);

    let mut addr_to_loc: HashMap<u64, Option<String>> = HashMap::new();

    let stack_traces: Vec<_> = stack_traces
        .into_iter()
        .map(|(stack_trace, _)| {
            stack_trace
                .split('\n')
                .filter_map(|x| {
                    let addr: u64 = u64::from_str_radix(x, 16).ok()?;
                    addr_to_loc.entry(addr).or_insert(None);
                    Some(addr)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    dbg!(&stack_traces);

    let curr_text_addr = kernel_text_addr();
    dbg!(curr_text_addr);
    let vmlinux_text_addr = vmlinux_text_addr(Path::new("/home/mike/tmp/modules/vmlinux"));
    dbg!(vmlinux_text_addr);
    let offset = (vmlinux_text_addr as i64 - curr_text_addr as i64) as i64;
    resolve_addr(&mut addr_to_loc, offset);

    let mut writer = WriterBuilder::new()
        .flexible(false)
        .from_writer(std::fs::File::create("stack.csv").unwrap());

    for (i, s) in stack_traces.iter().enumerate() {
        let record = [
            i.to_string(),
            s.iter()
                .map(|x| addr_to_loc[x].as_ref().unwrap())
                .join("\n"),
        ];
        writer.write_record(record).unwrap();
    }
}

fn resolve_addr(addr_to_line: &mut HashMap<u64, Option<String>>, vmlinux_offset: i64) {
    let modules: BTreeMap<u64, Module> = proc_modules::ModuleIter::new()
        .unwrap()
        .filter_map(|m| {
            let m = m.unwrap();
            Some((m.base?, m))
        })
        .collect();

    let mut addr_per_module = HashMap::new();
    let mut vmlinux_addr = HashSet::new();

    for addr in addr_to_line.keys() {
        let mut module = modules.upper_bound(Bound::Included(&addr));
        if let Some((base, module)) = module.prev() {
            let (_, set) = addr_per_module
                .entry(module.module.clone())
                .or_insert_with(|| (-(*base as i64), HashSet::new()));
            set.insert(*addr);
        } else {
            vmlinux_addr.insert(*addr);
        };
    }
    addr_per_module.insert("vmlinux".to_string(), (vmlinux_offset, vmlinux_addr));

    for (module, (base, addrs)) in &addr_per_module {
        let path = format!(
            "/home/mike/tmp/modules/{}",
            if module == "vmlinux" {
                "vmlinux".to_string()
            } else {
                format!("{}.ko", module)
            }
        );
        let output = Command::new("llvm-symbolizer")
            .arg("--output-style=JSON")
            .arg("--obj")
            .arg(&path)
            .args(addrs.iter().map(|x| format!("{:#x}", (*x as i64) + base)))
            .output()
            .unwrap();
        dbg!(&output);
        let items: Vec<LlvmSymbolizerItem> = serde_json::from_slice(&output.stdout).unwrap();
        dbg!(&items);
        for i in items {
            let addr = ((i.address_() as i64) - base) as u64;
            let loc = addr_to_line.get_mut(&addr).unwrap();
            let s = i
                .Symbol
                .iter()
                .map(|x| format!("{}\t{}:{}:{}", x.FunctionName, x.FileName, x.Line, x.Column))
                .join("\n");
            dbg!(&s);
            loc.replace(s);
        }
    }

    dbg!(&addr_per_module);
}

fn vmlinux_text_addr(vmlinux: &Path) -> u64 {
    // use readelf to get the address of .text section
    let output = std::process::Command::new("readelf")
        .arg("-S")
        .arg(vmlinux)
        .output()
        .unwrap();
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        if line.contains(".text") {
            let parts: Vec<_> = line.split_whitespace().collect();
            return u64::from_str_radix(parts[4], 16).unwrap();
        }
    }
    unreachable!()
}

fn kernel_text_addr() -> u64 {
    // read file /proc/kallsyms to get the address of stext
    let file = File::open("/proc/kallsyms").unwrap();
    let reader = std::io::BufReader::new(file);
    for line in reader.lines() {
        let line = line.unwrap();
        let mut parts = line.split_whitespace();
        let addr = parts.next().unwrap();
        parts.next().unwrap();
        let name = parts.next().unwrap();
        if name == "_stext" {
            return u64::from_str_radix(addr, 16).unwrap();
        }
    }
    unreachable!()
}

fn main() {
    let file = File::open("log.csv").unwrap();
    let mut reader = ReaderBuilder::new().flexible(true).from_reader(file);
    let writer = std::fs::File::create("output.csv").unwrap();
    let mut writer = WriterBuilder::new().flexible(true).from_writer(writer);

    let mut stack_traces: HashMap<String, usize> = HashMap::new();
    let mut stack_trace_id = 0;

    for result in reader.records() {
        let record = result.unwrap();
        let new_record = record.iter().map(|x| {
            if x.contains('\n') {
                let curr_stack_trace = x.to_owned();
                stack_traces
                    .entry(curr_stack_trace)
                    .or_insert_with(|| {
                        let ret = stack_trace_id;
                        stack_trace_id += 1;
                        ret
                    })
                    .to_string()
            } else {
                x.to_owned()
            }
        });
        writer.write_record(new_record).unwrap();
    }

    process_stack_traces(stack_traces);
}
