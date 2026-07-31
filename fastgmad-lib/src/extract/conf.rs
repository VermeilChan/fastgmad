use std::{ffi::OsString, num::NonZeroUsize, path::PathBuf};

#[derive(Debug)]
pub struct ExtractGmaConfig {
    pub out: PathBuf,
    pub max_io_threads: NonZeroUsize,
    pub max_io_memory_usage: NonZeroUsize,
}

#[cfg(feature = "binary")]
pub enum ExtractGmadIn {
    Stdin,
    File(PathBuf),
}

#[cfg(feature = "binary")]
pub struct PrintHelp(pub Option<&'static str>);

const DEFAULT_MEMORY: NonZeroUsize = NonZeroUsize::new(1 << 31).expect("2GB is non-zero");
const MIN_THREADS: usize = 1;
const MAX_THREADS: usize = 32;
const RESERVED_THREADS: usize = 2;

fn get_default_threads() -> NonZeroUsize {
    let available = std::thread::available_parallelism()
        .map(NonZeroUsize::get)
        .unwrap_or(MIN_THREADS);

    let threads = available
        .saturating_sub(RESERVED_THREADS)
        .clamp(MIN_THREADS, MAX_THREADS);

    NonZeroUsize::new(threads).expect("clamped value is guaranteed to be >= 1")
}

impl Default for ExtractGmaConfig {
    fn default() -> Self {
        Self {
            out: PathBuf::new(),
            max_io_threads: get_default_threads(),
            max_io_memory_usage: DEFAULT_MEMORY,
        }
    }
}

#[cfg(feature = "binary")]
impl ExtractGmaConfig {
    pub fn from_args(mut args: impl Iterator<Item = OsString>) -> Result<(Self, ExtractGmadIn), PrintHelp> {
        let mut config = Self::default();
        let mut input = None;

        while let Some(arg) = args.next() {
            let arg = arg.to_str().ok_or(PrintHelp(Some("Non-UTF-8 argument")))?;
            match arg {
                "-max-io-threads" => {
                    config.max_io_threads = args
                        .next()
                        .and_then(|v| v.to_str().and_then(|s| s.parse().ok()))
                        .ok_or(PrintHelp(Some("Expected integer greater than zero for -max-io-threads")))?;
                }
                "-max-io-memory-usage" => {
                    config.max_io_memory_usage = args
                        .next()
                        .and_then(|v| v.to_str().and_then(|s| s.parse().ok()))
                        .ok_or(PrintHelp(Some(
                            "Expected integer greater than zero for -max-io-memory-usage",
                        )))?;
                }
                "-out" => {
                    config.out = PathBuf::from(
                        args.next()
                            .filter(|p| !p.is_empty())
                            .ok_or(PrintHelp(Some("Expected a value after -out")))?,
                    );
                }
                "-stdin" => input = Some(ExtractGmadIn::Stdin),
                "-file" => {
                    input = Some(ExtractGmadIn::File(
                        args.next()
                            .filter(|p| !p.is_empty())
                            .map(PathBuf::from)
                            .ok_or(PrintHelp(Some("Expected a value after -file")))?,
                    ));
                }
                _ => return Err(PrintHelp(Some("Unknown GMAD extraction argument"))),
            }
        }

        let input = input.ok_or(PrintHelp(Some("Please provide an input path")))?;

        if config.out.as_os_str().is_empty() {
            if let ExtractGmadIn::File(path) = &input {
                let mut dir = path.to_owned();
                dir.set_extension("");
                if dir.exists() && !dir.is_dir() {
                    return Err(PrintHelp(Some(
                        "Default output path exists as a file. Please specify an output folder with -out",
                    )));
                }
                config.out = dir;
            } else {
                return Err(PrintHelp(Some("Please provide an output path")));
            }
        }

        Ok((config, input))
    }
}