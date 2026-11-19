// SPDX-License-Identifier: MIT OR Apache-2.0
//! LP-0001 gate guest: one actor per run.

lee_core::actor_entrypoint!(raw gate_program::handle_message);
