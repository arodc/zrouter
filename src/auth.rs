pub fn verify_api_key(provided: Option<&str>, expected: &Option<String>) -> bool {
    match expected {
        None => true,
        Some(expected_key) => match provided {
            Some(provided_key) => constant_time_eq(provided_key.as_bytes(), expected_key.as_bytes()),
            None => false,
        },
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // Still do a comparison to avoid timing side-channel on length
        let _ = a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y));
        false
    } else {
        let mut result = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            result |= x ^ y;
        }
        result == 0
    }
}
