use std::io::{Cursor, ErrorKind};

#[test]
fn null_required_column_type_returns_invalid_data() {
    let bytes = include_bytes!("data/msi-null-column-type.msi");
    let error = msi::Package::open(Cursor::new(bytes.as_slice()))
        .err()
        .expect("malformed schema metadata must be rejected");
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}
