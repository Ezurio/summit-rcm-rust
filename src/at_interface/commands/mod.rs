//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT command registry

pub mod basic;
pub mod cip;
pub mod http;

use crate::at_interface::fsm::FsmHandle;
use crate::publication::PublishedAtCommand;
use futures_util::future::BoxFuture;

pub trait Command: Send + Sync {
    fn signature(&self) -> &str;
    fn name(&self) -> &str;
    fn usage(&self) -> &str;
    fn execute<'a>(&'a self, fsm: &'a FsmHandle, params: &'a str) -> BoxFuture<'a, (bool, String)>;
}

fn build_core_commands() -> Vec<Box<dyn Command>> {
    vec![
        Box::new(basic::CommunicationCheck),
        Box::new(basic::Empty),
        Box::new(basic::Ping),
        Box::new(basic::ATEchoEnable),
        Box::new(basic::ATEchoDisable),
        Box::new(cip::CIPStart),
        Box::new(cip::CIPClose),
        Box::new(cip::CIPSend),
        Box::new(cip::CIPConfigureSSL),
        Box::new(http::HTTPConfigureTransaction),
        Box::new(http::HTTPExecuteTransaction),
        Box::new(http::HTTPAddHeader),
        Box::new(http::HTTPEnableResponseHeader),
        Box::new(http::HTTPClearConfiguration),
        Box::new(http::HTTPConfigureSSL),
    ]
}

fn core_published_commands() -> &'static [PublishedAtCommand] {
    static COMMANDS: &[PublishedAtCommand] = &[
        PublishedAtCommand::new("at", "Communication Check"),
        PublishedAtCommand::new("", "Empty"),
        PublishedAtCommand::new("at+ping", "Ping"),
        PublishedAtCommand::new("ate1", "AT Echo Enable"),
        PublishedAtCommand::new("ate0", "AT Echo Disable"),
        PublishedAtCommand::new("at+cipstart", "CIP Start"),
        PublishedAtCommand::new("at+cipclose", "CIP Close"),
        PublishedAtCommand::new("at+cipsend", "CIP Send"),
        PublishedAtCommand::new("at+cipssl", "CIP Configure SSL"),
        PublishedAtCommand::new("at+httpconf", "HTTP Configure Transaction"),
        PublishedAtCommand::new("at+httpexe", "HTTP Execute Transaction"),
        PublishedAtCommand::new("at+httpaddhdr", "HTTP Add Header"),
        PublishedAtCommand::new("at+httprshdr", "HTTP Enable Response Header"),
        PublishedAtCommand::new("at+httpclr", "HTTP Clear Configuration"),
        PublishedAtCommand::new("at+httpssl", "HTTP Configure SSL"),
    ];
    COMMANDS
}

pub fn build_commands() -> Vec<Box<dyn Command>> {
    let mut cmds = build_core_commands();
    for publication in crate::publication::builtin_at_publications() {
        if let Some(at_commands) = publication.at_commands.as_ref() {
            (at_commands.install)(&mut cmds);
        }
    }

    for cmd in &cmds {
        let _ = cmd.name();
    }

    let _published = published_commands();

    cmds
}

pub fn published_commands() -> Vec<PublishedAtCommand> {
    let mut commands = core_published_commands().to_vec();
    for publication in crate::publication::builtin_at_publications() {
        if let Some(at_commands) = publication.at_commands.as_ref() {
            if let Some(metadata) = at_commands.metadata {
                commands.extend_from_slice(metadata);
            }
        }
    }
    commands
}

pub fn lookup_command<'a>(
    commands: &'a [Box<dyn Command>],
    input: &str,
) -> Option<(&'a dyn Command, String, bool)> {
    if input.is_empty() {
        for cmd in commands {
            if cmd.signature().is_empty() {
                return Some((&**cmd, String::new(), false));
            }
        }
        return None;
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
        let params = stripped[eq_pos + 1..].to_string();
        for cmd in commands {
            if sig == cmd.signature() {
                return Some((&**cmd, params, print_usage));
            }
        }
    } else {
        let sig = stripped.to_lowercase();
        for cmd in commands {
            if sig == cmd.signature() {
                return Some((&**cmd, String::new(), print_usage));
            }
        }
    }
    None
}
