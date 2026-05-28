//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT command registry

pub mod basic;
pub mod cip;
pub mod http;
pub mod params;

pub use params::CsvParams;

use crate::at_interface::fsm::FsmHandle;
use std::future::Future;
use std::pin::Pin;

pub type CommandExecFuture<'a> = Pin<Box<dyn Future<Output = CommandOutcome> + Send + 'a>>;
pub type CommandExecutor = for<'a> fn(&'a FsmHandle, &'a params::CsvParams<'a>) -> CommandExecFuture<'a>;

pub enum CommandOutcome {
    Ok,
    Error,
    WithData(String),
    PendingInput,
}

#[derive(Clone, Copy, Debug)]
pub struct CommandHandler {
    pub usage: &'static str,
    pub param_count: usize,
    pub required_non_empty_indices: &'static [usize],
    pub execute: CommandExecutor,
}

pub type PublishedCommand = (&'static str, &'static CommandHandler);

macro_rules! command_spec {
    (
        $signature:expr,
        $usage:expr,
        $param_count:expr,
        $required_non_empty_indices:expr,
        $execute:path
    ) => {
        (
            $signature,
            &$crate::at_interface::commands::CommandHandler {
                usage: $usage,
                param_count: $param_count,
                required_non_empty_indices: $required_non_empty_indices,
                execute: |fsm, params| Box::pin($execute(fsm, params)),
            },
        )
    };
}

pub(crate) use command_spec;

pub struct CommandRegistry;

static CORE_COMMANDS: &[&[PublishedCommand]] = &[
    basic::COMMANDS,
    cip::COMMANDS,
    http::COMMANDS,
];

pub fn build_command_registry() -> CommandRegistry {
    CommandRegistry
}

fn find_published_command(signature: &str) -> Option<&'static CommandHandler> {
    for published in CORE_COMMANDS {
        for (candidate, command) in *published {
            if *candidate == signature {
                return Some(*command);
            }
        }
    }

    for publication in crate::publication::builtin_at_publications_slice() {
        let Some(at_commands) = publication.at_commands.as_ref() else {
            continue;
        };

        for (candidate, command) in at_commands.install {
            if *candidate == signature {
                return Some(*command);
            }
        }
    }

    None
}

pub fn parse_command_params<'a>(
    cmd: &CommandHandler,
    params_str: &'a str,
) -> Option<params::CsvParams<'a>> {
    if cmd.param_count == 0 {
        return params::CsvParams::parse_required(params_str, 0);
    }

    if cmd.required_non_empty_indices.is_empty() {
        params::CsvParams::parse_required(params_str, cmd.param_count)
    } else {
        params::CsvParams::parse_required_non_empty(
            params_str,
            cmd.param_count,
            cmd.required_non_empty_indices,
        )
    }
}

pub fn lookup_command_in_registry<'registry, 'input>(
    _registry: &'registry CommandRegistry,
    input: &'input str,
) -> Option<(&'registry CommandHandler, &'input str, bool)> {
    if input.is_empty() {
        let cmd = find_published_command("")?;
        return Some((cmd, "", false));
    }

    let lower = input.to_lowercase();
    if !lower.starts_with("at") {
        return None;
    }

    let (stripped, print_usage) = if lower.ends_with('?') {
        (&input[..input.len() - 1], true)
    } else {
        (input, false)
    };

    if let Some(eq_pos) = stripped.find('=') {
        let sig = stripped[..eq_pos].to_lowercase();
        let params = &stripped[eq_pos + 1..];
        if let Some(cmd) = find_published_command(sig.as_str()) {
            return Some((cmd, params, print_usage));
        }
    } else {
        let sig = stripped.to_lowercase();
        if let Some(cmd) = find_published_command(sig.as_str()) {
            return Some((cmd, "", print_usage));
        }
    }

    None
}

pub async fn execute_registered_command(
    command: &CommandHandler,
    fsm: &FsmHandle,
    params: &params::CsvParams<'_>,
) -> CommandOutcome {
    (command.execute)(fsm, params).await
}
