// Child-only, no external calls. r12=context, r13=file, r14=len, r15=status.
// The trampoline called us with a private COW stack. Never return to Rust.
.text
.hidden x11_credential_child_leaf
.global x11_credential_child_leaf
.type x11_credential_child_leaf,@function
x11_credential_child_leaf:
    mov r12, rdi
.if {lifecycle_enabled}
    mov edi, 1
    call .Lcred_lifecycle
.endif
    mov eax, {prctl}
    mov edi, {pdeathsig}
    mov esi, {sigkill}
    xor edx, edx
    xor r10d, r10d
    xor r8d, r8d
    syscall
    test rax, rax
    jne .Lcred_bad
.if {lifecycle_enabled}
    mov edi, 2
    call .Lcred_lifecycle
.endif
    mov eax, {getppid}
    syscall
    cmp eax, dword ptr [r12 + {parent_pid}]
    jne .Lcred_bad
    mov eax, {tgkill}
    mov edi, dword ptr [r12 + {parent_pid}]
    mov esi, dword ptr [r12 + {parent_tid}]
    xor edx, edx
    syscall
    test rax, rax
    jne .Lcred_bad
    // Current protocol has numeric evidence only: table index3, no lifetime witness.
    mov eax, {creator_admission}
    bt eax, 3
    jnc .Lcred_bad
    mov edi, dword ptr [r12 + {endpoint}]
    cmp edi, 3
    je .Lcred_close_ranges
    mov eax, {dup3}
    mov esi, 3
    mov edx, {cloexec}
    syscall
    cmp rax, 3
    jne .Lcred_bad
.Lcred_close_ranges:
    mov eax, {close_range}
    xor edi, edi
    mov esi, 2
    xor edx, edx
    syscall
    test rax, rax
    jne .Lcred_bad
    mov eax, {close_range}
    mov edi, 4
    mov esi, -1
    xor edx, edx
    syscall
    test rax, rax
    jne .Lcred_bad
    mov edi, 1
    call .Lcred_checkpoint
    cmp byte ptr [r12 + {checkpoints}], 0
    je .Lcred_open
    mov edi, 2
    call .Lcred_checkpoint
.Lcred_open:
.if {lifecycle_enabled}
    mov edi, 4
    call .Lcred_lifecycle
.endif
    call .Lcred_fresh
.if {lifecycle_enabled}
    mov rdx, qword ptr [r12 + {lifecycle_ptr}]
    test rdx, rdx
    jz .Lcred_open_unobserved
    mov dword ptr [rdx + {probe_entered}], 1
.Lcred_open_unobserved:
.endif
    mov eax, {openat}
    mov rdi, {at_fdcwd}
    mov rsi, qword ptr [r12 + {path}]
    mov edx, {open_flags}
    xor r10d, r10d
    syscall
.if {lifecycle_enabled}
    mov rdx, qword ptr [r12 + {lifecycle_ptr}]
    test rdx, rdx
    jz .Lcred_return_unobserved
    mov dword ptr [rdx + {probe_returned}], 1
.Lcred_return_unobserved:
.endif
    xor r14d, r14d
    xor r15d, r15d
    test rax, rax
    jns .Lcred_stat
    mov r15d, 2
    cmp rax, -{enoent}
    jne .Lcred_frame
    mov r15d, 1
    jmp .Lcred_frame
.Lcred_stat:
    mov r13, rax
    mov rdi, r13
    lea rsi, [r12 + {stat}]
    mov eax, {fstat}
    syscall
    test rax, rax
    jne .Lcred_read_error
    mov eax, dword ptr [r12 + {stat_mode}]
    and eax, {ifmt}
    cmp eax, {ifreg}
    je .Lcred_size
    mov r15d, 3
    jmp .Lcred_close
.Lcred_size:
    mov rax, qword ptr [r12 + {stat_size}]
    // Unsigned above also rejects negative size.
    cmp rax, {max_bytes}
    ja .Lcred_oversized
.Lcred_read:
    mov eax, {read}
    mov rdi, r13
    mov rsi, qword ptr [r12 + {bytes}]
    add rsi, r14
    mov edx, {buffer_bytes}
    sub rdx, r14
    syscall
    cmp rax, -{eintr}
    je .Lcred_read
    test rax, rax
    js .Lcred_read_error
    jz .Lcred_close
    cmp rax, rdx
    ja .Lcred_read_error
    add r14, rax
    cmp r14, {max_bytes}
    jbe .Lcred_read
.Lcred_oversized:
    mov r15d, 4
    jmp .Lcred_close
.Lcred_read_error:
    mov r15d, 2
.Lcred_close:
    mov eax, {close}
    mov rdi, r13
    syscall
    test rax, rax
    je .Lcred_frame
    // Never retry Linux close, including EINTR.
    mov r15d, 2
.Lcred_frame:
    cmp byte ptr [r12 + {checkpoints}], 0
    je .Lcred_header
    mov edi, 3
    call .Lcred_checkpoint
.Lcred_header:
    test r15d, r15d
    jz .Lcred_length
    xor r14d, r14d
.Lcred_length:
    mov byte ptr [r12 + {frame_status}], r15b
    mov eax, r14d
    bswap eax
    mov dword ptr [r12 + {frame_length}], eax
    lea rsi, [r12 + {frame}]
    mov edx, 12
    call .Lcred_send
    mov rsi, qword ptr [r12 + {bytes}]
    mov rdx, r14
    call .Lcred_send
    xor edi, edi
    jmp .Lcred_exit

// Each checkpoint emits one byte then waits for exact 0xa5 permission on fd3.
.Lcred_checkpoint:
    push rdi
    mov rsi, rsp
    mov edx, 1
    call .Lcred_send
    call .Lcred_permission
    add rsp, 8
    ret
.Lcred_permission:
    push 0
.if {lifecycle_enabled}
    mov edi, 3
    call .Lcred_lifecycle
.endif
.Lcred_permission_retry:
    mov eax, {read}
    mov edi, 3
    mov rsi, rsp
    mov edx, 1
    syscall
    cmp rax, -{eintr}
    je .Lcred_permission_retry
    cmp rax, 1
    jne .Lcred_bad
    cmp byte ptr [rsp], 0xa5
    jne .Lcred_bad
    call .Lcred_fresh
    add rsp, 8
    ret
// Freshness at permission consumption and immediately before the path syscall.
// Kernel timespec is two signed 64-bit fields on this supported ABI. Fail closed
// on clock failure or now >= deadline. Only raw syscalls and private stack.
.Lcred_fresh:
    sub rsp, 16
    mov eax, {clock_gettime}
    mov edi, {monotonic}
    mov rsi, rsp
    syscall
    test rax, rax
    jne .Lcred_bad
    mov rax, qword ptr [rsp]
    cmp rax, qword ptr [r12 + {deadline_seconds}]
    jg .Lcred_bad
    jl .Lcred_fresh_done
    mov rax, qword ptr [rsp + 8]
    cmp rax, qword ptr [r12 + {deadline_nanos}]
    jge .Lcred_bad
.Lcred_fresh_done:
    add rsp, 16
    ret
// rsi=source rdx=remaining; only fd3, MSG_NOSIGNAL, bounded source memory.
.Lcred_send:
    test rdx, rdx
    jz .Lcred_send_done
    mov eax, {sendto}
    mov edi, 3
    mov r10d, {nosignal}
    xor r8d, r8d
    xor r9d, r9d
    syscall
    cmp rax, -{eintr}
    je .Lcred_send
    test rax, rax
    jle .Lcred_bad
    cmp rax, rdx
    ja .Lcred_bad
    add rsi, rax
    sub rdx, rax
    jmp .Lcred_send
.Lcred_send_done:
    ret
.if {lifecycle_enabled}
// Fixture-only shared-memory barrier; no callback, allocation or external call.
// Preserve leaf's data registers and use raw shared futex (not FUTEX_PRIVATE).
.Lcred_lifecycle:
    push rbx
    push rbp
    mov rbx, qword ptr [r12 + {lifecycle_ptr}]
    test rbx, rbx
    jz .Lcred_probe_done
    mov ecx, edi
    mov ebp, 1
    shl ebp, cl
    mov dword ptr [rbx + {probe_reached}], edi
    test dword ptr [rbx + {probe_hold}], ebp
    jz .Lcred_probe_done
.Lcred_probe_wait:
    mov eax, dword ptr [rbx + {probe_release}]
    test eax, ebp
    jnz .Lcred_probe_done
    mov edx, eax
    lea rdi, [rbx + {probe_release}]
    xor esi, esi
    xor r10d, r10d
    xor r8d, r8d
    xor r9d, r9d
    mov eax, {futex}
    syscall
    jmp .Lcred_probe_wait
.Lcred_probe_done:
    pop rbp
    pop rbx
    ret
.endif
.Lcred_bad:
    mov edi, 127
.Lcred_exit:
    mov eax, {exit_group}
    syscall
    ud2
.size x11_credential_child_leaf, .-x11_credential_child_leaf
