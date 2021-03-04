pub fn parse_hex_string(value: &str) -> Result<Vec<u8>, hex::FromHexError> {
    let without_spaces = value.replace(' ', "");
    hex::decode(without_spaces)
}
