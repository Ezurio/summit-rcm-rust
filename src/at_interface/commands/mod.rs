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
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;

pub type CommandExecFuture<'a> = Pin<Box<dyn Future<Output = CommandOutcome> + Send + 'a>>;
pub type CommandExecutor = for<'a> fn(&'a FsmHandle, &'a params::CsvParams<'a>) -> CommandExecFuture<'a>;

pub enum CommandOutcome {
    Ok,
    Error,
    WithData(String),
    WithDataError(String),
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

pub(crate) static CORE_COMMANDS: &[&[PublishedCommand]] = &[
    basic::COMMANDS,
    cip::COMMANDS,
    http::COMMANDS,
];

static COMMAND_REGISTRY: LazyLock<HashMap<&'static str, &'static CommandHandler>> = LazyLock::new(|| {
    let mut by_signature = HashMap::new();

    for published in CORE_COMMANDS {
        for (candidate, command) in *published {
            by_signature.entry(*candidate).or_insert(*command);
        }
    }

    for publication in crate::publication::builtin_plugin_publications() {
        let Some(at_commands) = publication.at_commands.as_ref() else {
            continue;
        };

        for (candidate, command) in *at_commands {
            by_signature.entry(*candidate).or_insert(*command);
        }
    }

    by_signature
});

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

pub fn lookup_command_in_registry(
    input: &str,
) -> Option<(&'static CommandHandler, &str, bool)> {
    if input.is_empty() {
        let cmd = *COMMAND_REGISTRY.get("")?;
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
        if let Some(cmd) = COMMAND_REGISTRY.get(sig.as_str()) {
            return Some((cmd, params, print_usage));
        }
    } else {
        let sig = stripped.to_lowercase();
        if let Some(cmd) = COMMAND_REGISTRY.get(sig.as_str()) {
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
