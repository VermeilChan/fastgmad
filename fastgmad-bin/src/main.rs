use fastgmad::error::FastGmadError;
use fastgmad::extract::{ExtractGmaConfig, ExtractGmadIn};
use std::{
    ffi::OsStr,
    fs::File,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    time::Instant,
};

fn main() {
    init_logger();

    match bin() {
        Ok(()) => {
            log::info!("Finished");
            std::process::exit(0);
        }
        Err(FastGmadBinError::FastGmadError(err)) => {
            eprintln!();
            log::error!("{err}\n");
            std::process::exit(1);
        }
        Err(FastGmadBinError::PrintHelp(msg)) => {
            if let Some(msg) = msg {
                log::error!("{msg}\n");
            }
            eprintln!("{}", include_str!("usage.txt"));
        }
    }
}

fn init_logger() {
    struct Logger(Instant);
    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }
        fn log(&self, record: &log::Record) {
            if record.level() == log::Level::Info {
                eprintln!("[+{:?}] {}", self.0.elapsed(), record.args());
            } else {
                eprintln!(
                    "{}{}",
                    record.level().as_str().chars().next().unwrap_or(' '),
                    record.args()
                );
            }
        }
        fn flush(&self) {
            let _ = std::io::stderr().lock().flush();
        }
    }

    log::set_logger(Box::leak(Box::new(Logger(Instant::now())))).unwrap();
    log::set_max_level(log::LevelFilter::Info);
}

fn bin() -> Result<(), FastGmadBinError> {
    let mut args = std::env::args_os().skip(1);
    let cmd = args.next().ok_or(FastGmadBinError::PrintHelp(None))?;
    let path = Path::new(&cmd);

    if path.is_file() && path.extension() == Some(OsStr::new("gma")) {
        let conf = ExtractGmaConfig {
            out: path.with_extension(""),
            ..Default::default()
        };
        extract(conf, ExtractGmadIn::File(PathBuf::from(cmd)))
    } else if cmd.to_str() == Some("extract") {
        let (conf, input) = ExtractGmaConfig::from_args(args).map_err(|h| FastGmadBinError::PrintHelp(h.0))?;
        extract(conf, input)
    } else {
        Err(FastGmadBinError::PrintHelp(None))
    }
}

fn extract(conf: ExtractGmaConfig, input: ExtractGmadIn) -> Result<(), FastGmadBinError> {
    match input {
        ExtractGmadIn::File(path) => {
            let file = File::open(&path).map_err(|e| FastGmadError::io(e, "opening input file", Some(&path)))?;
            let mut r = BufReader::with_capacity(1 << 20, file);
            fastgmad::extract::extract_gma(&conf, &mut r)?;
        }
        ExtractGmadIn::Stdin => {
            let mut r = std::io::stdin().lock();
            fastgmad::extract::extract_gma(&conf, &mut r)?;
        }
    }
    Ok(())
}

enum FastGmadBinError {
    FastGmadError(FastGmadError),
    PrintHelp(Option<&'static str>),
}

impl From<FastGmadError> for FastGmadBinError {
    fn from(e: FastGmadError) -> Self {
        Self::FastGmadError(e)
    }
}