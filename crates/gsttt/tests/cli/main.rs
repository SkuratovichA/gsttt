//! Runs the real `gsttt` binary and checks what it prints, one module per part of the CLI spec.
#![cfg(test)]

mod colour;
mod config;
mod hook;
mod impact;
mod index;
mod init;
mod logs;
mod panics;
mod paths;
mod questions;
mod repository;
mod root;
mod schema;
mod support;
mod trust;
mod usage;
