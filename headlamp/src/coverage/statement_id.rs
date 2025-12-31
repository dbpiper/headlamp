pub fn statement_id_from_line_col(line: u32, col: u32) -> u64 {
    (u64::from(line) << 32) | u64::from(col)
}

pub fn statement_id_line(statement_id: u64) -> u32 {
    (statement_id >> 32) as u32
}
