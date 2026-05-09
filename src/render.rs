//! Pure rendering primitives and composition.
//!
//! Sub-modules are the building blocks of the camembert view; they have no
//! knowledge of the terminal or of user input. The TUI loop wires them up.

pub mod cheese_spinner;
pub mod layout;
pub mod mouse_target;
pub mod palette;
pub mod renderer;
