#![allow(clippy::unnecessary_literal_unwrap)]

use fastgmad::extract::{ExtractGmaConfig, ExtractGmadIn};
use fastgmad::error::{FastGmadError, FastGmadErrorKind};
use std::{
    ffi::OsStr,
    fs::File,
    io::{BufReader, Write},
    path::{Path, PathBuf},
    time::Instant,
};

fn main() {
    log::set_logger({
        log::set_max_level(log::LevelFilter::Info);

        struct Logger(Instant);
        impl log::Log for Logger {
            fn enabled(&self, metadata: &log::Metadata) -> bool {
                metadata.level() <= log::Level::Info
            }

            fn log(&self, record: &log::Record) {
                let level = match record.level() {
                    log::Level::Info => {
                        eprintln!("[+{:?}] {}", self.0.elapsed(), record.args());
                        return;
                    }
                    log::Level::Warn => "WARN: ",
                    log::Level::Error => "ERROR: ",
                    log::Level::Debug => "DEBUG: ",
                    log::Level::Trace => "TRACE: ",
                };
                eprintln!("{level}{}", record.args());
            }

            fn flush(&self) {
                std::io::stderr().lock().flush().ok();
            }
        }
        Box::leak(Box::new(Logger(Instant::now())))
    })
    .unwrap();

    eprintln!(concat!(
        "fastgmad v",
        env!("CARGO_PKG_VERSION"),
        " by Billy\nhttps://github.com/WilliamVenner/fastgmad\n",
        "Prefer to use a GUI? Check out https://github.com/WilliamVenner/gmpublisher\n"
    ));

    match bin() {
        Ok(()) => {}
        Err(FastGmadBinError::FastGmadError(err)) => {
            eprintln!();
            log::error!("{err}\n");
            Err::<(), _>(err).unwrap();
        }
        Err(FastGmadBinError::PrintHelp(msg)) => {
            if let Some(msg) = msg {
                log::error!("{msg}\n");
            }

            eprintln!("{}", include_str!("usage.txt"));
        }
    }
}

fn bin() -> Result<(), FastGmadBinError> {
    let mut exit = || {
        log::info!("Finished");
        std::process::exit(0);
    };

    let mut args = std::env::args_os().skip(1);
    let cmd = args.next().ok_or(FastGmadBinError::PrintHelp(None))?;
    let path = Path::new(&cmd);

    if path.is_file() && path.extension() == Some(OsStr::new("gma")) {
        // The first argument is a path to a GMA
        // Extract it
        extract(
            ExtractGmaConfig {
                out: path.with_extension(""),
                ..Default::default()
            },
            ExtractGmadIn::File(PathBuf::from(cmd)),
            &mut exit,
        )
    } else {
        match cmd.to_str() {
            Some("extract") => {
                let (conf, r#in) = ExtractGmaConfig::from_args().map_err(|_| FastGmadBinError::PrintHelp(None))?;
                extract(conf, r#in, &mut exit)
            }
            _ => Err(FastGmadBinError::PrintHelp(None)),
        }
    }
}

fn extract(
    conf: ExtractGmaConfig,
    r#in: ExtractGmadIn,
    exit: &mut impl FnMut(),
) -> Result<(), FastGmadBinError> {
    match r#in {
        ExtractGmadIn::File(path) => {
            log::info!("Opening input file...");
            let mut r = BufReader::new(File::open(&path).map_err(|error| FastGmadError {
                kind: FastGmadErrorKind::PathIoError { path, error },
                context: Some("opening input file".to_string()),
            })?);
            fastgmad::extract::extract_gma_with_done_callback(&conf, &mut r, exit)?;
        }

        ExtractGmadIn::Stdin => {
            let mut r = std::io::stdin().lock();
            fastgmad::extract::extract_gma_with_done_callback(&conf, &mut r, exit)?;
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

impl From<PrintHelp> for FastGmadBinError {
    fn from(e: PrintHelp) -> Self {
        Self::PrintHelp(e.0)
    }
}

struct PrintHelp(Option<&'static str>);
