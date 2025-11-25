use lsp_types::Position;

/// Convert 1-based line/character to 0-based LSP Position
pub fn to_lsp_position(line: u32, character: u32) -> Position {
    Position {
        line: line.saturating_sub(1),
        character: character.saturating_sub(1),
    }
}

/// Convert 0-based LSP Position to 1-based line/character
pub fn from_lsp_position(position: Position) -> (u32, u32) {
    (position.line + 1, position.character + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_lsp_position() {
        // 1-based (1,1) should become 0-based (0,0)
        let pos = to_lsp_position(1, 1);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        // 1-based (10,5) should become 0-based (9,4)
        let pos = to_lsp_position(10, 5);
        assert_eq!(pos.line, 9);
        assert_eq!(pos.character, 4);
    }

    #[test]
    fn test_from_lsp_position() {
        // 0-based (0,0) should become 1-based (1,1)
        let (line, char) = from_lsp_position(Position { line: 0, character: 0 });
        assert_eq!(line, 1);
        assert_eq!(char, 1);

        // 0-based (9,4) should become 1-based (10,5)
        let (line, char) = from_lsp_position(Position { line: 9, character: 4 });
        assert_eq!(line, 10);
        assert_eq!(char, 5);
    }

    #[test]
    fn test_edge_cases() {
        // Test zero values (should not underflow)
        let pos = to_lsp_position(0, 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }
}
