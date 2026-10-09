use msi::media::{MediaRow, cabinet_for, validate_media_rows};

#[test]
fn media_boundaries_and_compression_overrides_are_explicit() {
    let rows = validate_media_rows(&[
        MediaRow { last_sequence: 4, cabinet: Some("second.cab".into()) },
        MediaRow { last_sequence: 2, cabinet: Some("#first.cab".into()) },
    ])
    .unwrap();
    assert_eq!(
        cabinet_for(2, 0, 2, &rows).unwrap().as_deref(),
        Some("#first.cab")
    );
    assert_eq!(
        cabinet_for(3, 0x4000, 0, &rows).unwrap().as_deref(),
        Some("second.cab")
    );
    assert_eq!(cabinet_for(3, 0x2000, 2, &rows).unwrap(), None);
    assert!(cabinet_for(3, 0x6000, 2, &rows).is_err());
    assert!(cabinet_for(0, 0, 2, &rows).is_err());
    assert!(cabinet_for(5, 0, 2, &rows).is_err());
    assert!(
        cabinet_for(1, 0, 2, &[rows[0].clone(), rows[0].clone()]).is_err()
    );
    assert!(
        validate_media_rows(&[MediaRow { last_sequence: 0, cabinet: None }])
            .is_ok()
    );
    assert!(validate_media_rows(&[rows[0].clone(), rows[0].clone()]).is_err());
    assert!(
        validate_media_rows(&[MediaRow { last_sequence: -1, cabinet: None }])
            .is_err()
    );
}

#[cfg(feature = "media")]
mod codecs {
    use msi::{
        Package, PackageType,
        media::{Error, Resolver, read_payload},
    };
    use std::io::{self, Cursor, Write};
    struct Media(Vec<u8>);
    impl Resolver for Media {
        fn resolve(&mut self, _: &str, _: u64) -> io::Result<Vec<u8>> {
            Ok(self.0.clone())
        }
    }
    fn database() -> Package<Cursor<Vec<u8>>> {
        Package::create(PackageType::Installer, Cursor::new(Vec::new()))
            .unwrap()
    }
    fn cabinet(compression: cabinet::WriteCompression) -> Vec<u8> {
        let mut builder = cabinet::CabinetBuilder::new(compression);
        builder.add_file("Payload", b"payload bytes").unwrap();
        let mut output = Cursor::new(Vec::new());
        builder.write(&mut output).unwrap();
        output.into_inner()
    }
    #[test]
    fn embedded_external_and_all_native_codecs_decode_exact_bytes() {
        for compression in [
            cabinet::WriteCompression::None,
            cabinet::WriteCompression::MsZip,
            cabinet::WriteCompression::Lzx { window_order: 15 },
            cabinet::WriteCompression::Quantum { level: 4, window_order: 15 },
        ] {
            let bytes = cabinet(compression);
            let mut package = database();
            {
                let mut stream = package.write_stream("payload.cab").unwrap();
                stream.write_all(&bytes).unwrap();
                stream.flush().unwrap();
            }
            for reference in ["#payload.cab", "payload.cab"] {
                assert_eq!(
                    read_payload(
                        &mut package,
                        "Payload",
                        "ignored",
                        13,
                        Some(reference),
                        &mut Media(bytes.clone()),
                        4096
                    )
                    .unwrap(),
                    b"payload bytes"
                );
            }
        }
    }
    #[test]
    fn reserved_header_folder_and_data_bytes_are_bounded_and_supported() {
        let mut bytes = cabinet(cabinet::WriteCompression::None);
        let old_size = bytes.len();
        let file_offset =
            u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]])
                as usize;
        let data_offset =
            u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]])
                as usize;
        bytes.splice(36..36, [2, 0, 1, 1, 0xa, 0xb]);
        bytes.insert(50, 0xc);
        bytes.insert(data_offset + 7 + 8, 0xd);
        bytes[8..12].copy_from_slice(&((old_size + 8) as u32).to_le_bytes());
        bytes[16..20]
            .copy_from_slice(&((file_offset + 7) as u32).to_le_bytes());
        bytes[30..32].copy_from_slice(&4u16.to_le_bytes());
        bytes[42..46]
            .copy_from_slice(&((data_offset + 7) as u32).to_le_bytes());
        // The reserve bytes are opaque data, never a signature/trust assertion.
        assert_eq!(
            read_payload(
                &mut database(),
                "Payload",
                "ignored",
                13,
                Some("p.cab"),
                &mut Media(bytes.clone()),
                4096
            )
            .unwrap(),
            b"payload bytes"
        );
        bytes[36..38].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(
            read_payload(
                &mut database(),
                "Payload",
                "ignored",
                13,
                Some("p.cab"),
                &mut Media(bytes),
                4096
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_indexes_spanning_and_declared_limits_fail_before_decode() {
        let original = cabinet(cabinet::WriteCompression::None);
        let mut corrupted = original.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 1;
        assert!(matches!(
            read_payload(
                &mut database(),
                "Payload",
                "ignored",
                13,
                Some("p.cab"),
                &mut Media(corrupted),
                4096
            ),
            Err(Error::Cabinet(_))
        ));
        let mut package = database();
        assert!(matches!(
            read_payload(
                &mut package,
                "Payload",
                "ignored",
                13,
                Some("p.cab"),
                &mut Media(original.clone()),
                12
            ),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            read_payload(
                &mut package,
                "Payload",
                "ignored",
                12,
                Some("p.cab"),
                &mut Media(original.clone()),
                4096
            ),
            Err(Error::Integrity(_))
        ));
        for offset in [26, 28] {
            let mut bytes = original.clone();
            bytes[offset..offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
            assert!(
                read_payload(
                    &mut package,
                    "Payload",
                    "ignored",
                    13,
                    Some("p.cab"),
                    &mut Media(bytes),
                    4096
                )
                .is_err()
            );
        }
        for flags in [1u16, 2] {
            let mut bytes = original.clone();
            bytes[30..32].copy_from_slice(&flags.to_le_bytes());
            assert!(matches!(
                read_payload(
                    &mut package,
                    "Payload",
                    "ignored",
                    13,
                    Some("p.cab"),
                    &mut Media(bytes),
                    4096
                ),
                Err(Error::Unsupported(_))
            ));
        }
        assert_eq!(
            read_payload(
                &mut package,
                "Payload",
                "source/file",
                13,
                None,
                &mut Media(b"payload bytes".to_vec()),
                13
            )
            .unwrap(),
            b"payload bytes"
        );
        assert!(matches!(
            read_payload(
                &mut package,
                "Payload",
                "source/file",
                13,
                None,
                &mut Media(vec![0; 14]),
                13
            ),
            Err(Error::Integrity(_))
        ));
    }
}

#[test]
fn borrowed_file_relationships_are_bounded_and_validate_sparse_sequences() {
    use msi::media::{Error, FileSequence, Limits, resolve_files};
    let rows = [
        MediaRow { last_sequence: 0, cabinet: None },
        MediaRow { last_sequence: 10, cabinet: Some("#payload.cab".into()) },
    ];
    let files = [
        FileSequence { id: "second", sequence: 9, attributes: 0x2000 },
        FileSequence { id: "first", sequence: 3, attributes: 0 },
    ];
    let result = resolve_files(&files, &rows, 2, &Limits::default()).unwrap();
    assert_eq!(result[0].id, "first");
    assert_eq!(result[0].cabinet, Some("#payload.cab"));
    assert!(std::ptr::eq(
        result[0].cabinet.unwrap().as_ptr(),
        rows[1].cabinet.as_deref().unwrap().as_ptr()
    ));
    assert_eq!(result[1].cabinet, None);
    for duplicate in [
        FileSequence { id: "first", sequence: 9, attributes: 0 },
        FileSequence { id: "second", sequence: 3, attributes: 0 },
    ] {
        assert!(
            resolve_files(
                &[files[1], duplicate],
                &rows,
                2,
                &Limits::default()
            )
            .is_err()
        );
    }
    for sequence in [-1, 0, 11] {
        assert!(
            resolve_files(
                &[FileSequence { sequence, ..files[1] }],
                &rows,
                2,
                &Limits::default()
            )
            .is_err()
        );
    }
    for cabinet in
        [Some("".into()), Some("#".into()), Some("../cab.cab".into()), None]
    {
        assert!(
            resolve_files(
                &[files[1]],
                &[MediaRow { last_sequence: 10, cabinet }],
                2,
                &Limits::default()
            )
            .is_err()
        );
    }
    for limits in [
        Limits { max_entries: 1, ..Limits::default() },
        Limits { max_metadata_bytes: 0, ..Limits::default() },
        Limits { max_scratch_bytes: 0, ..Limits::default() },
    ] {
        assert!(matches!(
            resolve_files(&files, &rows, 2, &limits),
            Err(Error::Limit(_))
        ));
    }
}

#[test]
fn cabinet_references_obey_utf16_media_column_bounds() {
    assert!(
        validate_media_rows(&[MediaRow {
            last_sequence: 1,
            cabinet: Some(format!("{}.cab", "x".repeat(252)))
        }])
        .is_err()
    );
    assert!(
        validate_media_rows(&[MediaRow {
            last_sequence: 1,
            cabinet: Some(format!("{}.cab", "x".repeat(251)))
        }])
        .is_ok()
    );
    assert!(
        validate_media_rows(&[MediaRow {
            last_sequence: 1,
            cabinet: Some(format!("{}.cab", "𐐀".repeat(126)))
        }])
        .is_err()
    );
}
