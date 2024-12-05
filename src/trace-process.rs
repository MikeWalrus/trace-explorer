#![feature(iter_array_chunks)]

use csv::{ReaderBuilder, WriterBuilder};
use itertools::Itertools;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;

#[derive(Debug)]
struct Bio {
    offset: u64,
    size: u64,
    is_metadata: bool,
    is_flush: bool,
    is_write: bool,
    start: u64,
    end: Option<u64>,
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

    let mut addr_to_line: BTreeMap<u64, Option<String>> = BTreeMap::new();

    let stack_traces: Vec<_> = stack_traces
        .into_iter()
        .map(|(stack_trace, _)| {
            stack_trace
                .split('\n')
                .filter_map(|x| {
                    dbg!(x);
                    let addr: u64 = u64::from_str_radix(x, 16).ok()?;
                    addr_to_line.entry(addr).or_insert(None);
                    Some(addr)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    dbg!(&stack_traces);
    dbg!(&addr_to_line);

    let curr_text_addr = kernel_text_addr();
    dbg!(curr_text_addr);
    let vmlinux_text_addr = vmlinux_text_addr(Path::new("./vmlinux"));
    dbg!(vmlinux_text_addr);
    let offset = (vmlinux_text_addr as i64 - curr_text_addr as i64) as i64;
    addr2line(&mut addr_to_line, offset);

    let mut writer = WriterBuilder::new()
        .flexible(false)
        .from_writer(std::fs::File::create("stack.csv").unwrap());
    for (i, s) in stack_traces.iter().enumerate() {
        let record = [
            i.to_string(),
            s.iter()
                .map(|x| addr_to_line[x].as_ref().unwrap())
                .join("\n"),
        ];
        writer.write_record(record).unwrap();
    }
}

fn addr2line(addr_to_line: &mut BTreeMap<u64, Option<String>>, offset: i64) {
    let output = std::process::Command::new("addr2line")
        .arg("--functions")
        .arg("--inlines")
        .arg("-e")
        .arg("./vmlinux")
        .args(
            addr_to_line
                .keys()
                .map(|s| format!("{:#x}", (*s as i64) + offset)),
        )
        .output()
        .unwrap();
    let output = String::from_utf8(output.stdout).unwrap();
    let lines = output.split('\n');
    let results = lines.array_chunks();
    for ((_, result), [function, source]) in addr_to_line.iter_mut().zip(results) {
        result.replace(format!("{}\t{}", function, source));
    }
    dbg!(addr_to_line);
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
    let mut reader = std::io::BufReader::new(file);
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
                        stack_trace_id += 1;
                        stack_trace_id
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
