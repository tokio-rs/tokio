//! USDT support on linux, and other platforms that use the SystemTap SDT V3 system.
//!
//! Implementer details:
//!
//! ## Calling probes
//!
//! To update or add new probes, you should be able to copy an existing probe for inspiration.
//! For a guide on the register formats, see:
//! * <https://doc.rust-lang.org/reference/inline-assembly.html#template-modifiers>
//! * <https://doc.rust-lang.org/reference/inline-assembly.html#r-asm.register-operands.supported-register-classes>
//!
//! There should be no need to modify the macros provided if you just want to add a new probe.
//!
//! ## Semaphores
//!
//! Probes can either have a semaphore (with `semaphore = sym $ident`),
//! or they can have no semaphore (with `semaphore = const 0`).
//!
//! A semaphore lets you know if a program is attached to the given probe.
//! This can help offload any expensive setup, such as in `task_details` which needs to
//! allocate a null-termianted string.
//!
//! If you have some expensive setup needed to call the probe, consider moving it
//! into a seperate function and marking it as `#[cold]` or `#[inline(never)]`.
//! This ensures the hot path (probe is disabled) doesn't need to skip over a large
//! number of instructions, and the branch predictor can make better predictions.
#![allow(named_asm_labels)]

#[cfg(target_arch = "x86_64")]
macro_rules! call_probe {
    (
        $name:literal,
        x86_64: $x86_64:literal,
        aarch64: $aarch64:literal,
        $($args:tt)*
    ) => {
        // <https://sourceware.org/systemtap/wiki/UserSpaceProbeImplementation>
        // <https://github.com/cdkey/systemtap/blob/cbb34b7244ba60cb0904d61dc9167290855106aa/includes/sys/sdt.h#L240-L254>
        //
        // The note section `o` flag and the named labels here are significant to ensure
        // that the note is discarded or retained with the nop as a unit. Either both
        // are discarded or both are retained.
        std::arch::asm!(
            ".Lprobe_{label}:   nop
                    .pushsection .note.stapsdt, \"o\", \"note\", .Lprobe_{label}
                    .balign 4
                    .4byte 992f-991f, 994f-993f, 3
            991:
                    .asciz \"stapsdt\"
            992:
                    .balign 4
            993:
                    .8byte .Lprobe_{label}
                    .8byte _.stapsdt.base
                    .8byte {semaphore}
                    .asciz \"tokio\"",
            concat!(".asciz \"", $name, "\""),
            concat!(".asciz \"", $x86_64, "\""),
            "994:
                    .balign 4
                    .popsection",
            $($args)*
            // nasty hack to get a unique label name.
            label = label {},
            options(att_syntax, readonly, nostack, preserves_flags),
        );
    };
}

#[cfg(target_arch = "aarch64")]
macro_rules! call_probe {
    (
        $name:literal,
        x86_64: $x86_64:literal,
        aarch64: $aarch64:literal,
        $($args:tt)*
    ) => {
        // <https://sourceware.org/systemtap/wiki/UserSpaceProbeImplementation>
        // <https://github.com/cdkey/systemtap/blob/cbb34b7244ba60cb0904d61dc9167290855106aa/includes/sys/sdt.h#L240-L254>
        //
        // The note section `o` flag and the named labels here are significant to ensure
        // that the note is discarded or retained with the nop as a unit. Either both
        // are discarded or both are retained.
        std::arch::asm!(
            ".Lprobe_{label}:   nop
                    .pushsection .note.stapsdt, \"o\", \"note\", .Lprobe_{label}
                    .balign 4
                    .4byte 992f-991f, 994f-993f, 3
            991:
                    .asciz \"stapsdt\"
            992:
                    .balign 4
            993:
                    .8byte .Lprobe_{label}
                    .8byte _.stapsdt.base
                    .8byte {semaphore}
                    .asciz \"tokio\"",
            concat!(".asciz \"", $name, "\""),
            concat!(".asciz \"", $aarch64, "\""),
            "994:
                    .balign 4
                    .popsection",
            $($args)*
            // nasty hack to get a unique label name.
            label = label {},
            options(readonly, nostack, preserves_flags),
        );
    };
}

#[allow(unused)]
macro_rules! declare_semaphore {
    ($name:ident) => {
        extern "C" {
            static mut $name: u16;
        }

        std::arch::global_asm!(
            ".ifndef {semaphore}
                    .pushsection .probes, \"aw\", \"progbits\"
                    .weak {semaphore}
                    .hidden {semaphore}
            {semaphore}:
                    .zero 2
                    .type {semaphore}, @object
                    .size {semaphore}, 2
                    .popsection
            .endif",
            semaphore = sym $name
        );
    };
}

cfg_rt!(
    declare_semaphore!(__usdt_sema_tokio_task__details);

    // `inline(always)` is ok here since we only inline into `super::start_task` which is `inline(never)`
    #[inline(always)]
    pub(super) fn task_details(task_id: u64, name: &str, file: &std::ffi::CStr, line: u32, col: u32) {
        #[cold]
        fn task_details_inner(task_id: u64, name: &str, file: &std::ffi::CStr, line: u32, col: u32) {
            // add nul bytes
            let name0 = [name.as_bytes(), b"\0"].concat();

            unsafe {
                call_probe!(
                    "task-details",
                    x86_64: "8@{task_id:r} 8@{name} 8@{file} 4@{line:e} 4@{col:e}",
                    aarch64: "8@{task_id} 8@{name} 8@{file} 4@{line:w} 4@{col:w}",
                    semaphore = sym __usdt_sema_tokio_task__details,
                    task_id = in(reg) task_id,
                    name = in(reg) name0.as_ptr(),
                    file = in(reg) file.as_ptr(),
                    line = in(reg) line,
                    col = in(reg) col,
                );
            }
        }

        if unsafe { __usdt_sema_tokio_task__details } != 0 {
            task_details_inner(task_id, name, file, line, col);
        }
    }

    // `inline(always)` is ok here since we only inline into `super::start_task` which is `inline(never)`
    #[inline(always)]
    pub(super) fn task_start(task_id: u64, kind: u8, size: usize, original_size: usize) {
        unsafe {
            call_probe!(
                "task-start",
                x86_64: "8@{task_id:r} 1@{kind:l} 8@{size} 8@{original_size}",
                aarch64: "8@{task_id} 1@{kind:w} 8@{size} 8@{original_size}",
                semaphore = const 0,
                task_id = in(reg) task_id,
                kind = in(reg) kind as u32,
                size = in(reg) size,
                original_size = in(reg) original_size,
            );
        }
    }

    #[inline(always)]
    pub(super) fn task_poll_start(task_id: u64) {
        unsafe {
            call_probe!(
                "task-poll-start",
                x86_64: "8@{task_id:r}",
                aarch64: "8@{task_id}",
                semaphore = const 0,
                task_id = in(reg) task_id,
            );
        }
    }

    #[inline(always)]
    pub(super) fn task_poll_end(task_id: u64) {
        unsafe {
            call_probe!(
                "task-poll-end",
                x86_64: "8@{task_id:r}",
                aarch64: "8@{task_id}",
                semaphore = const 0,
                task_id = in(reg) task_id,
            );
        }
    }

    #[inline(always)]
    pub(super) fn task_terminate(task_id: u64, reason: u8) {
        unsafe {
            call_probe!(
                "task-terminate",
                x86_64: "8@{task_id:r} 1@{reason:l}",
                aarch64: "8@{task_id} 1@{reason:w}",
                semaphore = const 0,
                task_id = in(reg) task_id,
                reason = in(reg) reason as u32,
            );
        }
    }

    // `inline(always)` is ok here since wakers are polymorphised.
    #[inline(always)]
    pub(crate) fn waker_clone(task_id: u64) {
        unsafe {
            call_probe!(
                "waker-clone",
                x86_64: "8@{task_id:r}",
                aarch64: "8@{task_id}",
                semaphore = const 0,
                task_id = in(reg) task_id,
            );
        }
    }

    // `inline(always)` is ok here since wakers are polymorphised.
    #[inline(always)]
    pub(crate) fn waker_drop(task_id: u64) {
        unsafe {
            call_probe!(
                "waker-drop",
                x86_64: "8@{task_id:r}",
                aarch64: "8@{task_id}",
                semaphore = const 0,
                task_id = in(reg) task_id,
            );
        }
    }

    // `inline(always)` is ok here since wakers are polymorphised.
    #[inline(always)]
    pub(crate) fn waker_wake(task_id: u64) {
        unsafe {
            call_probe!(
                "waker-wake",
                x86_64: "8@{task_id:r}",
                aarch64: "8@{task_id}",
                semaphore = const 0,
                task_id = in(reg) task_id,
            );
        }
    }
);

#[inline(always)]
pub(crate) fn trace_root() {
    unsafe {
        call_probe!(
            "trace-root",
            x86_64: "",
            aarch64: "",
            semaphore = const 0,
        );
    }
}

std::arch::global_asm!(
    ".ifndef _.stapsdt.base
            .pushsection .stapsdt.base, \"aGR\", \"progbits\", .stapsdt.base, comdat
            .weak _.stapsdt.base
            .hidden _.stapsdt.base
    _.stapsdt.base:
            .space 1
            .size _.stapsdt.base, 1
            .popsection
    .endif",
);
