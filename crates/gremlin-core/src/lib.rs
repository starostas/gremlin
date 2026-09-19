pub mod fixtures;
pub mod rng;
pub use fixtures::*;
pub use rng::*;
pub mod corpus;
pub mod ir;
pub mod syntax;
pub mod types;
pub use corpus::*;
pub use ir::*;
use sha2::{Digest, Sha256};
pub use syntax::*;
pub use types::*;
pub const SCHEMA_VERSION: u32 = 1;
pub const SEMANTICS_VERSION: &str = "gremlin-2";
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn object_hash<T: serde::Serialize>(value: &T) -> String {
    hash(&serde_json::to_vec(value).expect("serializable data"))
}
pub mod binary;
pub use binary::*;

pub mod module;
mod structured;
pub use module::*;
