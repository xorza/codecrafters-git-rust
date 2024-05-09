extern crate core;

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use anyhow::anyhow;
use bytes::{BufMut, BytesMut};
use clap::{Arg, ArgAction, ArgMatches, Command};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::sha1hash::Sha1Hash;

mod sha1hash;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match get_matches().subcommand() {
        Some(("init", _)) => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n")?;
            println!("Initialized git directory")
        }
        Some(("cat-file", cat_file_matches)) => {
            let blob_sha: Sha1Hash = cat_file_matches.get_one::<String>("blob_sha")
                .expect("Blob SHA is required")
                .parse()?;
            let filename = filename_from_sha(&blob_sha)?;
            let file = fs::File::open(filename)?;
            let decoder = ZlibDecoder::new(file);
            let mut reader = BufReader::new(decoder);

            let mut buf = Vec::new();
            reader.read_until(0, &mut buf)?;

            let text = std::str::from_utf8(&buf[..buf.len() - 1])?;
            let text = text.strip_prefix("blob ").ok_or(anyhow!("Invalid blob"))?;
            let size: usize = text.parse()?;
            buf.resize(size, 0);
            reader.read_exact(&mut buf)?;

            let content = std::str::from_utf8(&buf)?;
            print!("{}", content);
        }
        Some(("hash-object", hash_object_matches)) => {
            let filename = hash_object_matches.get_one::<String>("file")
                .expect("File argument is required")
                .as_str();
            let should_write = hash_object_matches.get_flag("write");

            let blob_sha = hash_object(&filename.into(), should_write)?;
            println!("{}", blob_sha);
        }
        Some(("ls-tree", ls_tree_matches)) => {
            let tree_sha: Sha1Hash = ls_tree_matches.get_one::<String>("tree_sha")
                .expect("Tree SHA is required")
                .parse()?;
            let name_only = ls_tree_matches.get_flag("name-only");

            let filename = filename_from_sha(&tree_sha)?;
            let file = fs::File::open(filename)?;
            let decoder = ZlibDecoder::new(file);
            let mut buf_reader = BufReader::new(decoder);

            let mut buf = Vec::new();
            let read = buf_reader.read_until(0, &mut buf)?;
            let str = std::str::from_utf8(&buf[..read - 1])?;
            let str = str.strip_prefix("tree ").ok_or(anyhow!("Invalid tree"))?;
            let size: usize = str.parse()?;

            let mut left = size;
            while left > 0 {
                buf.clear();
                let read = buf_reader.read_until(0, &mut buf)?;
                let (mode, name) = std::str::from_utf8(&buf[..read - 1])?
                    .split_once(' ')
                    .ok_or(anyhow!("Invalid tree entry"))?;
                let (mode, name) = (u32::from_str_radix(mode, 8)?, name.to_string());

                buf.resize(20, 0);
                buf_reader.read_exact(&mut buf)?;
                let sha = hex::encode(&buf);

                if name_only {
                    println!("{}", name);
                } else {
                    if mode == 0o40000 {
                        println!("{:06o} tree {} {}", mode, name, sha);
                    } else {
                        println!("{:06o} blob {} {}", mode, name, sha);
                    }
                }

                left -= read + 20;
            }
        }
        Some(("write-tree", _)) => {
            let sha1 = write_tree(&".".into())?;
            println!("{}", sha1);
        },

        _ => {
            eprintln!("Invalid command, use --help.");
        }
    }

    Ok(())
}

fn hash_object(filename: &PathBuf, should_write: bool) -> anyhow::Result<Sha1Hash> {
    let mut input_file = fs::File::open(&filename)?;
    let size = input_file.metadata()?.len();

    let mut buf = BytesMut::new();
    buf.write_str("blob ")?;
    buf.write_str(&size.to_string())?;
    buf.put_u8(0);
    let start_content = buf.len();
    buf.resize(start_content + size as usize, 0);

    input_file.read_exact(&mut buf[start_content..])?;

    let blob_sha: Sha1Hash = Sha1Hash::hash(&buf);

    if should_write {
        write_object(&buf, Some(blob_sha.clone()))?;
        // println!("Written blob {} {}", blob_sha, filename.display());
    }

    Ok(blob_sha)
}

fn write_tree(path: &PathBuf) -> anyhow::Result<Sha1Hash> {
    let dir_entries = fs::read_dir(path)?;
    let mut entries = Vec::new();

    for entry in dir_entries {
        let entry = entry?;
        let name = entry.path();

        let last_name = name.file_name()
            .ok_or(anyhow!("Invalid file name"))?
            .to_str()
            .ok_or(anyhow!("Invalid file name"))?
            .to_string();
        if last_name.starts_with(".") {
            continue;
        }

        let metadata = entry.metadata()?;
        let mode: u32 = if metadata.is_dir() { 0o40000 } else { 0o100644 };

        let sha = if metadata.is_dir() {
            write_tree(&entry.path())?
        } else {
            hash_object(&entry.path(), true)?
        };

        entries.push((mode, last_name, sha));
    }

    entries.sort_by(|a, b| a.1.cmp(&b.1));
    
    let mut buf = BytesMut::new();
    for (mode, name, sha) in entries {
        buf.write_fmt(format_args!("{:o} {}", mode, name))?;
        buf.put_u8(0);
        buf.put_slice(sha.as_ref());
    }
    let buf = buf.freeze();
    let mut object_buf = BytesMut::with_capacity(buf.len() + 32);
    object_buf.write_fmt(format_args!("tree {}", buf.len()))?;
    object_buf.put_u8(0);
    object_buf.put_slice(&buf);

    let sha1 = write_object(&object_buf, None)?;

    // println!("Written tree {} {}", sha1, path.display());

    Ok(sha1)
}

fn write_object(buf: &[u8], sha1: Option<Sha1Hash>) -> anyhow::Result<Sha1Hash> {
    let sha1 = sha1.unwrap_or_else(|| Sha1Hash::hash(&buf));

    let directory = directory_from_sha(&sha1)?;
    fs::create_dir_all(&directory)?;

    let filename = filename_from_sha(&sha1)?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(filename)?;
    let mut file = ZlibEncoder::new(file, flate2::Compression::default());
    let mut file = file;
    file.write(buf)?;

    Ok(sha1)
}

fn get_matches() -> ArgMatches {
    Command::new("Rust Git")
        .version("0.1.0")
        .author("xxorza")
        .about("A simple git implementation in Rust")
        .subcommand(Command::new("init").about("Initialize a new git repository"))
        .subcommand(
            Command::new("cat-file")
                .about("Prints the contents of a git object")
                .arg(
                    Arg::new("blob_sha")
                        .short('p')
                        .required(true)
                        .value_name("BLOB_SHA")
                        .help("The SHA of the blob to print"),
                ),
        )
        .subcommand(
            Command::new("hash-object")
                .about("Compute object ID and optionally creates a blob from a file")
                .arg(
                    Arg::new("write")
                        .short('w')
                        .action(ArgAction::SetTrue)
                        .help("Write the object into the object database"),
                )
                .arg(
                    Arg::new("file")
                        .value_name("FILE")
                        .help("Read the object from the given file"),
                ),
        )
        .subcommand(
            Command::new("ls-tree")
                .about("List the contents of a tree object")
                .arg(
                    Arg::new("name-only")
                        .long("name-only")
                        .action(ArgAction::SetTrue)
                        .help("Only show names of tree entries"),
                )
                .arg(
                    Arg::new("tree_sha")
                        .value_name("TREE_SHA")
                        .help("The SHA of the tree to list"),
                ),
        )
        .subcommand(
            Command::new("write-tree")
                .about("Write a tree object from the current index")
        )
        .get_matches()
}

fn filename_from_sha(sha: &Sha1Hash) -> anyhow::Result<String> {
    let str = sha.to_string();
    Ok(format!(".git/objects/{}/{}", &str[..2], &str[2..]))
}

fn directory_from_sha(sha: &Sha1Hash) -> anyhow::Result<String> {
    Ok(format!(".git/objects/{}", &sha.to_string()[..2]))
}

