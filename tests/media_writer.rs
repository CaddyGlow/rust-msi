#![cfg(feature = "media")]
use msi::media::{self, CabinetSpec, Error, Layout, Limits, Sink};
use std::io::{self, Cursor, Read, Write};

#[test]
fn prepares_deterministic_mixed_cabinets_and_embedded_streams() {
    let files = [
        ("First", "données/first.txt", b"first".as_slice()),
        ("Second", "second.txt", b"second".as_slice()),
    ];
    let layout = Layout::Cabinets {
        cabinets: vec![
            CabinetSpec {
                name: "inside.cab".into(),
                file_count: 1,
                embedded: true,
            },
            CabinetSpec {
                name: "media/外部.cab".into(),
                file_count: 1,
                embedded: false,
            },
        ],
    };
    let first =
        media::prepare(&files, &layout, "répertoire", &Limits::default())
            .unwrap();
    let second =
        media::prepare(&files, &layout, "répertoire", &Limits::default())
            .unwrap();
    assert_eq!(first.embedded(), second.embedded());
    assert_eq!(first.external(), second.external());
    assert_eq!(
        first.rows(),
        &[(1, Some("#inside.cab".into())), (2, Some("media/外部.cab".into()))]
    );
    let mut package = msi::Package::create(
        msi::PackageType::Installer,
        Cursor::new(Vec::new()),
    )
    .unwrap();
    first.write_embedded(&mut package).unwrap();
    let mut bytes = Vec::new();
    package
        .read_stream("inside.cab")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, first.embedded()[0].1);
    assert!(first.write_embedded(&mut package).is_err());
}

#[test]
fn validates_partitions_paths_and_exact_resource_budgets() {
    let files = [("ID", "déjà/file.txt", b"abc".as_slice())];
    let mut limits = Limits {
        max_file_bytes: 3,
        max_total_bytes: 3,
        max_output_bytes: 3,
        max_scratch_bytes: 6,
        ..Limits::default()
    };
    let loose =
        media::prepare(&files, &Layout::Loose, "répertoire", &limits).unwrap();
    assert_eq!(
        loose.external()[0],
        ("répertoire/déjà/file.txt".into(), b"abc".to_vec())
    );
    limits.max_scratch_bytes = 5;
    assert!(matches!(
        media::prepare(&files, &Layout::Loose, "directory", &limits),
        Err(Error::Limit(_))
    ));
    for name in ["../escape", "/absolute", "a//b", "C:drive", "COM¹.txt"] {
        assert!(
            media::prepare(
                &[("ID", name, b"a")],
                &Layout::Loose,
                "directory",
                &Limits::default()
            )
            .is_err()
        );
    }
    for count in [0, 2] {
        let layout = Layout::Cabinets {
            cabinets: vec![CabinetSpec {
                name: "one.cab".into(),
                file_count: count,
                embedded: true,
            }],
        };
        assert!(
            media::prepare(&files, &layout, "directory", &Limits::default())
                .is_err()
        );
    }
    assert_eq!(media::summary_word_count(0b1101, true), 0b1111);
    assert_eq!(media::summary_word_count(0b1111, false), 0b1101);
}

struct Output {
    bytes: usize,
    fail_write: bool,
    fail_flush: bool,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.fail_write && self.bytes > 0 {
            return Err(io::Error::other("write"));
        }
        let count = bytes.len().min(1);
        self.bytes += count;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.fail_flush { Err(io::Error::other("flush")) } else { Ok(()) }
    }
}
struct Destination {
    creates: usize,
    finalized: Vec<String>,
    fail: &'static str,
}
impl Sink for Destination {
    type Writer = Output;
    fn create(&mut self, _: &str) -> io::Result<Output> {
        self.creates += 1;
        if self.creates == 2 && self.fail == "create" {
            return Err(io::Error::other("create"));
        }
        Ok(Output {
            bytes: 0,
            fail_write: self.creates == 2 && self.fail == "write",
            fail_flush: self.creates == 2 && self.fail == "flush",
        })
    }
    fn finish(&mut self, name: &str, _: Output) -> io::Result<()> {
        if self.creates == 2 && self.fail == "finish" {
            return Err(io::Error::other("finish"));
        }
        self.finalized.push(name.into());
        Ok(())
    }
}
#[test]
fn accounts_partial_writes_flush_and_finalization_failures() {
    let artifacts =
        [("first.cab".into(), vec![1; 3]), ("second.cab".into(), vec![2; 3])];
    for (failure, bytes) in
        [("create", 3), ("write", 4), ("flush", 6), ("finish", 6)]
    {
        let mut sink =
            Destination { creates: 0, finalized: Vec::new(), fail: failure };
        match media::write_media(&artifacts, &mut sink, &Limits::default())
            .unwrap_err()
        {
            Error::Emission {
                completed,
                incomplete,
                bytes_written,
                source,
            } => {
                assert_eq!(completed, ["first.cab"]);
                assert_eq!(incomplete, "second.cab");
                assert_eq!(bytes_written, bytes);
                assert!(matches!(*source, Error::Io(_)));
            }
            error => panic!("unexpected {error}"),
        }
        assert_eq!(sink.finalized, ["first.cab"]);
    }
    let mut sink = Destination { creates: 0, finalized: Vec::new(), fail: "" };
    let report =
        media::write_media(&artifacts, &mut sink, &Limits::default()).unwrap();
    assert_eq!(report.bytes_written, 6);
    assert_eq!(report.completed.len(), 2);
}

#[test]
fn emission_preflights_collisions_and_limits_before_opening_sink() {
    let mut sink = Destination { creates: 0, finalized: Vec::new(), fail: "" };
    for artifacts in [
        vec![("a".into(), vec![]), ("a/b".into(), vec![])],
        vec![("Données".into(), vec![]), ("données".into(), vec![])],
    ] {
        assert!(
            media::write_media(&artifacts, &mut sink, &Limits::default())
                .is_err()
        );
    }
    let limits = Limits { max_output_bytes: 2, ..Limits::default() };
    assert!(
        media::write_media(&[("a".into(), vec![1; 3])], &mut sink, &limits)
            .is_err()
    );
    assert_eq!(sink.creates, 0);
}

#[test]
fn cabinet_output_and_codec_scratch_have_exact_boundaries() {
    let files = [("ID", "file.txt", b"abc".as_slice())];
    let prepared = media::prepare(
        &files,
        &Layout::Embedded,
        "directory",
        &Limits::default(),
    )
    .unwrap();
    let size = prepared.embedded()[0].1.len() as u64;
    let mut limits = Limits {
        max_output_bytes: size,
        max_scratch_bytes: size + 3 + 32768,
        ..Limits::default()
    };
    assert_eq!(
        media::prepare(&files, &Layout::Embedded, "directory", &limits)
            .unwrap()
            .embedded(),
        prepared.embedded()
    );
    limits.max_output_bytes -= 1;
    assert!(matches!(
        media::prepare(&files, &Layout::Embedded, "directory", &limits),
        Err(Error::Limit(_))
    ));
    limits.max_output_bytes = size;
    limits.max_scratch_bytes -= 1;
    assert!(matches!(
        media::prepare(&files, &Layout::Embedded, "directory", &limits),
        Err(Error::Limit(_))
    ));
}

#[test]
fn database_identifiers_are_not_windows_destination_names() {
    let files = [("CON", "ordinary.txt", b"data".as_slice())];
    let prepared = media::prepare(
        &files,
        &Layout::Embedded,
        "directory",
        &Limits::default(),
    )
    .unwrap();
    let mut cabinet =
        cabinet::Cabinet::new(Cursor::new(&prepared.embedded()[0].1)).unwrap();
    assert_eq!(cabinet.read_file_bytes("CON", 4).unwrap(), b"data");
}

#[test]
fn cabinet_reference_representability_is_checked_during_preparation() {
    let files = [("ID", "ordinary.txt", b"data".as_slice())];
    for (name, embedded) in [
        (format!("{}.cab", "é".repeat(32)), true),
        (format!("{}.cab", "a".repeat(256)), false),
        (format!("{}.cab", "😀".repeat(126)), false),
    ] {
        let layout = Layout::Cabinets {
            cabinets: vec![CabinetSpec { name, embedded, file_count: 1 }],
        };
        assert!(matches!(
            media::prepare(&files, &layout, "directory", &Limits::default()),
            Err(Error::Invalid(_))
        ));
    }
    let valid =
        Layout::ExternalCabinet { name: format!("{}.cab", "a".repeat(251)) };
    assert!(
        media::prepare(&files, &valid, "directory", &Limits::default())
            .is_ok()
    );
}
