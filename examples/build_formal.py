"""Build a constructor-free affine ELF supported by the narrow formal model."""
from pathlib import Path
import hashlib
import subprocess
root = Path(__file__).resolve().parent.parent
out = root / "runs" / "formal-example"
out.mkdir(parents=True, exist_ok=True)
library = out / "affine.so"
subprocess.run(["cc", "-shared", "-nostdlib", "-fPIC", "-O2", "-fcf-protection=none", "-o", str(library), str(root / "tests/fixtures/affine.c")], check=True)
config = (root / "examples/affine.toml").read_text().replace('kind = "fixture"', 'kind = "binary"').replace('directory = "runs/affine"', 'directory = "runs/formal-affine-search"')
config += f'''
[target.binary]
path = "{library}"
sha256 = "{hashlib.sha256(library.read_bytes()).hexdigest()}"
architecture = "x86_64"
format = "elf"
symbol = "affine_u64"
abi = "sysv64"
environment = "empty"
wall_timeout_ms = 3000
cpu_seconds = 2
memory_mb = 256
'''
(out / "affine.toml").write_text(config)
(out / "candidate.gremlin").write_text((root / "examples/affine.gremlin").read_text())
(out / "wrong.gremlin").write_text("fn candidate(x:u64)->u64 { return x; }\n")
print(out / "affine.toml")
