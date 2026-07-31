use crate::{
    error::FastGmadError,
    util::{BufReadEx, ReadSkip},
    GMA_MAGIC, GMA_VERSION,
};
use std::{
    borrow::Cow,
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufWriter, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Condvar, Mutex},
};

mod conf;
pub use conf::ExtractGmaConfig;
#[cfg(feature = "binary")]
pub use conf::{ExtractGmadIn, PrintHelp};

fn read_u32_le(r: &mut impl BufRead) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64_le(r: &mut impl BufRead) -> std::io::Result<u64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

pub fn extract_gma(conf: &ExtractGmaConfig, r: &mut impl BufRead) -> Result<(), FastGmadError> {
    std::fs::create_dir_all(&conf.out)
        .map_err(|e| FastGmadError::io(e, "creating output directory", Some(&conf.out)))?;

    log::debug!("Reading metadata...");
    let mut buf = Vec::new();

    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)
        .map_err(|e| FastGmadError::io(e, "reading GMA magic bytes", None))?;
    if magic != GMA_MAGIC {
        return Err(FastGmadError::io(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "File is not in GMA format"),
            "validating GMA magic",
            None,
        ));
    }

    let mut version_buf = [0u8; 1];
    r.read_exact(&mut version_buf)
        .map_err(|e| FastGmadError::io(e, "reading version byte", None))?;
    let version = version_buf[0];

    if version != GMA_VERSION {
        log::warn!("File is in GMA version {version}, expected version {GMA_VERSION}, reading anyway...");
    }

    r.skip(16)
        .map_err(|e| FastGmadError::io(e, "reading SteamID and timestamp", None))?;

    if version > 1 {
        loop {
            let content = r
                .read_nul_str(&mut buf)
                .map_err(|e| FastGmadError::io(e, "reading required content", None))?;
            if content.is_empty() {
                break;
            }
        }
    }

    let title = r
        .read_nul_str(&mut buf)
        .map_err(|e| FastGmadError::io(e, "reading addon name", None))?
        .to_vec();
    let addon_json = r
        .read_nul_str(&mut buf)
        .map_err(|e| FastGmadError::io(e, "reading addon description", None))?
        .to_vec();

    r.skip_nul_str()
        .map_err(|e| FastGmadError::io(e, "reading addon author", None))?;
    r.skip(4)
        .map_err(|e| FastGmadError::io(e, "reading addon version", None))?;

    log::debug!("Writing addon.json...");
    let addon_json_path = conf.out.join("addon.json");
    {
        let mut f = BufWriter::new(
            File::create(&addon_json_path)
                .map_err(|e| FastGmadError::io(e, "creating addon.json file", Some(&addon_json_path)))?,
        );

        let res = if let Ok(mut kv) =
            serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&addon_json)
        {
            kv.entry("title".to_string())
                .or_insert_with(|| serde_json::Value::String(String::from_utf8_lossy(&title).into_owned()));
            serde_json::to_writer_pretty(&mut f, &kv)
        } else {
            serde_json::to_writer_pretty(
                &mut f,
                &StubAddonJson {
                    title: String::from_utf8_lossy(&title),
                    description: String::from_utf8_lossy(&addon_json),
                },
            )
        };
        res.map_err(|e| FastGmadError::io(std::io::Error::from(e), "writing addon.json", Some(&addon_json_path)))?;
        f.flush()
            .map_err(|e| FastGmadError::io(e, "flushing addon.json", Some(&addon_json_path)))?;
    }

    log::debug!("Reading file list...");
    let mut file_index = Vec::new();
    while read_u32_le(r).map_err(|e| FastGmadError::io(e, "reading entry index", None))? != 0 {
        let path = r
            .read_nul_str(&mut buf)
            .map_err(|e| FastGmadError::io(e, "reading entry path", None))?
            .to_vec();
        let size = read_u64_le(r).map_err(|e| FastGmadError::io(e, "reading entry size", None))?;
        r.skip(4)
            .map_err(|e| FastGmadError::io(e, "reading entry CRC", None))?;
        file_index.push(GmaEntry::new(&conf.out, path, size)?);
    }

    log::debug!("Extracting entries...");
    if conf.max_io_threads.get() == 1 {
        write_entries_sequential(r, &file_index)?;
    } else {
        write_entries_parallel(conf, r, &file_index)?;
    }

    Ok(())
}

fn create_parent_dirs(path: &Path) -> Result<(), FastGmadError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| FastGmadError::io(e, "creating directory for GMA entry", Some(parent)))?;
    }
    Ok(())
}

fn write_entry_streaming(r: &mut impl BufRead, path: &Path, size: usize) -> Result<(), FastGmadError> {
    create_parent_dirs(path)?;

    let mut w = BufWriter::new(
        File::create(path).map_err(|e| FastGmadError::io(e, "creating file for GMA entry", Some(path)))?,
    );

    let copied = std::io::copy(&mut (&mut *r).take(size as u64), &mut w)
        .map_err(|e| FastGmadError::io(e, "copying GMA entry data", Some(path)))?;
    if copied != size as u64 {
        return Err(FastGmadError::io(
            std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "GMA entry data truncated"),
            "copying GMA entry data",
            Some(path),
        ));
    }

    w.flush()
        .map_err(|e| FastGmadError::io(e, "flushing GMA entry file", Some(path)))?;
    Ok(())
}

fn write_entries_sequential(r: &mut impl BufRead, file_index: &[GmaEntry]) -> Result<(), FastGmadError> {
    for GmaEntry { path, size } in file_index {
        match path {
            Some(p) => write_entry_streaming(r, p, *size)?,
            None => {
                r.skip(*size as u64)
                    .map_err(|e| FastGmadError::io(e, "skipping past GMA entry data", None))?;
            }
        }
    }
    Ok(())
}

fn write_entries_parallel(
    conf: &ExtractGmaConfig,
    r: &mut impl BufRead,
    file_index: &[GmaEntry],
) -> Result<(), FastGmadError> {
    struct State {
        queue: VecDeque<(PathBuf, Vec<u8>)>,
        mem_used: usize,
        error: Option<FastGmadError>,
        producer_done: bool,
    }

    let state = Mutex::new(State {
        queue: VecDeque::new(),
        mem_used: 0,
        error: None,
        producer_done: false,
    });
    let queue_cv = Condvar::new();
    let mem_cv = Condvar::new();

    struct ProducerGuard<'a> {
        state: &'a Mutex<State>,
        queue_cv: &'a Condvar,
        mem_cv: &'a Condvar,
    }
    impl Drop for ProducerGuard<'_> {
        fn drop(&mut self) {
            let mut s = self.state.lock().unwrap();
            s.producer_done = true;
            drop(s);
            self.queue_cv.notify_all();
            self.mem_cv.notify_all();
        }
    }

    std::thread::scope(|s| {
        for _ in 0..conf.max_io_threads.get() {
            s.spawn(|| {
                loop {
                    let (path, buf) = {
                        let mut s = state.lock().unwrap();
                        loop {
                            if s.error.is_some() || (s.producer_done && s.queue.is_empty()) {
                                return;
                            }
                            if let Some(item) = s.queue.pop_front() {
                                break item;
                            }
                            s = queue_cv.wait(s).unwrap();
                        }
                    };

                    let res = create_parent_dirs(&path).and_then(|()| {
                        std::fs::write(&path, &buf)
                            .map_err(|e| FastGmadError::io(e, "writing GMA entry file", Some(&path)))
                    });

                    let is_err = res.is_err();

                    {
                        let mut s = state.lock().unwrap();
                        s.mem_used -= buf.len();
                        if let Err(e) = res {
                            s.error.get_or_insert(e);
                        }
                        drop(s);
                        mem_cv.notify_one();
                        if is_err {
                            queue_cv.notify_all();
                        }
                    }

                    if is_err {
                        return;
                    }
                }
            });
        }

        let _guard = ProducerGuard {
            state: &state,
            queue_cv: &queue_cv,
            mem_cv: &mem_cv,
        };

        for GmaEntry { path, size } in file_index {
            if state.lock().unwrap().error.is_some() {
                break;
            }

            let path = match path {
                Some(p) => p.clone(),
                None => {
                    r.skip(*size as u64)
                        .map_err(|e| FastGmadError::io(e, "skipping past GMA entry data", None))?;
                    continue;
                }
            };

            if *size > conf.max_io_memory_usage.get() {
                write_entry_streaming(r, &path, *size)?;
                continue;
            }

            if *size == 0 {
                let mut s = state.lock().unwrap();
                s.queue.push_back((path, Vec::new()));
                drop(s);
                queue_cv.notify_one();
                continue;
            }

            {
                let mut s = state.lock().unwrap();
                while s.mem_used.saturating_add(*size) > conf.max_io_memory_usage.get() {
                    if s.error.is_some() {
                        break;
                    }
                    s = mem_cv.wait(s).unwrap();
                }
                if s.error.is_some() {
                    break;
                }
                s.mem_used += *size;
            }

            let mut buf = Vec::with_capacity(*size);
            let mut take = (&mut *r).take(*size as u64);
            take.read_to_end(&mut buf)
                .map_err(|e| FastGmadError::io(e, "reading GMA entry data", Some(&path)))?;
            if buf.len() != *size {
                return Err(FastGmadError::io(
                    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "GMA entry data truncated"),
                    "reading GMA entry data",
                    Some(&path),
                ));
            }

            {
                let mut s = state.lock().unwrap();
                s.queue.push_back((path, buf));
                drop(s);
                queue_cv.notify_one();
            }
        }

        Ok::<_, FastGmadError>(())
    })?;

    if let Some(e) = state.lock().unwrap().error.take() {
        return Err(e);
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct StubAddonJson<'a> {
    title: Cow<'a, str>,
    description: Cow<'a, str>,
}

struct GmaEntry {
    path: Option<PathBuf>,
    size: usize,
}

impl GmaEntry {
    fn new(base_path: &Path, path: Vec<u8>, size: u64) -> Result<Self, FastGmadError> {
        let size = usize::try_from(size).map_err(|_| {
            let err = std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unsupported file size ({size} bytes)"),
            );
            FastGmadError::io(err, "reading GMA entry size", None)
        })?;

        let path_str = match String::from_utf8(path) {
            Ok(s) => s,
            Err(_) => return Ok(Self { path: None, size }),
        };

        let path = PathBuf::from(path_str);
        if path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
            || path.as_os_str().is_empty()
        {
            return Ok(Self { path: None, size });
        }

        Ok(Self {
            path: Some(base_path.join(path)),
            size,
        })
    }
}