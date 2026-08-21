use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct CaptureLog {
    inner: Arc<Mutex<File>>,
}

impl CaptureLog {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        if file.metadata()?.len() == 0 {
            writeln!(file, "# unix_ns\tdirection\tclient\tlen\thex")?;
        }
        Ok(Self { inner: Arc::new(Mutex::new(file)) })
    }

    pub fn write(&self, direction: &str, client: SocketAddr, data: &[u8]) {
        let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let hex = hex(data);
        if let Ok(mut f) = self.inner.lock() {
            let _ = writeln!(f, "{ns}\t{direction}\t{client}\t{}\t{hex}", data.len());
            let _ = f.flush();
        }
    }
}

pub fn hex(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for b in data { out.push_str(&format!("{b:02x}")); }
    out
}
