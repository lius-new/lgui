use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PersistentCacheKey {
    pub namespace: String,
    pub key: String,
    pub version: u64,
}

impl PersistentCacheKey {
    pub fn new(namespace: impl Into<String>, key: impl Into<String>, version: u64) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
            version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentEntry {
    pub key: PersistentCacheKey,
    pub bytes: Vec<u8>,
    pub mime: Option<String>,
    pub created_unix_seconds: u64,
    pub accessed_unix_seconds: u64,
    pub expires_unix_seconds: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub sensitive: bool,
}

impl PersistentEntry {
    pub fn new(key: PersistentCacheKey, bytes: Vec<u8>) -> Self {
        let now = unix_seconds();
        Self {
            key,
            bytes,
            mime: None,
            created_unix_seconds: now,
            accessed_unix_seconds: now,
            expires_unix_seconds: None,
            etag: None,
            last_modified: None,
            sensitive: false,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_unix_seconds
            .is_some_and(|expires| expires <= unix_seconds())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PersistentCacheStats {
    pub entry_count: usize,
    pub bytes: u64,
    pub evictions: u64,
}

#[derive(Debug)]
pub enum CacheStoreError {
    Io(io::Error),
    InvalidMetadata(String),
    SensitiveEntry,
}

impl fmt::Display for CacheStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "persistent cache I/O failed: {error}"),
            Self::InvalidMetadata(message) => {
                write!(formatter, "persistent cache metadata is invalid: {message}")
            }
            Self::SensitiveEntry => formatter.write_str("sensitive entries cannot be persisted"),
        }
    }
}

impl std::error::Error for CacheStoreError {}

impl From<io::Error> for CacheStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub trait PersistentCacheStore: Send + Sync + 'static {
    fn get(&self, key: &PersistentCacheKey) -> Result<Option<PersistentEntry>, CacheStoreError>;
    fn get_stale(
        &self,
        key: &PersistentCacheKey,
    ) -> Result<Option<PersistentEntry>, CacheStoreError> {
        self.get(key)
    }
    fn put(&self, entry: PersistentEntry) -> Result<(), CacheStoreError>;
    fn remove(&self, key: &PersistentCacheKey) -> Result<(), CacheStoreError>;
    fn trim_to(&self, budget_bytes: u64) -> Result<PersistentCacheStats, CacheStoreError>;
    fn clear(&self, namespace: Option<&str>) -> Result<(), CacheStoreError>;
    fn stats(&self) -> Result<PersistentCacheStats, CacheStoreError>;
}

#[derive(Clone, Debug)]
pub struct FileCacheStore {
    root: PathBuf,
}

impl FileCacheStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn paths(&self, key: &PersistentCacheKey) -> CachePaths {
        let namespace = normalized_namespace(&key.namespace);
        let mut digest = Sha256::new();
        digest.update(key.namespace.as_bytes());
        digest.update([0]);
        digest.update(key.key.as_bytes());
        digest.update([0]);
        digest.update(key.version.to_le_bytes());
        let name = hex_bytes(&digest.finalize());
        let directory = self.root.join(namespace);
        CachePaths {
            data: directory.join(format!("{name}.data")),
            metadata: directory.join(format!("{name}.meta")),
        }
    }

    fn records(&self) -> Result<Vec<CacheRecord>, CacheStoreError> {
        let mut records = Vec::new();
        let namespaces = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(records),
            Err(error) => return Err(error.into()),
        };
        for namespace in namespaces {
            let namespace = namespace?;
            if !namespace.file_type()?.is_dir() {
                continue;
            }
            for entry in fs::read_dir(namespace.path())? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("meta") {
                    continue;
                }
                let metadata = match read_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        remove_pair_for_metadata(&path);
                        continue;
                    }
                };
                let data = path.with_extension("data");
                records.push(CacheRecord {
                    metadata_path: path,
                    data_path: data,
                    length: metadata.length,
                    accessed: metadata.accessed,
                    expires: metadata.expires,
                });
            }
        }
        Ok(records)
    }

    fn read_entry(
        &self,
        key: &PersistentCacheKey,
        allow_expired: bool,
    ) -> Result<Option<PersistentEntry>, CacheStoreError> {
        let paths = self.paths(key);
        let mut metadata = match read_metadata(&paths.metadata) {
            Ok(metadata) => metadata,
            Err(CacheStoreError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(None)
            }
            Err(_) => {
                remove_paths(&paths);
                return Ok(None);
            }
        };
        if metadata.key != *key
            || (!allow_expired
                && metadata
                    .expires
                    .is_some_and(|expires| expires <= unix_seconds()))
        {
            if metadata.key != *key || !allow_expired {
                remove_paths(&paths);
            }
            return Ok(None);
        }
        let bytes = match fs::read(&paths.data) {
            Ok(bytes) => bytes,
            Err(_) => {
                remove_paths(&paths);
                return Ok(None);
            }
        };
        if bytes.len() as u64 != metadata.length || sha256(&bytes) != metadata.checksum {
            remove_paths(&paths);
            return Ok(None);
        }
        metadata.accessed = unix_seconds();
        atomic_write(&paths.metadata, metadata.encode().as_bytes())?;
        Ok(Some(PersistentEntry {
            key: metadata.key,
            bytes,
            mime: metadata.mime,
            created_unix_seconds: metadata.created,
            accessed_unix_seconds: metadata.accessed,
            expires_unix_seconds: metadata.expires,
            etag: metadata.etag,
            last_modified: metadata.last_modified,
            sensitive: false,
        }))
    }
}

impl PersistentCacheStore for FileCacheStore {
    fn get(&self, key: &PersistentCacheKey) -> Result<Option<PersistentEntry>, CacheStoreError> {
        self.read_entry(key, false)
    }

    fn get_stale(
        &self,
        key: &PersistentCacheKey,
    ) -> Result<Option<PersistentEntry>, CacheStoreError> {
        self.read_entry(key, true)
    }

    fn put(&self, mut entry: PersistentEntry) -> Result<(), CacheStoreError> {
        if entry.sensitive {
            return Err(CacheStoreError::SensitiveEntry);
        }
        let paths = self.paths(&entry.key);
        if entry.created_unix_seconds == 0 {
            entry.created_unix_seconds = unix_seconds();
        }
        entry.accessed_unix_seconds = unix_seconds();
        let metadata = CacheMetadata {
            key: entry.key,
            length: entry.bytes.len() as u64,
            checksum: sha256(&entry.bytes),
            created: entry.created_unix_seconds,
            accessed: entry.accessed_unix_seconds,
            expires: entry.expires_unix_seconds,
            mime: entry.mime,
            etag: entry.etag,
            last_modified: entry.last_modified,
        };
        atomic_write(&paths.data, &entry.bytes)?;
        atomic_write(&paths.metadata, metadata.encode().as_bytes())?;
        Ok(())
    }

    fn remove(&self, key: &PersistentCacheKey) -> Result<(), CacheStoreError> {
        remove_paths(&self.paths(key));
        Ok(())
    }

    fn trim_to(&self, budget_bytes: u64) -> Result<PersistentCacheStats, CacheStoreError> {
        let now = unix_seconds();
        let mut records = self.records()?;
        let mut evictions = 0_u64;
        records.retain(|record| {
            if record.expires.is_some_and(|expires| expires <= now) {
                remove_record(record);
                evictions = evictions.saturating_add(1);
                false
            } else {
                true
            }
        });
        records.sort_by_key(|record| record.accessed);
        let mut bytes = records
            .iter()
            .fold(0_u64, |total, record| total.saturating_add(record.length));
        let mut entries = records.len();
        for record in records {
            if bytes <= budget_bytes {
                break;
            }
            remove_record(&record);
            bytes = bytes.saturating_sub(record.length);
            entries = entries.saturating_sub(1);
            evictions = evictions.saturating_add(1);
        }
        Ok(PersistentCacheStats {
            entry_count: entries,
            bytes,
            evictions,
        })
    }

    fn clear(&self, namespace: Option<&str>) -> Result<(), CacheStoreError> {
        let target = namespace
            .map(|namespace| self.root.join(normalized_namespace(namespace)))
            .unwrap_or_else(|| self.root.clone());
        match fs::remove_dir_all(target) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn stats(&self) -> Result<PersistentCacheStats, CacheStoreError> {
        let records = self.records()?;
        Ok(PersistentCacheStats {
            entry_count: records.len(),
            bytes: records
                .iter()
                .fold(0_u64, |total, record| total.saturating_add(record.length)),
            evictions: 0,
        })
    }
}

#[derive(Clone, Debug)]
struct CacheMetadata {
    key: PersistentCacheKey,
    length: u64,
    checksum: String,
    created: u64,
    accessed: u64,
    expires: Option<u64>,
    mime: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl CacheMetadata {
    fn encode(&self) -> String {
        [
            "lgui-cache-v1".to_owned(),
            hex_text(&self.key.namespace),
            hex_text(&self.key.key),
            self.key.version.to_string(),
            self.length.to_string(),
            self.checksum.clone(),
            self.created.to_string(),
            self.accessed.to_string(),
            self.expires
                .map_or_else(|| "-".to_owned(), |value| value.to_string()),
            encode_optional(&self.mime),
            encode_optional(&self.etag),
            encode_optional(&self.last_modified),
        ]
        .join("\n")
    }

    fn decode(text: &str) -> Result<Self, CacheStoreError> {
        let lines = text.lines().collect::<Vec<_>>();
        if lines.len() != 12 || lines[0] != "lgui-cache-v1" {
            return Err(CacheStoreError::InvalidMetadata(
                "unsupported metadata format".to_owned(),
            ));
        }
        Ok(Self {
            key: PersistentCacheKey {
                namespace: decode_text(lines[1])?,
                key: decode_text(lines[2])?,
                version: parse_number(lines[3], "key version")?,
            },
            length: parse_number(lines[4], "content length")?,
            checksum: lines[5].to_owned(),
            created: parse_number(lines[6], "created time")?,
            accessed: parse_number(lines[7], "accessed time")?,
            expires: if lines[8] == "-" {
                None
            } else {
                Some(parse_number(lines[8], "expiry time")?)
            },
            mime: decode_optional(lines[9])?,
            etag: decode_optional(lines[10])?,
            last_modified: decode_optional(lines[11])?,
        })
    }
}

struct CachePaths {
    data: PathBuf,
    metadata: PathBuf,
}

struct CacheRecord {
    metadata_path: PathBuf,
    data_path: PathBuf,
    length: u64,
    accessed: u64,
    expires: Option<u64>,
}

fn read_metadata(path: &Path) -> Result<CacheMetadata, CacheStoreError> {
    CacheMetadata::decode(&fs::read_to_string(path)?)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CacheStoreError> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
    let parent = path
        .parent()
        .ok_or_else(|| CacheStoreError::InvalidMetadata("cache path has no parent".to_owned()))?;
    fs::create_dir_all(parent)?;
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let mut file = fs::File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if let Err(error) = fs::rename(&temp, path) {
        if path.exists() {
            fs::remove_file(path)?;
            fs::rename(&temp, path)?;
        } else {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
    }
    Ok(())
}

fn remove_paths(paths: &CachePaths) {
    let _ = fs::remove_file(&paths.metadata);
    let _ = fs::remove_file(&paths.data);
}

fn remove_record(record: &CacheRecord) {
    let _ = fs::remove_file(&record.metadata_path);
    let _ = fs::remove_file(&record.data_path);
}

fn remove_pair_for_metadata(metadata: &Path) {
    let _ = fs::remove_file(metadata);
    let _ = fs::remove_file(metadata.with_extension("data"));
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn normalized_namespace(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(48)
        .collect::<String>();
    if normalized.is_empty() {
        "default".to_owned()
    } else {
        normalized
    }
}

fn sha256(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_text(value: &str) -> String {
    hex_bytes(value.as_bytes())
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_text(value: &str) -> Result<String, CacheStoreError> {
    let bytes = decode_hex(value)?;
    String::from_utf8(bytes)
        .map_err(|_| CacheStoreError::InvalidMetadata("text is not UTF-8".to_owned()))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, CacheStoreError> {
    if value.len() % 2 != 0 {
        return Err(CacheStoreError::InvalidMetadata(
            "hex value has an odd length".to_owned(),
        ));
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        output.push((high << 4) | low);
    }
    Ok(output)
}

fn hex_digit(value: u8) -> Result<u8, CacheStoreError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(CacheStoreError::InvalidMetadata(
            "invalid hex digit".to_owned(),
        )),
    }
}

fn encode_optional(value: &Option<String>) -> String {
    value.as_deref().map_or_else(|| "-".to_owned(), hex_text)
}

fn decode_optional(value: &str) -> Result<Option<String>, CacheStoreError> {
    if value == "-" {
        Ok(None)
    } else {
        decode_text(value).map(Some)
    }
}

fn parse_number<T>(value: &str, label: &str) -> Result<T, CacheStoreError>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| CacheStoreError::InvalidMetadata(format!("invalid {label}")))
}
