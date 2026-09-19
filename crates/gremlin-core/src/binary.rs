use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryContract {
    pub path: String,
    pub sha256: String,
    pub architecture: String,
    pub format: String,
    pub symbol: String,
    pub abi: String,
    pub environment: String,
    pub wall_timeout_ms: u64,
    pub cpu_seconds: u64,
    pub memory_mb: u64,
}
impl BinaryContract {
    pub fn validate(&self) -> Result<(), String> {
        if self.architecture != "x86_64"
            || self.format != "elf"
            || self.abi != "sysv64"
            || self.environment != "empty"
        {
            return Err("binary targets require architecture=x86_64, format=elf, abi=sysv64, environment=empty".into());
        }
        if self.path.is_empty()
            || self.symbol.is_empty()
            || self.symbol.contains('\0')
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("binary path, symbol, and 64-digit SHA-256 are required".into());
        }
        if self.wall_timeout_ms == 0
            || self.wall_timeout_ms > 60_000
            || self.cpu_seconds == 0
            || self.cpu_seconds > 60
            || !(32..=4096).contains(&self.memory_mb)
        {
            return Err(
                "binary limits: wall_timeout_ms 1..60000, cpu_seconds 1..60, memory_mb 32..4096"
                    .into(),
            );
        }
        Ok(())
    }
}
