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
            let write_to_file = hash_object_matches.get_flag("write");

            let blob_sha = hash_object(&filename.into(), write_to_file)?;
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
        Some(("commit-tree", commit_tree_matches)) => {
            let tree_sha: Sha1Hash = commit_tree_matches.get_one::<String>("tree_sha")
                .expect("Tree SHA is required")
                .parse()?;
            let parent: Option<Sha1Hash> = commit_tree_matches.get_one::<String>("parent")
                .map(|s| s.parse().map(Some))
                .transpose()
                .map(|opt| opt.flatten())?;
            let message = commit_tree_matches.get_one::<String>("message")
                .expect("Message is required");

            // println!("tree: {}, parent: {:?}, message: {}", tree_sha, parent, message);
            let mut commit_buf = String::new();
            writeln!(commit_buf, "tree {}", tree_sha)?;
            if let Some(parent) = parent {
                writeln!(commit_buf, "parent {}", parent)?;
            }
            writeln!(commit_buf, "author Noname <noreply@noname.com> 1709990458 +0200")?;
            writeln!(commit_buf, "committer Noname <noreply@noname.com> 1709990458 +0200")?;
            writeln!(commit_buf, "")?;
            writeln!(commit_buf, "{}", message)?;

            let sha1 = write_object("commit", commit_buf.as_bytes(), true)?;
            println!("{}", sha1);
        }

        _ => {
            eprintln!("Invalid command, use --help.");
        }
    }

    Ok(())
}

fn hash_object(filename: &PathBuf, write_to_file: bool) -> anyhow::Result<Sha1Hash> {
    let buf = fs::read(filename)?;
    let sha = write_object("blob", &buf, write_to_file)?;

    Ok(sha)
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

    let sha1 = write_object("tree", &buf, true)?;
    Ok(sha1)
}

fn write_object(kind: &str, content: &[u8], write_to_file: bool) -> anyhow::Result<Sha1Hash> {
    let mut buf = BytesMut::new();
    buf.write_fmt(format_args!("{} {}", kind, content.len()))?;
    buf.put_u8(0);
    buf.put_slice(content);

    let sha1 = Sha1Hash::hash(&buf);

    if write_to_file {
        let directory = directory_from_sha(&sha1)?;
        fs::create_dir_all(&directory)?;

        let filename = filename_from_sha(&sha1)?;
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(filename)?;
        let file = ZlibEncoder::new(file, flate2::Compression::default());

        let mut file = file;
        file.write(&buf)?;
    }

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
                        .required(true)
                        .help("The SHA of the tree to list"),
                ),
        )
        .subcommand(
            Command::new("write-tree")
                .about("Write a tree object from the current index")
        )
        .subcommand(
            Command::new("commit-tree")
                .about("Create a new commit object")
                .arg(
                    Arg::new("tree_sha")
                        .value_name("TREE_SHA")
                        .required(true)
                        .help("The SHA of the tree to commit"),
                )
                .arg(
                    Arg::new("parent")
                        .short('p')
                        .value_name("PARENT")
                        .help("The SHA of the parent commit"),
                )
                .arg(
                    Arg::new("message")
                        .short('m')
                        .value_name("MESSAGE")
                        .required(true)
                        .help("The commit message"),
                ),
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

