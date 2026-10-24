// Copyright 2018-2026 the Deno authors. MIT license.
// Matches deno_fs 0.169.0's private SerializableStat wire format.
use deno_core::ToV8;
use deno_runtime::deno_io::fs::FsStat;

#[derive(ToV8)]
pub(super) struct Stat {
    is_file: bool,
    is_directory: bool,
    is_symlink: bool,
    size: u64,
    mtime_set: bool,
    mtime: i64,
    atime_set: bool,
    atime: i64,
    birthtime_set: bool,
    birthtime: i64,
    ctime_set: bool,
    ctime: i64,
    dev: u64,
    ino_set: bool,
    ino: u64,
    mode: u32,
    nlink_set: bool,
    nlink: u64,
    uid: u32,
    gid: u32,
    rdev: u64,
    blksize: u64,
    blocks_set: bool,
    blocks: u64,
    is_block_device: bool,
    is_char_device: bool,
    is_fifo: bool,
    is_socket: bool,
}

impl From<FsStat> for Stat {
    fn from(stat: FsStat) -> Self {
        Self {
            is_file: stat.is_file,
            is_directory: stat.is_directory,
            is_symlink: stat.is_symlink,
            size: stat.size,
            mtime_set: stat.mtime.is_some(),
            mtime: stat.mtime.unwrap_or(0),
            atime_set: stat.atime.is_some(),
            atime: stat.atime.unwrap_or(0),
            birthtime_set: stat.birthtime.is_some(),
            birthtime: stat.birthtime.unwrap_or(0),
            ctime_set: stat.ctime.is_some(),
            ctime: stat.ctime.unwrap_or(0),
            dev: stat.dev,
            ino_set: stat.ino.is_some(),
            ino: stat.ino.unwrap_or(0),
            mode: stat.mode,
            nlink_set: stat.nlink.is_some(),
            nlink: stat.nlink.unwrap_or(0),
            uid: stat.uid,
            gid: stat.gid,
            rdev: stat.rdev,
            blksize: stat.blksize,
            blocks_set: stat.blocks.is_some(),
            blocks: stat.blocks.unwrap_or(0),
            is_block_device: stat.is_block_device,
            is_char_device: stat.is_char_device,
            is_fifo: stat.is_fifo,
            is_socket: stat.is_socket,
        }
    }
}

impl Stat {
    pub(super) fn write(self, output: &mut [u32]) {
        let fields = [
            self.is_file as u64,
            self.is_directory as u64,
            self.is_symlink as u64,
            self.size,
            self.mtime_set as u64,
            self.mtime.cast_unsigned(),
            self.atime_set as u64,
            self.atime.cast_unsigned(),
            self.birthtime_set as u64,
            self.birthtime.cast_unsigned(),
            self.ctime_set as u64,
            self.ctime.cast_unsigned(),
            self.dev,
            self.ino_set as u64,
            self.ino,
            self.mode as u64,
            self.nlink_set as u64,
            self.nlink,
            self.uid as u64,
            self.gid as u64,
            self.rdev,
            self.blksize,
            self.blocks_set as u64,
            self.blocks,
            self.is_block_device as u64,
            self.is_char_device as u64,
            self.is_fifo as u64,
            self.is_socket as u64,
        ];
        for (field, pair) in fields.into_iter().zip(output.as_chunks_mut::<2>().0) {
            pair[0] = (field & 0xffff_ffff) as u32;
            pair[1] = (field >> 32) as u32;
        }
    }
}
