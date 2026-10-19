//! Raw kernel ABI only. Negative errno is returned directly, no TLS/errno/libc.
//! Spawn trampolines branch to the child leaf BEFORE returning into Rust parent
//! frames. No CLONE_VM/FILES/FS/SIGHAND; stack is a private COW copy.
//! Raw parent publishes the full return to an aligned AtomicIsize BEFORE return
//! to Rust/hooks/mask restoration. x86 aligned MOV is release on this ABI; ARM64
//! uses STLR. Consumers use Acquire; the external asm call has memory effects.
//! Zero child branch NEVER publishes. Publication pointer is callee-saved.
#[inline(always)]
pub(super) unsafe fn syscall(n: i64, args: [usize; 6]) -> isize {
    let result: isize;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("syscall", inlateout("rax") n as isize => result,
        in("rdi") args[0], in("rsi") args[1], in("rdx") args[2],
        in("r10") args[3], in("r8") args[4], in("r9") args[5],
        lateout("rcx") _, lateout("r11") _, options(nostack));
    #[cfg(target_arch = "aarch64")]
    core::arch::asm!("svc 0", in("x8") n, inlateout("x0") args[0] as isize => result,
        in("x1") args[1], in("x2") args[2], in("x3") args[3],
        in("x4") args[4], in("x5") args[5], options(nostack));
    result
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(r#"
.text
.hidden x11_credential_clone3
.global x11_credential_clone3
.type x11_credential_clone3,@function
x11_credential_clone3:
    push r12
    push r13
    sub rsp, 8
    mov r13, rcx
    mov r12, rdx
    mov eax, {clone3}
    syscall
    test rax, rax
    jz 2f
    mov qword ptr [r13], rax
    add rsp, 8
    pop r13
    pop r12
    ret
2:
    mov rdi, r12
    call x11_credential_child_leaf
    ud2
.size x11_credential_clone3, .-x11_credential_clone3
.hidden x11_credential_legacy
.global x11_credential_legacy
.type x11_credential_legacy,@function
x11_credential_legacy:
    push r12
    push r13
    sub rsp, 8
    mov r13, rcx
    mov r12, rdx
    mov rdx, rsi
    xor esi, esi
    xor r10d, r10d
    xor r8d, r8d
    mov eax, {clone}
    syscall
    test rax, rax
    jz 2f
    mov qword ptr [r13], rax
    add rsp, 8
    pop r13
    pop r12
    ret
2:
    mov rdi, r12
    call x11_credential_child_leaf
    ud2
.size x11_credential_legacy, .-x11_credential_legacy
"#, clone3 = const libc::SYS_clone3, clone = const libc::SYS_clone);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(r#"
.text
.hidden x11_credential_clone3
.global x11_credential_clone3
.type x11_credential_clone3,%function
x11_credential_clone3:
    stp x19, x20, [sp, #-32]!
    str x30, [sp, #16]
    mov x20, x3
    mov x19, x2
    mov x8, #{clone3}
    svc #0
    cbz x0, 2f
    stlr x0, [x20]
    ldr x30, [sp, #16]
    ldp x19, x20, [sp], #32
    ret
2:
    mov x0, x19
    bl x11_credential_child_leaf
    brk #0
.size x11_credential_clone3, .-x11_credential_clone3
.hidden x11_credential_legacy
.global x11_credential_legacy
.type x11_credential_legacy,%function
x11_credential_legacy:
    stp x19, x20, [sp, #-32]!
    str x30, [sp, #16]
    mov x20, x3
    mov x19, x2
    mov x2, x1
    mov x1, #0
    mov x3, #0
    mov x4, #0
    mov x8, #{clone}
    svc #0
    cbz x0, 2f
    stlr x0, [x20]
    ldr x30, [sp, #16]
    ldp x19, x20, [sp], #32
    ret
2:
    mov x0, x19
    bl x11_credential_child_leaf
    brk #0
.size x11_credential_legacy, .-x11_credential_legacy
"#, clone3 = const libc::SYS_clone3, clone = const libc::SYS_clone);

#[repr(C)]
#[derive(Default)]
pub(super) struct CloneArgs {
    pub flags: u64,
    pub pidfd: u64,
    pub child_tid: u64,
    pub parent_tid: u64,
    pub exit_signal: u64,
    pub stack: u64,
    pub stack_size: u64,
    pub tls: u64,
    pub set_tid: u64,
    pub set_tid_size: u64,
    pub cgroup: u64,
}
extern "C" {
    pub(super) fn x11_credential_clone3(
        args: *const CloneArgs,
        size: usize,
        child: *mut super::leaf::Child,
        publication: *mut isize,
    ) -> isize;
    pub(super) fn x11_credential_legacy(
        flags: usize,
        pidfd: *mut i32,
        child: *mut super::leaf::Child,
        publication: *mut isize,
    ) -> isize;
}
