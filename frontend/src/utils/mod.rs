pub mod api;
pub mod diff;
pub mod download;
pub mod export;
pub mod validation;

pub use api::{fetch_server_contract, push_contract_to_server};
pub use diff::{
    check_filename_diff, compare_contracts, compute_side_by_side_diff, DiffLineType, SideBySideDiff,
};
#[allow(unused_imports)]
pub use download::download_html;
#[allow(unused_imports)]
pub use export::{escape_html, generated_at, html_document, slugify};
pub use validation::{
    generate_unique_random_filename, validate_contract_content, validate_filename,
};
