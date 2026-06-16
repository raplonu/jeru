//! Background work sessions: detached tmux running `claude remote-control`,
//! locally or (resilient to ssh drops) on a remote host.

mod conflicts;
pub mod control;
pub mod start;
pub mod state;

pub use control::{info, info_all, inspect, list, print_session_info, stop, stop_all};
pub use start::{StartOptions, build_mcp_tunnel, start};
pub use state::{SessionState, session_id};
