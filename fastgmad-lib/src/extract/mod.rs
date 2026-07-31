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
    sync::{atomic::{AtomicBool, Ordering}, Condvar, Mutex},
};

mod conf;
pub use conf::ExtractGmaConfig;
#[cfg(feature = "binary")]
pub use conf::ExtractGmadIn;

fn read_u32_le(r: &mut impl BufRead) -> std::io::Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i64_le(r: &mut impl BufRead) -> std::io::Result<i64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

pub fn extract_gma(conf: &ExtractGmaConfig, r: &mut (impl BufRead + ReadSkip)) -> Result<(), FastGmadError> {
    extract_gma_internal(conf, r, &mut || ())
}

#[cfg(feature = "binary")]
pub fn extract_gma_with_done_callback(
    conf: &ExtractGmaConfig,
    r: &mut (impl BufRead + ReadSkip),
    done_callback: &mut dyn FnMut(),
) -> Result<(), FastGmadError> {
    extract_gma_internal(conf, r, done_callback)
}

fn extract_gma_internal(
    conf: &ExtractGmaConfig,
    r: &mut (impl BufRead + ReadSkip),
    done_callback: &mut dyn FnMut(),
) -> Result<(), FastGmadError> {
    std::fs::create_dir_all(&conf.out).map_err(|e| FastGmadError::io(e, "creating output directory", Some(&conf.out)))?;

    log::debug!("Reading metadata...");
    let mut buf = Vec::new();

    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).map_err(|e| FastGmadError::io(e, "reading GMA magic bytes", None))?;
    if magic != GMA_MAGIC {
        return Err(FastGmadError::io(std::io::Error::new(std::io::ErrorKind::InvalidData, "File is not in GMA format"), "validating GMA magic", None));
    }

    let mut version_buf = [0u8; 1];
    r.read_exact(&mut version_buf).map_err(|e| FastGmadError::io(e, "reading version byte", None))?;
    let version = version_buf[0];
    
    if version != GMA_VERSION {
        log::warn!("File is in GMA version {version}, expected version {GMA_VERSION}, reading anyway...");
    }

    // SteamID & Timestamp (unused)
    r.skip(16).map_err(|e| FastGmadError::io(e, "reading SteamID and timestamp", None))?;

    if version > 1 {
        loop {
            let content = r.read_nul_str(&mut buf).map_err(|e| FastGmadError::io(e, "reading required content", None))?;
            if content.is_empty() { break; }
        }
    }

    let title = r.read_nul_str(&mut buf).map_err(|e| FastGmadError::io(e, "reading addon name", None))?.to_vec();
    let addon_json = r.read_nul_str(&mut buf).map_err(|e| FastGmadError::io(e, "reading addon description", None))?.to_vec();
    
    r.skip_nul_str().map_err(|e| FastGmadError::io(e, "reading addon author", None))?;
    r.skip(4).map_err(|e| FastGmadError::io(e, "reading addon version", None))?; // Addon version (unused)

    log::debug!("Writing addon.json...");
    let addon_json_path = conf.out.join("addon.json");
    {
        let mut addon_json_f = BufWriter::new(File::create(&addon_json_path).map_err(|e| FastGmadError::io(e, "creating addon.json file", Some(&addon_json_path)))?);
        
        let res = if let Ok(mut kv) = serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&addon_json) {
            kv.entry("title".to_string()).or_insert(serde_json::Value::String(String::from_utf8_lossy(&title).into_owned()));
            serde_json::to_writer_pretty(&mut addon_json_f, &kv)
        } else {
            serde_json::to_writer_pretty(&mut addon_json_f, &StubAddonJson {
                title: String::from_utf8_lossy(&title),
                description: String::from_utf8_lossy(&addon_json),
            })
        };
        res.map_err(|e| FastGmadError::io(std::io::Error::from(e), "writing addon.json", Some(&addon_json_path)))?;
        addon_json_f.flush().map_err(|e| FastGmadError::io(e, "flushing addon.json", Some(&addon_json_path)))?;
    }

    log::debug!("Reading file list...");
    let mut file_index = Vec::new();
    while read_u32_le(r).map_err(|e| FastGmadError::io(e, "reading entry index", None))? != 0 {
        let path = r.read_nul_str(&mut buf).map_err(|e| FastGmadError::io(e, "reading entry path", None))?.to_vec();
        let size = read_i64_le(r).map_err(|e| FastGmadError::io(e, "reading entry size", None))?;
        r.skip(4).map_err(|e| FastGmadError::io(e, "reading entry CRC", None))?; // _crc
        file_index.push(GmaEntry::new(&conf.out, path, size)?);
    }

    log::debug!("Extracting entries...");
    if conf.max_io_threads.get() == 1 {
        write_entries_standard(conf, r, &file_index)?;
    } else {
        write_entries_parallel(conf, r, &file_index)?;
    }

    done_callback();
    Ok(())
}

fn write_entries_standard(conf: &ExtractGmaConfig, r: &mut (impl BufRead + ReadSkip), file_index: &[GmaEntry]) -> Result<(), FastGmadError> {
    for GmaEntry { path, size } in file_index.iter() {
        let path = match path {
            Some(p) => p.as_path(),
            None => {
                r.skip(*size as u64).map_err(|e| FastGmadError::io(e, "skipping past GMA entry data", None))?;
                continue;
            }
        };

        let mut w = (|| {
            if let Some(parent) = path.parent() {
                if parent != conf.out {
                    std::fs::create_dir_all(parent).map_err(|e| FastGmadError::io(e, "creating directory for GMA entry", Some(parent)))?;
                }
            }
            File::create(path).map(BufWriter::new).map_err(|e| FastGmadError::io(e, "creating file for GMA entry", Some(path)))
        })()?;

        let mut take = (&mut *r).take(*size as u64);
        std::io::copy(&mut take, &mut w).map_err(|e| FastGmadError::io(e, "copying GMA entry data", Some(path)))?;
        w.flush().map_err(|e| FastGmadError::io(e, "flushing GMA entry file", Some(path)))?;
    }
    Ok(())
}

fn write_entries_parallel(conf: &ExtractGmaConfig, r: &mut (impl BufRead + ReadSkip), file_index: &[GmaEntry]) -> Result<(), FastGmadError> {
    let queue: Mutex<VecDeque<(PathBuf, Vec<u8>, usize)>> = Mutex::new(VecDeque::new());
    let queue_condvar = Condvar::new();
    let error = Mutex::new(None);
    let memory_used = Mutex::new(0usize);
    let memory_condvar = Condvar::new();
    let producer_done = AtomicBool::new(false);

    std::thread::scope(|s| {
        for _ in 0..conf.max_io_threads.get() {
            let queue = &queue;
            let queue_condvar = &queue_condvar;
            let error = &error;
            let memory_used = &memory_used;
            let memory_condvar = &memory_condvar;
            let producer_done = &producer_done;
            
            s.spawn(move || {
                loop {
                    let item = {
                        let mut q = queue.lock().unwrap();
                        loop {
                            if error.lock().unwrap().is_some() {
                                return;
                            }
                            if let Some(item) = q.pop_front() {
                                break Some(item);
                            }
                            if producer_done.load(Ordering::Relaxed) {
                                return;
                            }
                            q = queue_condvar.wait(q).unwrap();
                        }
                    };
                    
                    let Some((path, buf, size)) = item else { break; };
                    
                    let res = (|| {
                        if let Some(parent) = path.parent() {
                            if parent != conf.out {
                                std::fs::create_dir_all(parent).map_err(|e| FastGmadError::io(e, "creating directory for GMA entry", Some(parent)))?;
                            }
                        }
                        std::fs::write(&path, &buf).map_err(|e| FastGmadError::io(e, "writing GMA entry file", Some(&path)))
                    })();

                    {
                        let mut mem = memory_used.lock().unwrap();
                        *mem -= size;
                        memory_condvar.notify_one();
                    }

                    if let Err(e) = res {
                        *error.lock().unwrap() = Some(e);
                        queue_condvar.notify_all();
                        memory_condvar.notify_one();
                        break;
                    }
                }
            });
        }

        for GmaEntry { path, size } in file_index.iter() {
            if error.lock().unwrap().is_some() { break; }

            let path = match path {
                Some(p) => p.clone(),
                None => {
                    r.skip(*size as u64).map_err(|e| FastGmadError::io(e, "skipping past GMA entry data", None))?;
                    continue;
                }
            };

            if *size > conf.max_io_memory_usage.get() {
                let mut w = (|| {
                    if let Some(parent) = path.parent() {
                        if parent != conf.out {
                            std::fs::create_dir_all(parent).map_err(|e| FastGmadError::io(e, "creating directory for GMA entry", Some(parent)))?;
                        }
                    }
                    File::create(&path).map(BufWriter::new).map_err(|e| FastGmadError::io(e, "creating file for GMA entry", Some(&path)))
                })()?;
                let mut take = (&mut *r).take(*size as u64);
                std::io::copy(&mut take, &mut w).map_err(|e| FastGmadError::io(e, "copying GMA entry data", Some(&path)))?;
                w.flush().map_err(|e| FastGmadError::io(e, "flushing GMA entry file", Some(&path)))?;
                continue;
            }

            if *size == 0 {
                let mut q = queue.lock().unwrap();
                q.push_back((path, Vec::new(), 0));
                queue_condvar.notify_one();
                continue;
            }

            {
                let mut mem = memory_used.lock().unwrap();
                while *mem + *size > conf.max_io_memory_usage.get() {
                    if error.lock().unwrap().is_some() { break; }
                    mem = memory_condvar.wait(mem).unwrap();
                }
                if error.lock().unwrap().is_some() { break; }
                *mem += *size;
            }

            let mut buf = Vec::with_capacity(*size);
            let mut take = (&mut *r).take(*size as u64);
            take.read_to_end(&mut buf).map_err(|e| FastGmadError::io(e, "reading GMA entry data", Some(&path)))?;

            {
                let mut q = queue.lock().unwrap();
                q.push_back((path, buf, *size));
                queue_condvar.notify_one();
            }
        }

        producer_done.store(true, Ordering::Relaxed);
        queue_condvar.notify_all();

        Ok::<_, FastGmadError>(())
    })?;

    if let Some(e) = error.lock().unwrap().take() {
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
    fn new(base_path: &Path, path: Vec<u8>, size: i64) -> Result<Self, FastGmadError> {
        let size = usize::try_from(size).map_err(|_| {
            let err = std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unsupported file size ({size} bytes)"));
            FastGmadError::io(err, "reading GMA entry size", None)
        })?;

        let path_str = match String::from_utf8(path) {
            Ok(s) => s,
            Err(_) => return Ok(Self { path: None, size }),
        };

        let path = PathBuf::from(path_str);
        if path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))) || path.as_os_str().is_empty() {
            return Ok(Self { path: None, size });
        }

        Ok(Self { path: Some(base_path.join(path)), size })
    }
}