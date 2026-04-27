pub mod glob_handler;
pub mod grep_handler;
pub mod handler;
pub mod post_handler;
pub mod read_handler;
pub use handler::handle;
pub use handler::handle_gemini;
pub use handler::handle_qwen;
pub use post_handler::handle_post;
pub use post_handler::handle_post_gemini;
pub use post_handler::handle_post_qwen;

/// Maximum stdin payload size accepted by hook handlers (10 MB).
pub const MAX_STDIN_BYTES: usize = 10 * 1024 * 1024;
