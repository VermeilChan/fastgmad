use std::{num::NonZeroUsize, path::PathBuf};
use crate::util::PrintHelp;

#[derive(Debug)]
pub struct ExtractGmaConfig {
    pub out: PathBuf,
    pub max_io_threads: NonZeroUsize,
    pub max_io_memory_usage: NonZeroUsize,
    #[cfg(feature = "binary")]
    pub noprogress: bool,
}

#[cfg(feature = "binary")]
pub enum ExtractGmadIn {
    Stdin,
    File(PathBuf),
}

const DEFAULT_THREADS: NonZeroUsize = NonZeroUsize::new(1).expect("1 is non-zero");
const DEFAULT_MEMORY: NonZeroUsize = NonZeroUsize::new(1 << 31).expect("2GB is non-zero"); // 2 GiB

impl Default for ExtractGmaConfig {
    fn default() -> Self {
        Self {
            out: PathBuf::new(),
            max_io_threads: std::thread::available_parallelism().unwrap_or(DEFAULT_THREADS),
            max_io_memory_usage: DEFAULT_MEMORY,
            #[cfg(feature = "binary")]
            noprogress: false,
        }
    }
}

#[cfg(feature = "binary")]
impl ExtractGmaConfig {
    pub fn from_args() -> Result<(Self, ExtractGmadIn), PrintHelp> {
        let mut config = Self::default();
        let mut r#in = None;
        let mut args = std::env::args_os().skip(2);
        
        while let Some(arg) = args.next() {
            match arg.to_str().ok_or(PrintHelp(Some("Unknown GMAD extraction argument")))? {
                "-max-io-threads" => {
                    config.max_io_threads = args.next()
                        .and_then(|v| v.to_str().and_then(|s| s.parse().ok()))
                        .ok_or(PrintHelp(Some("Expected integer greater than zero for -max-io-threads")))?;
                }
                "-max-io-memory-usage" => {
                    config.max_io_memory_usage = args.next()
                        .and_then(|v| v.to_str().and_then(|s| s.parse().ok()))
                        .ok_or(PrintHelp(Some("Expected integer greater than zero for -max-io-memory-usage")))?;
                }
                "-out" => {
                    config.out = PathBuf::from(args.next().filter(|out| !out.is_empty()).ok_or(PrintHelp(Some("Expected a value after -out")))?);
                }
                "-stdin" => r#in = Some(ExtractGmadIn::Stdin),
                "-file" => {
                    r#in = Some(ExtractGmadIn::File(args.next().filter(|i| !i.is_empty()).map(PathBuf::from).ok_or(PrintHelp(Some("Expected a value after -file")))?));
                }
                "-noprogress" => config.noprogress = true,
                _ => return Err(PrintHelp(Some("Unknown GMAD extraction argument"))),
            }
        }

        let r#in = r#in.ok_or(PrintHelp(Some("Please provide an input path")))?;

        if config.out.as_os_str().is_empty() {
            if let ExtractGmadIn::File(path) = &r#in {
                let mut dir = path.to_owned();
                dir.set_extension("");
                if dir.exists() && !dir.is_dir() {
                    return Err(PrintHelp(Some("Default output path exists as a file. Please specify an output folder with -out")));
                }
                config.out = dir;
            } else {
                return Err(PrintHelp(Some("Please provide an output path")));
            }
        }

        Ok((config, r#in))
    }
}