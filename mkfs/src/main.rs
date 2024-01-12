use std::{
    cmp::min,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    mem::{size_of, size_of_val},
    vec::Vec,
};

use bytemuck::{bytes_of, cast, cast_slice, checked::from_bytes};
use clap::Parser;

#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(clippy::unreadable_literal)]
#[allow(dead_code)]
mod c_bindings {
    include!(concat!(env!("OUT_DIR"), "/kernel_bindings.rs"));
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct MKFSArgs {
    #[arg(short = 'd', long)]
    disk_path: String,
    #[arg(short, long, default_value_t = 200)]
    num_inodes: usize,
    files: Vec<String>,
}

macro_rules! IBLOCK {
    ($i:expr, $sb:expr) => {
        ($i as usize / INODES_PER_BLOCK) + $sb.inodestart as usize
    };
}

const INODES_PER_BLOCK: usize = c_bindings::BSIZE as usize / size_of::<c_bindings::dinode>();
const NUM_INDIRECT: usize = c_bindings::BSIZE as usize / size_of::<u32>();
const MAX_FILE: usize = c_bindings::NDIRECT as usize + NUM_INDIRECT;

fn main() {
    let args = MKFSArgs::parse();
    assert!(
        c_bindings::BSIZE as usize % size_of::<c_bindings::dinode>() == 0,
        "BSIZE = {}, dinode size = {}",
        c_bindings::BSIZE,
        size_of::<c_bindings::dinode>()
    );
    assert!(
        c_bindings::BSIZE as usize % size_of::<c_bindings::dirent>() == 0,
        "BSIZE = {}, dirent size = {}",
        c_bindings::BSIZE,
        size_of::<c_bindings::dirent>()
    );
    let nbitmap = (c_bindings::FSSIZE / (c_bindings::BSIZE * 8) + 1) as usize;
    let ninodeblocks = args.num_inodes / INODES_PER_BLOCK + 1;
    let nlog = c_bindings::LOGSIZE as usize;
    let mut disk = OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(true)
        .create(true)
        .open(&args.disk_path)
        .unwrap();
    let nmeta = 2usize + nlog + ninodeblocks + nbitmap;
    let nblocks = c_bindings::FSSIZE as usize - nmeta;
    let superblock = c_bindings::superblock {
        magic: c_bindings::FSMAGIC,
        size: c_bindings::FSSIZE.to_le(),
        nblocks: (nblocks as u32).to_le(),
        ninodes: (args.num_inodes as u32).to_le(),
        nlog: (nlog as u32).to_le(),
        logstart: 2u32.to_le(),
        inodestart: (2u32 + nlog as u32).to_le(),
        bmapstart: (2u32 + nlog as u32 + ninodeblocks as u32).to_le(),
    };

    println!("nmeta {nmeta} (boot, super, log blocks {nlog} inode blocks {ninodeblocks}, bitmap blocks {nbitmap}) blocks {nblocks} total {}", c_bindings::FSSIZE);
    let mut freeblock = u32::try_from(nmeta).unwrap();
    disk.set_len((c_bindings::FSSIZE * c_bindings::BSIZE) as u64)
        .unwrap();
    {
        let mut superblock_data = [0u8; c_bindings::BSIZE as usize];
        superblock_data[0..size_of_val(&superblock)].copy_from_slice(bytes_of(&superblock));
        wsect(1, &mut disk, &superblock_data);
    }
    let mut freeinode: u32 = 1;
    let rootino = ialloc(
        c_bindings::T_DIR as i16,
        &mut freeinode,
        &mut disk,
        &superblock,
    );
    assert!(rootino == 1, "rootino = {rootino}");
    let mut dirent = c_bindings::dirent {
        inum: u16::try_from(rootino).unwrap().to_le(),
        name: [0i8; c_bindings::DIRSIZ as usize],
    };
    dirent.name[0] = i8::try_from(b'.').unwrap();
    iappend(
        rootino,
        bytes_of(&dirent),
        &mut freeblock,
        &mut disk,
        &superblock,
    );
    dirent.name[1] = i8::try_from(b'.').unwrap();
    iappend(
        rootino,
        bytes_of(&dirent),
        &mut freeblock,
        &mut disk,
        &superblock,
    );

    for file in args.files {
        let shortname = file.trim_start_matches("user/").trim_start_matches("_");
        assert!(!shortname.contains('/'), "shortname = {}", shortname);
        assert!(
            shortname.len() < c_bindings::DIRSIZ as usize,
            "shortname length = {}, DIRSIZE = {}",
            shortname.len(),
            c_bindings::DIRSIZ
        );
        let inum = ialloc(
            c_bindings::T_FILE as i16,
            &mut freeinode,
            &mut disk,
            &superblock,
        );
        let mut dirent = c_bindings::dirent {
            inum: u16::try_from(inum).unwrap().to_le(),
            name: [0i8; c_bindings::DIRSIZ as usize],
        };
        for (index, byte) in shortname.as_bytes().iter().enumerate() {
            dirent.name[index] = i8::try_from(*byte).unwrap();
        }
        iappend(
            rootino,
            bytes_of(&dirent),
            &mut freeblock,
            &mut disk,
            &superblock,
        );

        let mut file_reader = File::open(file).unwrap();
        let mut file_buffer = [0u8; c_bindings::BSIZE as usize];
        while file_reader.read(&mut file_buffer).unwrap() != 0 {
            iappend(inum, &file_buffer, &mut freeblock, &mut disk, &superblock);
        }
    }

    let mut rootinode = rinode(rootino, &mut disk, &superblock);
    let offset =
        ((u32::from_le(rootinode.size) / c_bindings::BSIZE as u32) + 1) * c_bindings::BSIZE as u32;
    rootinode.size = offset.to_le();
    balloc(freeblock, &mut disk, &superblock);
}

fn wsect(section: u64, file: &mut File, data: &[u8]) {
    assert!(
        data.len() == c_bindings::BSIZE as usize,
        "data.len() = {}, BSIZE = {}",
        data.len(),
        c_bindings::BSIZE
    );
    file.seek(SeekFrom::Start(section * c_bindings::BSIZE as u64))
        .unwrap();
    file.write_all(data).unwrap();
}

fn rsect(section: u64, file: &mut File) -> [u8; c_bindings::BSIZE as usize] {
    file.seek(SeekFrom::Start(section * c_bindings::BSIZE as u64))
        .unwrap();
    let mut buffer = [0u8; c_bindings::BSIZE as usize];
    file.read_exact(&mut buffer).unwrap();
    if section == 46 {
        println!("{:?}", cast_slice::<u8, c_bindings::dinode>(&buffer));
    }
    buffer
}

fn winode(
    inum: u32,
    dinode: &c_bindings::dinode,
    file: &mut File,
    superblock: &c_bindings::superblock,
) {
    let block_number = IBLOCK!(inum, superblock);
    let inode_offset = (inum as usize % INODES_PER_BLOCK) * size_of::<c_bindings::dinode>();
    println!("inum = {inum}, dinode size = {}, INODES_PER_BLOCK = {INODES_PER_BLOCK}, block_number = {block_number}, inode_offset = {inode_offset}", size_of::<c_bindings::dinode>());
    let mut sect_data = rsect(block_number as u64, file);
    sect_data[inode_offset..(inode_offset + size_of::<c_bindings::dinode>())]
        .copy_from_slice(bytes_of(dinode));
    wsect(block_number as u64, file, &sect_data);
}

fn rinode(inum: u32, file: &mut File, superblock: &c_bindings::superblock) -> c_bindings::dinode {
    let block_number = IBLOCK!(inum, superblock);
    let inode_offset = (inum as usize % INODES_PER_BLOCK) * size_of::<c_bindings::dinode>();
    let sect_data = rsect(block_number as u64, file);
    let inode = from_bytes::<c_bindings::dinode>(
        &sect_data[inode_offset..(inode_offset + size_of::<c_bindings::dinode>())],
    )
    .clone();
    if inode.addrs.contains(&46) {
        println!("Addr 46: ({inum:?}): {inode:?}");
    }
    inode
}

fn ialloc(
    ftype: i16,
    freeinode: &mut u32,
    file: &mut File,
    superblock: &c_bindings::superblock,
) -> u32 {
    let inum = *freeinode;
    *freeinode += 1;
    let dinode = c_bindings::dinode {
        type_: ftype.to_le(),
        major: 0,
        minor: 0,
        nlink: 1i16.to_le(),
        size: 0u32,
        addrs: [0u32; 13],
    };
    println!("allocating new inode, inum = {inum}, {:?}", dinode);
    winode(inum, &dinode, file, superblock);
    inum
}

fn balloc(used: u32, file: &mut File, superblock: &c_bindings::superblock) {
    println!("balloc: first {used} blocks have been allocated");
    assert!(
        used < c_bindings::BSIZE as u32 * 8,
        "used = {used}, BSIZE * 8 = {}",
        c_bindings::BSIZE * 8
    );
    let mut buf = [0u8; c_bindings::BSIZE as usize];
    for i in 0..(used as usize) {
        buf[i / 8] = buf[i / 8] | (0x1 << (i % 8));
    }
    println!(
        "balloc: write bitmap block at sector {}",
        superblock.bmapstart
    );
    wsect(superblock.bmapstart as u64, file, &buf);
}

fn iappend(
    inum: u32,
    data: &[u8],
    freeblock: &mut u32,
    file: &mut File,
    superblock: &c_bindings::superblock,
) {
    let mut dinode = rinode(inum, file, superblock);
    let mut offset = u32::from_le(dinode.size) as usize;
    let mut count = data.len();
    while count > 0 {
        let fbn = offset / c_bindings::BSIZE as usize;
        assert!(fbn < MAX_FILE, "fbn = {fbn}, MAX_FILE = {MAX_FILE}");
        let block_num = if fbn < c_bindings::NDIRECT as usize {
            if u32::from_le(dinode.addrs[fbn]) == 0 {
                dinode.addrs[fbn] = freeblock.to_le();
                println!("new direct block = {freeblock}");
                *freeblock += 1;
            }
            println!(
                "direct block fbn = {fbn}, din.addrs = {:?}, block_num = {}",
                dinode.addrs,
                u32::from_le(dinode.addrs[fbn])
            );
            u32::from_le(dinode.addrs[fbn])
        } else {
            if u32::from_le(dinode.addrs[c_bindings::NDIRECT as usize]) == 0 {
                dinode.addrs[c_bindings::NDIRECT as usize] = freeblock.to_le();
                println!("allocating indirect");
                *freeblock += 1;
            }
            let indirect_block = u32::from_le(dinode.addrs[c_bindings::NDIRECT as usize]).into();
            println!("indirect_block = {indirect_block}");
            let mut indirect = cast::<[u8; c_bindings::BSIZE as usize], [u32; NUM_INDIRECT]>(
                rsect(indirect_block, file),
            );
            if u32::from_le(indirect[fbn - c_bindings::NDIRECT as usize]) == 0 {
                indirect[fbn - c_bindings::NDIRECT as usize] = freeblock.to_le();
                println!("new indirect block = {freeblock}");
                *freeblock += 1;
                wsect(
                    u32::from_le(dinode.addrs[c_bindings::NDIRECT as usize]).into(),
                    file,
                    cast_slice(&indirect),
                );
            }
            println!(
                "indirect block fbn = {fbn}, block_num = {}",
                u32::from_le(indirect[fbn - c_bindings::NDIRECT as usize])
            );
            u32::from_le(indirect[fbn - c_bindings::NDIRECT as usize])
        };
        let n1 = min(count, (fbn + 1) * c_bindings::BSIZE as usize - offset);
        println!("block_num = {block_num}, n1 = {n1}, count = {count}");
        let mut data_buf = rsect(block_num as u64, file);
        let start_buf = offset % c_bindings::BSIZE as usize;
        let end_buf = start_buf + n1;
        data_buf[start_buf..end_buf]
            .copy_from_slice(&data[(data.len() - count)..(data.len() - count + n1)]);
        wsect(block_num as u64, file, &data_buf);
        count -= n1;
        offset += n1;
    }
    dinode.size = u32::try_from(offset).unwrap().to_le();
    winode(inum, &dinode, file, superblock);
}
