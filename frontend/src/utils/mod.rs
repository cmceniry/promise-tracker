pub mod diff;
pub mod validation;

pub use diff::{
    check_filename_diff, compare_contracts, compute_side_by_side_diff, DiffLineType, SideBySideDiff,
};
pub use validation::{
    generate_unique_random_filename, validate_contract_content, validate_filename,
};
