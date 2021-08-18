use std::path::Path;

use probe_rs::{config::RamRegion, Core};

use crate::elf::Elf;

mod pp;
mod symbolicate;
mod unwind;

// change as follows:
// --force-backtrace is removed
// --backtrace-len is renamed to --backtrace-limit
// --backtrace is added

// Additionally,
// --backtrace flag is optional and defaults to auto
// --backtrace-limit flag is optional and defaults to 50 (+)
// --backtrace-limit=0 is accepted and means "no limit"

#[derive(PartialEq, Eq)]
pub(crate) enum BacktraceOptions {
    Auto, Never, Always
}

impl From<&String> for BacktraceOptions {
    fn from(item: &String) -> Self {
        match item.as_str() {
            "auto" | "Auto" => BacktraceOptions::Auto,
            "never" | "Never" => BacktraceOptions::Never,
            "always" | "Always" => BacktraceOptions::Always,
            _ => panic!("options for `--backtrace` are `auto`, `never`, `always`.")
        }
    }
}

pub(crate) struct Settings<'p> {
    pub(crate) current_dir: &'p Path,
    pub(crate) backtrace: BacktraceOptions,
    pub(crate) backtrace_limit: u32,
    pub(crate) shorten_paths: bool,
}

/// (virtually) unwinds the target's program and prints its backtrace
pub(crate) fn print(
    core: &mut Core,
    elf: &Elf,
    active_ram_region: &Option<RamRegion>,
    settings: &mut Settings<'_>,
) -> anyhow::Result<Outcome> {
    let unwind = unwind::target(core, elf, active_ram_region);

    let frames = symbolicate::frames(&unwind.raw_frames, settings.current_dir, elf);

    let contains_exception = unwind
        .raw_frames
        .iter()
        .any(|raw_frame| raw_frame.is_exception());

    let print_backtrace = settings.backtrace == BacktraceOptions::Always
        || unwind.outcome == Outcome::StackOverflow
        || unwind.corrupted
        || contains_exception;

    if settings.backtrace_limit == 0 {
        let frames_number = &frames.len();
        settings.backtrace_limit = *frames_number as u32;
    }
    if print_backtrace && settings.backtrace_limit > 0 {
        pp::backtrace(&frames, settings);

        if unwind.corrupted {
            log::warn!("call stack was corrupted; unwinding could not be completed");
        }
        if let Some(err) = unwind.processing_error {
            log::error!(
                "error occurred during backtrace creation: {:?}\n               \
                         the backtrace may be incomplete.",
                err
            );
        }
    }

    Ok(unwind.outcome)
}

/// Target program outcome
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Outcome {
    HardFault,
    Ok,
    StackOverflow,
}

impl Outcome {
    pub(crate) fn log(&self) {
        match self {
            Outcome::StackOverflow => {
                log::error!("the program has overflowed its stack");
            }
            Outcome::HardFault => {
                log::error!("the program panicked");
            }
            Outcome::Ok => {
                log::info!("device halted without error");
            }
        }
    }
}

/// Converts `Outcome` to an exit code.
impl From<Outcome> for i32 {
    fn from(outcome: Outcome) -> i32 {
        match outcome {
            Outcome::HardFault | Outcome::StackOverflow => crate::SIGABRT,
            Outcome::Ok => 0,
        }
    }
}
