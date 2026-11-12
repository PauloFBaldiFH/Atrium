// SPDX-License-Identifier: GPL-3.0-or-later
//! Atrium entry point. Roles (`serve`, `import`, `worker`, `panel`) arrive in later tasks.

fn main() {
    println!("atrium {}", env!("CARGO_PKG_VERSION"));
}
