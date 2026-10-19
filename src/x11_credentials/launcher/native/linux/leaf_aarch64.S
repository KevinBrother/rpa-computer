// Child-only, no external calls. x19=context x20=file x21=len x22=status.
// All stack storage is private COW, aligned 16 bytes. Never return to Rust.
.text
.hidden x11_credential_child_leaf
.global x11_credential_child_leaf
.type x11_credential_child_leaf,%function
x11_credential_child_leaf:
    mov x19, x0
.if {lifecycle_enabled}
    mov w0, #1
    bl .Lcred_lifecycle
.endif
    mov x8, #{prctl}
    mov x0, #{pdeathsig}
    mov x1, #{sigkill}
    mov x2, #0
    mov x3, #0
    mov x4, #0
    svc #0
    cbnz x0, .Lcred_bad
.if {lifecycle_enabled}
    mov w0, #2
    bl .Lcred_lifecycle
.endif
    mov x8, #{getppid}
    svc #0
    ldr w1, [x19, #{parent_pid}]
    cmp w0, w1
    b.ne .Lcred_bad
    mov x8, #{tgkill}
    ldr w0, [x19, #{parent_pid}]
    ldr w1, [x19, #{parent_tid}]
    mov x2, #0
    svc #0
    cbnz x0, .Lcred_bad
    // Numeric group+TID evidence, absent original-lifetime witness: index3.
    mov w0, #{creator_admission}
    tbz w0, #3, .Lcred_bad
    ldr w0, [x19, #{endpoint}]
    cmp w0, #3
    b.eq .Lcred_close_ranges
    mov x8, #{dup3}
    mov x1, #3
    mov x2, #{cloexec}
    svc #0
    cmp x0, #3
    b.ne .Lcred_bad
.Lcred_close_ranges:
    mov x8, #{close_range}
    mov x0, #0
    mov x1, #2
    mov x2, #0
    svc #0
    cbnz x0, .Lcred_bad
    mov x8, #{close_range}
    mov x0, #4
    mov w1, #-1
    mov x2, #0
    svc #0
    cbnz x0, .Lcred_bad
    mov w0, #1
    bl .Lcred_checkpoint
    ldrb w0, [x19, #{checkpoints}]
    cbz w0, .Lcred_open
    mov w0, #2
    bl .Lcred_checkpoint
.Lcred_open:
.if {lifecycle_enabled}
    mov w0, #4
    bl .Lcred_lifecycle
.endif
    bl .Lcred_fresh
.if {lifecycle_enabled}
    ldr x9, [x19, #{lifecycle_ptr}]
    cbz x9, .Lcred_open_unobserved
    mov w10, #1
    add x11, x9, #{probe_entered}
    stlr w10, [x11]
.Lcred_open_unobserved:
.endif
    mov x8, #{openat}
    mov x0, #{at_fdcwd}
    ldr x1, [x19, #{path}]
    mov x2, #{cloexec}
    orr x2, x2, #{nonblock}
    mov x3, #0
    svc #0
.if {lifecycle_enabled}
    ldr x9, [x19, #{lifecycle_ptr}]
    cbz x9, .Lcred_return_unobserved
    mov w10, #1
    add x11, x9, #{probe_returned}
    stlr w10, [x11]
.Lcred_return_unobserved:
.endif
    mov x21, #0
    mov x22, #0
    cmp x0, #0
    b.ge .Lcred_stat
    mov w22, #2
    cmn x0, #{enoent}
    b.ne .Lcred_frame
    mov w22, #1
    b .Lcred_frame
.Lcred_stat:
    mov x20, x0
    mov x0, x20
    add x1, x19, #{stat}
    mov x8, #{fstat}
    svc #0
    cbnz x0, .Lcred_read_error
    ldr w0, [x19, #{stat_mode}]
    and w0, w0, #{ifmt}
    cmp w0, #{ifreg}
    b.eq .Lcred_size
    mov w22, #3
    b .Lcred_close
.Lcred_size:
    ldr x0, [x19, #{stat_size}]
    mov x1, #{max_bytes}
    cmp x0, x1
    b.hi .Lcred_oversized
.Lcred_read:
    mov x8, #{read}
    mov x0, x20
    ldr x1, [x19, #{bytes}]
    add x1, x1, x21
    mov x2, #{max_bytes}
    add x2, x2, #1
    sub x2, x2, x21
    svc #0
    cmn x0, #{eintr}
    b.eq .Lcred_read
    cmp x0, #0
    b.lt .Lcred_read_error
    b.eq .Lcred_close
    cmp x0, x2
    b.hi .Lcred_read_error
    add x21, x21, x0
    mov x1, #{max_bytes}
    cmp x21, x1
    b.ls .Lcred_read
.Lcred_oversized:
    mov w22, #4
    b .Lcred_close
.Lcred_read_error:
    mov w22, #2
.Lcred_close:
    mov x8, #{close}
    mov x0, x20
    svc #0
    cbz x0, .Lcred_frame
    mov w22, #2
.Lcred_frame:
    ldrb w0, [x19, #{checkpoints}]
    cbz w0, .Lcred_header
    mov w0, #3
    bl .Lcred_checkpoint
.Lcred_header:
    cbz x22, .Lcred_length
    mov x21, #0
.Lcred_length:
    strb w22, [x19, #{frame_status}]
    rev w0, w21
    str w0, [x19, #{frame_length}]
    add x1, x19, #{frame}
    mov x2, #12
    bl .Lcred_send
    ldr x1, [x19, #{bytes}]
    mov x2, x21
    bl .Lcred_send
    mov x0, #0
    b .Lcred_exit

.Lcred_checkpoint:
    sub sp, sp, #32
    str x30, [sp, #16]
    strb w0, [sp]
    mov x1, sp
    mov x2, #1
    bl .Lcred_send
    bl .Lcred_permission
    ldr x30, [sp, #16]
    add sp, sp, #32
    ret
.Lcred_permission:
    sub sp, sp, #16
    strb wzr, [sp]
    str x30, [sp, #8]
.if {lifecycle_enabled}
    mov w0, #3
    bl .Lcred_lifecycle
.endif
.Lcred_permission_retry:
    mov x8, #{read}
    mov x0, #3
    mov x1, sp
    mov x2, #1
    svc #0
    cmn x0, #{eintr}
    b.eq .Lcred_permission_retry
    cmp x0, #1
    b.ne .Lcred_bad
    ldrb w0, [sp]
    cmp w0, #0xa5
    b.ne .Lcred_bad
    bl .Lcred_fresh
    ldr x30, [sp, #8]
    add sp, sp, #16
    ret
// Same conservative absolute CLOCK_MONOTONIC deadline as the x86-64 leaf.
// x30 is untouched here; callers that themselves return save it before BL.
.Lcred_fresh:
    sub sp, sp, #16
    mov x8, #{clock_gettime}
    mov x0, #{monotonic}
    mov x1, sp
    svc #0
    cbnz x0, .Lcred_bad
    ldr x0, [sp]
    ldr x1, [x19, #{deadline_seconds}]
    cmp x0, x1
    b.gt .Lcred_bad
    b.lt .Lcred_fresh_done
    ldr x0, [sp, #8]
    ldr x1, [x19, #{deadline_nanos}]
    cmp x0, x1
    b.ge .Lcred_bad
.Lcred_fresh_done:
    add sp, sp, #16
    ret
.Lcred_send:
    cbz x2, .Lcred_send_done
    mov x8, #{sendto}
    mov x0, #3
    mov x3, #{nosignal}
    mov x4, #0
    mov x5, #0
    svc #0
    cmn x0, #{eintr}
    b.eq .Lcred_send
    cmp x0, #0
    b.le .Lcred_bad
    cmp x0, x2
    b.hi .Lcred_bad
    add x1, x1, x0
    sub x2, x2, x0
    b .Lcred_send
.Lcred_send_done:
    ret
.if {lifecycle_enabled}
// Fixture-only acquire/release shared-memory observations and raw shared futex.
.Lcred_lifecycle:
    ldr x9, [x19, #{lifecycle_ptr}]
    cbz x9, .Lcred_probe_done
    mov w10, #1
    lsl w10, w10, w0
    add x11, x9, #{probe_reached}
    stlr w0, [x11]
    add x11, x9, #{probe_hold}
    ldar w12, [x11]
    tst w12, w10
    b.eq .Lcred_probe_done
    add x11, x9, #{probe_release}
.Lcred_probe_wait:
    ldar w12, [x11]
    tst w12, w10
    b.ne .Lcred_probe_done
    mov x0, x11
    mov x1, #0
    mov w2, w12
    mov x3, #0
    mov x4, #0
    mov x5, #0
    mov x8, #{futex}
    svc #0
    b .Lcred_probe_wait
.Lcred_probe_done:
    ret
.endif
.Lcred_bad:
    mov x0, #127
.Lcred_exit:
    mov x8, #{exit_group}
    svc #0
    brk #0
.size x11_credential_child_leaf, .-x11_credential_child_leaf
