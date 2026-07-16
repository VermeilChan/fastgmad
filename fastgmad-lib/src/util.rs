#![allow(dead_code)]

use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom, StdinLock, Write},
    path::Path,
};

pub fn is_hidden_file(path: &Path) -> Result<bool, std::io::Error> {
    let hidden;

    #[cfg(unix)]
    {
        if let Some(file_name) = path.file_name() {
            use std::os::unix::prelude::OsStrExt;
            hidden = file_name.as_bytes().starts_with(b".");
        } else {
            hidden = false;
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x00000002;
        hidden = std::fs::metadata(path)?.file_attributes() & HIDDEN != 0;
    }

    Ok(hidden)
}

pub trait WriteEx: Write {
    fn write_nul_str(&mut self, bytes: &[u8]) -> Result<(), std::io::Error>;
}
impl<W: Write> WriteEx for W {
    fn write_nul_str(&mut self, bytes: &[u8]) -> Result<(), std::io::Error> {
        self.write_all(bytes)?; // Write the bytes
        self.write_all(&[0u8])?; // Write the null terminator
        Ok(())
    }
}

pub trait BufReadEx: BufRead {
    fn skip_until(&mut self, delim: u8) -> Result<usize, std::io::Error> {
        // https://github.com/rust-lang/rust/pull/98943
        let mut read = 0;
        loop {
            let (done, used) = {
                let available = match self.fill_buf() {
                    Ok(n) => n,
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                };
                match memchr::memchr(delim, available) {
                    Some(i) => (true, i + 1),
                    None => (false, available.len()),
                }
            };
            self.consume(used);
            read += used;
            if done || used == 0 {
                return Ok(read);
            }
        }
    }

    fn read_nul_str<'a>(&mut self, buf: &'a mut Vec<u8>) -> Result<&'a mut [u8], std::io::Error>;
    fn skip_nul_str(&mut self) -> Result<(), std::io::Error>;
}
impl<R: BufRead> BufReadEx for R {
    fn read_nul_str<'a>(&mut self, buf: &'a mut Vec<u8>) -> Result<&'a mut [u8], std::io::Error> {
        let read = self.read_until(0u8, buf)?;
        Ok(&mut buf[0..read.saturating_sub(1)])
    }

    fn skip_nul_str(&mut self) -> Result<(), std::io::Error> {
        BufReadEx::skip_until(self, 0).map(|_| ())
    }
}

pub trait IoSkip {
    fn skip(&mut self, bytes: u64) -> Result<(), std::io::Error>;
}
impl IoSkip for File {
    fn skip(&mut self, bytes: u64) -> Result<(), std::io::Error> {
        let pos = self.stream_position()?;
        self.seek(SeekFrom::Start(pos + bytes))?;
        Ok(())
    }
}
impl IoSkip for BufReader<File> {
    fn skip(&mut self, bytes: u64) -> Result<(), std::io::Error> {
        let pos = self.stream_position()?;
        self.seek(SeekFrom::Start(pos + bytes))?;
        Ok(())
    }
}
impl IoSkip for StdinLock<'_> {
    fn skip(&mut self, bytes: u64) -> Result<(), std::io::Error> {
        let mut consumed = 0;
        while consumed < bytes {
            let buffered = self.fill_buf()?;
            let buffered = buffered.len();
            let consume = (bytes - consumed).min(buffered as u64);
            self.consume(consume as _);
            consumed += consume;
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn ansi_to_wide(ansi: &[u8]) -> Result<Vec<u16>, std::io::Error> {
    use winapi::um::{stringapiset::MultiByteToWideChar, winnls::CP_ACP};

    // Get the required buffer size.
    let required_size = unsafe { MultiByteToWideChar(CP_ACP, 0, ansi.as_ptr() as *const i8, ansi.len() as i32, core::ptr::null_mut(), 0) };
    if required_size == 0 {
        return Err(std::io::Error::last_os_error());
    }

    // Convert the ANSI string to wide string.
    let mut wide = vec![0u16; required_size as usize];
    let ret = unsafe { MultiByteToWideChar(CP_ACP, 0, ansi.as_ptr() as *const i8, ansi.len() as i32, wide.as_mut_ptr(), required_size) };
    if ret == 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(wide)
}

#[cfg(feature = "binary")]
mod binary {
    pub struct PrintHelp(pub Option<&'static str>);
}
#[cfg(feature = "binary")]
pub use binary::*;
