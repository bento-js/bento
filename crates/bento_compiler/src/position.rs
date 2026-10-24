/// One-based line and column of a byte offset, counting characters within the line.
pub(crate) fn line_column(source: &str, offset: usize) -> (u32, u32) {
    let before = &source[..offset.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let column = before[line_start..].chars().count() + 1;
    (u32::try_from(line).unwrap_or(u32::MAX), u32::try_from(column).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::line_column;

    #[test]
    fn counts_lines_and_characters() {
        let source = "ab\ncdé f\n";
        assert_eq!(line_column(source, 0), (1, 1));
        assert_eq!(line_column(source, 3), (2, 1));
        assert_eq!(line_column(source, source.find('f').unwrap()), (2, 5));
        assert_eq!(line_column(source, source.len()), (3, 1));
    }
}
