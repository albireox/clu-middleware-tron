/*
 *  @Author: José Sánchez-Gallego (gallegoj@uw.edu)
 *  @Date: 2025-12-04
 *  @Filename: lib.rs
 *  @License: BSD 3-clause (http://www.opensource.org/licenses/BSD-3-Clause)
 */

//! A library and service to act as middleware between Tron actors and a RabbitMQ message broker.
//!
//! Refer to the [README](https://github.com/albireox/clu-middleware-tron/blob/main/README.md) in
//! the [clu-middleware-tron](https://github.com/albireox/clu-middleware-tron) repository for more
//! information.

pub mod parser;
pub mod rabbitmq;
pub mod tcp;
pub mod tool;
