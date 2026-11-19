// SPDX-License-Identifier: MIT OR Apache-2.0
//! LP-0001 reference consumer guest: one actor per run.

lee_core::actor_entrypoint!(raw consumer_program::handle_message);
