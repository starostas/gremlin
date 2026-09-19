use crate::{Request, MAX_BATCH};
use gremlin_core::*;
use std::{arch::asm, ffi::CString, mem, ptr};
#[repr(C)]
struct Shared {
    status: u32,
    count: u32,
    values: [u64; MAX_BATCH],
}
fn os_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}
fn limit(resource: libc::__rlimit_resource_t, value: u64) -> Result<(), String> {
    let r = libc::rlimit {
        rlim_cur: value,
        rlim_max: value,
    };
    if unsafe { libc::setrlimit(resource, &r) } != 0 {
        Err(os_error("setrlimit"))
    } else {
        Ok(())
    }
}
// BPF offsets are the Linux x86-64 seccomp_data ABI: nr=0, arch=4, args=16.
fn filter(loader: bool) -> Result<(), String> {
    let stmt = |code, k| libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    };
    let jump = |k, jt, jf| libc::sock_filter {
        code: 0x15,
        jt,
        jf,
        k,
    };
    let mut f = vec![
        stmt(0x20, 4),
        jump(0xc000003e, 1, 0),
        stmt(0x06, 0x80000000),
        stmt(0x20, 0),
    ];
    // openat is permitted only with read-only flags. All forbidden calls kill, never return fake success.
    if loader {
        f.extend([
            jump(libc::SYS_openat as u32, 0, 4),
            stmt(0x20, 32),
            stmt(
                0x54,
                (libc::O_ACCMODE | libc::O_CREAT | libc::O_TRUNC | libc::O_APPEND | libc::O_TMPFILE)
                    as u32,
            ),
            jump(0, 0, 1),
            stmt(0x06, 0x7fff0000),
            stmt(0x20, 0),
        ]);
    }
    if loader {
        f.extend([jump(libc::SYS_prctl as u32, 0, 1), stmt(0x06, 0x7fff0000)]);
    }
    for syscall in [
        libc::SYS_read,
        libc::SYS_pread64,
        libc::SYS_close,
        libc::SYS_fstat,
        libc::SYS_newfstatat,
        libc::SYS_lseek,
        libc::SYS_mmap,
        libc::SYS_mprotect,
        libc::SYS_munmap,
        libc::SYS_brk,
        libc::SYS_futex,
        libc::SYS_madvise,
        libc::SYS_exit,
        libc::SYS_exit_group,
    ] {
        if !loader
            && [
                libc::SYS_read,
                libc::SYS_pread64,
                libc::SYS_fstat,
                libc::SYS_newfstatat,
                libc::SYS_lseek,
            ]
            .contains(&syscall)
        {
            continue;
        }
        f.extend([jump(syscall as u32, 0, 1), stmt(0x06, 0x7fff0000)]);
    }
    f.push(stmt(0x06, 0x80000000));
    let program = libc::sock_fprog {
        len: f.len() as u16,
        filter: f.as_mut_ptr(),
    };
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0
        || unsafe { libc::prctl(libc::PR_SET_SECCOMP, 2, &program) } != 0
    {
        return Err(os_error("seccomp installation"));
    }
    Ok(())
}
unsafe fn invoke(address: *mut libc::c_void, args: &[Value]) -> u64 {
    let mut registers = [0u64; 4];
    for (i, v) in args.iter().enumerate() {
        registers[i] = if v.ty.signed() {
            v.signed() as u64
        } else {
            v.bits
        };
    }
    let result: u64;
    // r12 is saved explicitly and is callee-saved under sysv64. The stack is aligned before CALL.
    asm!("push r12","mov r12, rsp","and rsp, -16","call r11","mov rsp, r12","pop r12",in("r11") address,in("rdi") registers[0],in("rsi") registers[1],in("rdx") registers[2],in("rcx") registers[3],lateout("rax") result,clobber_abi("sysv64"));
    result
}
pub fn execute(request: &Request) -> Result<Vec<Value>, String> {
    request.contract.validate()?;
    request.signature.validate()?;
    if request.inputs.len() > MAX_BATCH {
        return Err("oracle batch exceeds limit".into());
    }
    for args in &request.inputs {
        encode_transport(&request.signature, args)?;
    }
    if hash(&std::fs::read(&request.contract.path).map_err(|e| e.to_string())?)
        != request.contract.sha256.to_lowercase()
    {
        return Err("worker binary hash mismatch".into());
    }
    let path = CString::new(request.contract.path.as_str()).map_err(|_| "invalid binary path")?;
    let symbol = CString::new(request.contract.symbol.as_str()).map_err(|_| "invalid symbol")?;
    let memory = unsafe {
        libc::mmap(
            ptr::null_mut(),
            mem::size_of::<Shared>(),
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if memory == libc::MAP_FAILED {
        return Err(os_error("shared result allocation"));
    }
    let shared = memory.cast::<Shared>();
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        unsafe {
            libc::munmap(memory, mem::size_of::<Shared>());
        }
        return Err(os_error("worker fork"));
    }
    if pid == 0 {
        let result = (|| {
            if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } != 0 {
                return Err(os_error("parent death signal"));
            }
            limit(libc::RLIMIT_AS, request.contract.memory_mb * 1024 * 1024)?;
            limit(libc::RLIMIT_CPU, request.contract.cpu_seconds)?;
            limit(libc::RLIMIT_CORE, 0)?;
            limit(libc::RLIMIT_FSIZE, 0)?;
            if unsafe { libc::syscall(libc::SYS_close_range, 0u32, u32::MAX, 0u32) } != 0 {
                return Err(os_error("close inherited descriptors"));
            }
            filter(true)?;
            let library = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            if library.is_null() {
                return Err("dlopen failed".into());
            }
            let function = unsafe { libc::dlsym(library, symbol.as_ptr()) };
            if function.is_null() {
                unsafe {
                    (*shared).status = 2;
                }
                return Err("missing symbol".into());
            }
            filter(false)?;
            for (i, args) in request.inputs.iter().enumerate() {
                let first = Value::new(request.signature.return_type, unsafe {
                    invoke(function, args)
                });
                let second = Value::new(request.signature.return_type, unsafe {
                    invoke(function, args)
                });
                if first != second {
                    unsafe {
                        (*shared).status = 3;
                    }
                    return Err("nondeterministic return".into());
                }
                unsafe {
                    (*shared).values[i] = first.bits;
                    (*shared).count = (i + 1) as u32;
                }
            }
            unsafe {
                (*shared).status = 1;
            }
            Ok::<(), String>(())
        })();
        if result.is_err() {
            unsafe {
                if (*shared).status == 0 {
                    (*shared).status = 4;
                }
            }
        }
        unsafe {
            libc::_exit(0);
        }
    }
    let mut status = 0;
    let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
    let result = if waited < 0 {
        Err(os_error("waitpid"))
    } else if libc::WIFSIGNALED(status) {
        Err(format!(
            "oracle terminated by signal {} (trap, forbidden syscall, or resource limit)",
            libc::WTERMSIG(status)
        ))
    } else {
        let result = unsafe { &*shared };
        match result.status {
            1 if result.count as usize == request.inputs.len() => Ok(result.values
                [..request.inputs.len()]
                .iter()
                .map(|v| Value::new(request.signature.return_type, *v))
                .collect()),
            2 => Err("missing oracle symbol".into()),
            3 => Err("oracle nondeterministic return".into()),
            4 => Err("oracle loader or isolation setup failed".into()),
            _ => Err("oracle exited without a complete result".into()),
        }
    };
    unsafe {
        libc::munmap(memory, mem::size_of::<Shared>());
    }
    result
}
