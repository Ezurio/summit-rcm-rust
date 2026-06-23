//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Shared comma-separated AT parameter parsing helpers.

use std::str::FromStr;

pub struct CsvParams<'a> {
    input: &'a str,
    all_parts: Vec<&'a str>,
    parts: Vec<&'a str>,
}

impl<'a> CsvParams<'a> {
    pub fn parse_required(input: &'a str, required_parts: usize) -> Option<Self> {
        let all_parts: Vec<&str> = if input.is_empty() {
            Vec::new()
        } else {
            input.split(',').collect()
        };
        if all_parts.len() < required_parts {
            return None;
        }

        let parts = if required_parts == 0 {
            all_parts.clone()
        } else {
            input.splitn(required_parts, ',').collect()
        };

        Some(Self {
            input,
            all_parts,
            parts,
        })
    }

    pub fn parse(input: &'a str, expected_parts: usize) -> Option<Self> {
        Self::parse_required(input, expected_parts)
    }

    pub fn parse_required_non_empty(
        input: &'a str,
        required_parts: usize,
        required_non_empty_indices: &[usize],
    ) -> Option<Self> {
        let parsed = Self::parse_required(input, required_parts)?;
        for &index in required_non_empty_indices {
            if index >= required_parts {
                return None;
            }
            if parsed.trimmed(index).is_empty() {
                return None;
            }
        }
        Some(parsed)
    }

    pub fn trimmed(&self, index: usize) -> &'a str {
        self.parts[index].trim()
    }

    pub fn raw_input(&self) -> &'a str {
        self.input
    }

    pub fn iter_raw_parameters(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.all_parts.iter().copied()
    }

    pub fn parameter_count(&self) -> usize {
        self.all_parts.len()
    }

    pub fn iter_parameters(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.all_parts.iter().map(|value| value.trim())
    }

    pub fn parse_value<T: FromStr>(&self, index: usize) -> Option<T> {
        self.trimmed(index).parse().ok()
    }

    pub fn parse_or<T: FromStr>(&self, index: usize, default: T) -> T {
        self.trimmed(index).parse().ok().unwrap_or(default)
    }
}
