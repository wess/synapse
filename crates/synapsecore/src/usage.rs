//! Token savings, recorded per session, recall, and read.
//!
//! The baseline is the same memory kept in an instruction file that every
//! session loads whole. A session is credited with everything it could reach
//! and charged for what was actually sent, plus Synapse's own tool definitions,
//! guidance, and response framing. Small stores can come out negative.
//!
//! Figures are a floor: each token counts once per session even though context
//! is re-read every turn. Rows hold sizes only, never text, and don't sync.

mod format;
mod price;
mod record;
mod report;

pub use format::{approximately, money, thousands};
pub use price::{DEFAULTPRICE, dollars, price, setprice};
pub use record::{Entry, Kind, bodies, hooked, record, standing};
pub use report::{Day, Report, Totals, report};

/// Characters per token. An estimate, not a tokenizer.
pub const PERTOKEN: i64 = 4;

pub fn tokens(chars: i64) -> i64 {
    chars / PERTOKEN
}
