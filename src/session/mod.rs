//! Background work sessions: detached tmux running `claude remote-control`,
//! locally or (resilient to ssh drops) on a remote host.

pub mod control;
pub mod start;
pub mod state;

pub use control::{inspect, list, stop};
pub use start::{StartOptions, build_mcp_tunnel, start};
pub use state::{SessionState, session_id};
