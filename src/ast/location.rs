//! Source locations. Lines are 1-based; columns are the lexer's 0-based
//! column + 1. Kept as `i32` because callers compute `line - 1`.

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSourceLocation {
    pub name: String,
    pub line: i32,
    pub column: i32,
    pub end_line: i32,
    pub end_column: i32,
}
