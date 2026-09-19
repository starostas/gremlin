"""Build the local affine development ELF and emit an explicit binary config."""
from pathlib import Path
import hashlib
import subprocess
root = Path(__file__).resolve().parent.parent
out = root / "runs" / "binary-example-input"
out.mkdir(parents=True, exist_ok=True)
library = out / "affine.so"
subprocess.run(["cc", "-shared", "-fPIC", "-O2", "-o", str(library), str(root / "tests/fixtures/affine.c")], check=True)
config = (root / "examples/affine.toml").read_text().replace('kind = "fixture"', 'kind = "binary"').replace('directory = "runs/affine"', 'directory = "runs/binary-affine"')
config = config.replace('max_instructions = 12', 'max_instructions = 12\nenumeration_depth = 3\nenumeration_proposals = 128')
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
[refinement]
max_rounds = 16
differential_cases = 256
initial_inputs = [["0x0000000000000000"]]
'''
(out / "affine.toml").write_text(config)
(out / "wrong.gremlin").write_text("fn candidate(x:u64)->u64 { return 0x000000007f6e5d4bu64; }\n")
print(out / "affine.toml")
